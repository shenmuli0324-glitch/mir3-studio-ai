#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::Arc;

use tauri::{
    ipc::Invoke,
    menu::{Menu, MenuEvent, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    Manager, Runtime, WebviewUrl, WebviewWindowBuilder, Wry,
};

#[cfg(windows)]
use crate::desktop::window::on_page_load;
use crate::desktop::window::{on_download, on_new_window};
use crate::utils::show_main_window;

/// setup app
pub fn setup(app_handle: tauri::AppHandle) {
    // 启动前清扫上次崩溃残留的孤儿 MIR3 AI Core（端口/PID 双重确认，见
    // workflow::sweep_orphan_core），避免新实例一路漂移端口
    crate::service::workflow::sweep_orphan_core(&app_handle);

    // 启动进程监控（tick 检测 dsh 服务状态）
    crate::service::scheduler::start(&app_handle);

    // 开机自启动：已安装且开启 auto_start 时拉起服务
    let app_for_start = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let setting = crate::config::get_store_dat_setting(&app_for_start);
        if !setting.auto_start {
            log::debug!("auto_start disabled, skipping startup");
            return;
        }
        if let Err(e) = crate::service::workflow::start(app_for_start).await {
            log::error!("start failed: {}", e);
        }
    });

    // 命令行集成自愈：已安装且开启时，确保 shim 与 PATH 注册完整
    // （shim 被删除、PATH 条目丢失等情况下自动重建）
    tauri::async_runtime::spawn(async move {
        let setting = crate::config::get_store_dat_setting(&app_handle);
        if !setting.installed || !setting.cli_link_enabled {
            return;
        }
        if let Err(e) = crate::service::cli::ensure(&app_handle) {
            log::warn!("cli link self-heal failed: {e}");
        }
    });
}

/// setup tray
pub fn tray<R: Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    // 使用默认窗口图标
    let icon = app.default_window_icon().unwrap().clone();

    // 构建菜单
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", "打开面板", true, None::<&str>)?,
            &MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?,
        ],
    )?;

    fn handle_menu_event<R: Runtime>(app: &tauri::AppHandle<R>, event: &MenuEvent) {
        match event.id().as_ref() {
            "open" => show_main_window(app),
            "quit" => {
                app.exit(0);
            }
            _ => {}
        }
    }

    fn handle_tray_icon_event<R: Runtime>(tray: &tauri::tray::TrayIcon<R>, event: &TrayIconEvent) {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            ..
        } = event
        {
            show_main_window(tray.app_handle());
        }
    }

    // 构建托盘图标
    let _ = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip(crate::config::brand::get().product_name.as_str())
        .on_menu_event(move |app, event| handle_menu_event(app, &event))
        .on_tray_icon_event(move |tray, event| handle_tray_icon_event(tray, &event))
        .build(app)?;

    Ok(())
}

/// 构建主窗口。
///
/// 主窗口在这里手动创建（不再从 tauri.conf.json 声明）：
/// config 声明的窗口无法挂载 on_download，而内嵌 iframe 的 dsh 页面
/// 触发下载时 WebView2 静默保存、用户零感知，需要接管下载以给出反馈。
pub fn build_main_window(app: &tauri::AppHandle<Wry>) -> tauri::Result<tauri::WebviewWindow<Wry>> {
    let app_handle = app.clone();

    #[cfg(windows)]
    let _notification_handlers_registered = Arc::new(AtomicBool::new(false));
    #[cfg(windows)]
    let notification_handlers_registered_for_page = _notification_handlers_registered.clone();

    let webview_builder =
        WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title(crate::config::brand::get().product_name.as_str())
            .inner_size(1280.0, 840.0)
            .min_inner_size(860.0, 620.0)
            .resizable(true)
            // 无系统标题栏：窗口 chrome 由壳层 ShellNavBar 常驻提供
            // （44px 顶部导航：左侧 iframe 导航控制 + 右侧窗口控制）
            .decorations(false)
            // 恢复 iframe 内 HTML5 拖拽（拖入图片/拖动元素）：
            // Tauri 默认注册 wry drag_drop_handler → WebView2 SetAllowExternalDrop(false)
            // 并注入 IDropTarget 接管拖放，iframe 内拖拽被禁用。
            // 注意不能用 .drag_and_drop(false)：它只设置 tao 窗口层的拖放开关
            // （tauri issue #13761），不影响 webview 层，拖拽依旧失效；
            // disable_drag_drop_handler 才能关掉 wry 的接管（等价于旧配置 dragDropEnabled: false）。
            .disable_drag_drop_handler()
            // 接管内嵌 iframe 的 window.open() / target=_blank 新窗口请求：
            // WebView2 里这类请求走 NewWindowRequested，wry 在没有 handler 时
            // 直接 SetHandled(true) 吞掉（点了没反应）——dshmarket 等预设插件的
            // “源码”按钮在桌面端因此无法跳转（浏览器里正常）。
            // 这里把 http(s) 链接交给系统浏览器打开，其余协议一律拒绝。
            .on_new_window(move |url, features| on_new_window(app_handle.clone(), url, features))
            .on_download(|webview, event| on_download(webview, event));

    #[cfg(windows)]
    let webview_builder = webview_builder.on_page_load(move |webview_window, payload| {
        on_page_load(
            webview_window,
            payload,
            notification_handlers_registered_for_page.clone(),
        )
    });

    // 非 Windows（macOS/Linux）没有 WebView2 的 FrameCreated/ContentLoading 流程，
    // 直接用 Tauri 的 initialization_script_for_all_frames 把兼容桥、通知桥、导航桥
    // 与样式桥注入所有 frame（脚本均带 window.__dsh_*_bridge__ 幂等守卫，重复注入安全）。
    #[cfg(not(windows))]
    let webview_builder = webview_builder
        .initialization_script_for_all_frames(crate::desktop::compat::ABORT_SIGNAL_ANY_SHIM_JS)
        .initialization_script_for_all_frames(crate::desktop::notification::NOTIFICATION_SHIM_JS)
        .initialization_script_for_all_frames(crate::desktop::nav::NAV_SHIM_JS)
        .initialization_script_for_all_frames(crate::desktop::brand::IFRAME_BRAND_JS)
        .initialization_script_for_all_frames(crate::desktop::style::IFRAME_STYLES_JS)
        .initialization_script_for_all_frames(crate::desktop::paste::PASTE_SHIM_JS);

    let webview_window = webview_builder.build()?;

    // 恢复上次的窗口大小/位置/最大化状态（无历史时保持 builder 默认的 1280×840，
    // 由 Tauri 自动居中；见 config::window_state）。
    crate::config::restore_main_window(app, &webview_window);

    #[cfg(windows)]
    {
        if !_notification_handlers_registered.swap(true, Ordering::SeqCst) {
            log::info!("[notification] scheduling handler registration from setup");
            let webview_for_dialog = webview_window.clone();
            if let Err(e) = webview_window.with_webview(move |webview| {
                if let Err(e) = crate::desktop::notification::enable_notification_permissions(
                    webview,
                    webview_for_dialog,
                ) {
                    log::warn!("[webview] failed to enable notification permission: {e}");
                }
            }) {
                log::warn!("[webview] failed to schedule notification permission setup: {e}");
            }
        }
    }

    Ok(webview_window)
}

// configure invoke handler
pub fn handler() -> impl Fn(Invoke<Wry>) -> bool + Send + Sync + 'static {
    let generated: fn(Invoke<Wry>) -> bool = tauri::generate_handler![
        crate::bridge::install_dependencies,
        crate::bridge::check_dsh_update,
        crate::bridge::launch_harness,
        crate::bridge::shutdown_harness,
        crate::bridge::restart_harness,
        crate::bridge::mark_core_ready,
        crate::bridge::rollback_core_update,
        crate::bridge::get_dsh_status,
        crate::bridge::get_preinstall_plugins,
        crate::bridge::get_preinstall_pending,
        crate::bridge::install_preinstall_plugins,
        crate::bridge::cancel_preinstall_plugins,
        crate::bridge::skip_preinstall_plugins,
        crate::bridge::open_preinstall_repo,
        crate::bridge::get_dsh_plugins,
        crate::bridge::domain_pack_list,
        crate::bridge::domain_pack_state,
        crate::bridge::domain_pack_update_check,
        crate::bridge::domain_pack_update_stage,
        crate::bridge::domain_pack_activate,
        crate::bridge::domain_pack_rollback,
        crate::bridge::domain_pack_set_enabled,
        crate::bridge::update_dsh_plugin,
        crate::bridge::remove_dsh_plugin,
        crate::bridge::report_plugin_error,
        crate::bridge::detect_plugin_recovery,
        crate::bridge::recover_plugin,
        crate::bridge::get_profiles,
        crate::bridge::create_profile,
        crate::bridge::set_active_profile,
        crate::bridge::remove_profile,
        crate::bridge::get_cores,
        crate::bridge::set_active_core,
        crate::bridge::download_core,
        crate::bridge::remove_core,
        crate::bridge::update_local_core,
        crate::bridge::proxy_health_check,
        crate::bridge::get_runtime_info,
        crate::bridge::runtime_ready,
        crate::bridge::get_app_config,
        crate::bridge::update_app_config,
        crate::bridge::get_cli_link_status,
        crate::bridge::open_in_browser,
        crate::bridge::copy_service_url,
        crate::bridge::reveal_data_dir,
        crate::bridge::reveal_in_folder,
        crate::bridge::open_dir,
        crate::bridge::read_service_logs,
        crate::bridge::read_run_logs,
        crate::bridge::clear_service_logs,
        crate::bridge::set_language,
        crate::bridge::toggle_sidebar,
        crate::bridge::get_dsh_theme,
        crate::bridge::check_desktop_update,
        crate::bridge::download_desktop_update,
        crate::bridge::open_desktop_installer,
        crate::bridge::get_desktop_about,
        crate::bridge::open_external_url,
        crate::bridge::read_clipboard_image,
        crate::desktop::notification::show_native_notification,
        crate::bridge::log_frontend,
        crate::bridge::project_pick_directory,
        crate::bridge::workspace_pick_directory,
        crate::bridge::project_import,
        crate::bridge::project_list,
        crate::bridge::project_get_active,
        crate::bridge::project_activate,
        crate::bridge::project_relink,
        crate::bridge::project_remove,
        crate::bridge::project_validate,
        crate::bridge::workspace_select,
        crate::bridge::workspace_list,
        crate::bridge::scan_start,
        crate::bridge::scan_cancel,
        crate::bridge::scan_status,
        crate::bridge::index_stats,
        crate::bridge::index_search,
        crate::bridge::domain_system_list,
        crate::bridge::domain_system_describe,
        crate::bridge::domain_file_query,
        crate::bridge::domain_unclaimed_file_query,
        crate::bridge::domain_resource_get,
        crate::bridge::domain_resource_query,
        crate::bridge::domain_dependency_resolve,
        crate::bridge::domain_validate,
        crate::bridge::domain_draft_validate,
        crate::bridge::task_receipt_list,
        crate::bridge::task_receipt_save,
        crate::bridge::user_capability_list,
        crate::bridge::user_capability_compile,
        crate::bridge::user_capability_get,
        crate::bridge::user_capability_set_status,
        crate::bridge::domain_memory_list,
        crate::bridge::domain_memory_save,
        crate::bridge::memory_candidate_list,
        crate::bridge::memory_candidate_activate,
        crate::bridge::memory_candidate_contest,
        crate::bridge::memory_candidate_revoke,
        crate::bridge::system_session_get,
        crate::bridge::system_session_bind,
        crate::bridge::task_scope_issue,
        crate::bridge::task_scope_revoke,
        crate::bridge::draft_list,
        crate::bridge::domain_draft_open,
        crate::bridge::domain_draft_composite_associate,
        crate::bridge::draft_preview,
        crate::bridge::draft_legacy_clone,
        crate::bridge::draft_apply,
        crate::bridge::draft_composite_apply,
        crate::bridge::draft_discard,
        crate::bridge::safe_file_open,
        crate::bridge::safe_text_patch,
        crate::bridge::safe_lua_patch,
        crate::bridge::safe_xls_open,
        crate::bridge::safe_xls_sheet_read,
        crate::bridge::safe_xls_patch,
        crate::bridge::safe_file_status,
        crate::bridge::snapshot_list,
        crate::bridge::snapshot_create,
        crate::bridge::snapshot_restore,
        crate::bridge::knowledge_list,
        crate::bridge::knowledge_get,
        crate::bridge::knowledge_set_status,
        crate::bridge::diagnostics_get,
        crate::bridge::gui_designer_status,
        crate::bridge::gui_document_list,
        crate::bridge::gui_document_open,
        crate::bridge::gui_document_reparse,
        crate::bridge::gui_document_template,
        crate::bridge::gui_dev_tree_list,
        crate::bridge::gui_asset_meta,
        crate::bridge::gui_asset_read,
        crate::bridge::gui_readonly_document_open,
        crate::bridge::gui_draft_prepare,
        crate::bridge::gui_draft_confirm,
        crate::bridge::gui_draft_apply,
    ];
    move |invoke| {
        let command = invoke.message.command().to_string();
        if (command.starts_with("gui_")
            || command.starts_with("domain_")
            || command.starts_with("draft_")
            || command.starts_with("task_")
            || command.starts_with("system_session_")
            || command.starts_with("memory_")
            || command.starts_with("user_capability_"))
            && !is_trusted_studio_invoke(&invoke)
        {
            log::warn!("[ipc] rejected Studio-only command from remote origin: {command}");
            invoke
                .resolver
                .reject("STUDIO_IPC_ORIGIN_DENIED: command is only available to the Studio shell");
            true
        } else {
            generated(invoke)
        }
    }
}

fn is_trusted_studio_invoke(invoke: &Invoke<Wry>) -> bool {
    let origin = invoke
        .message
        .headers()
        .get("Origin")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    is_trusted_studio_origin(origin)
}

fn is_trusted_studio_origin(origin: &str) -> bool {
    origin == "tauri://localhost"
        || origin == "http://tauri.localhost"
        || origin == "https://tauri.localhost"
        || (cfg!(debug_assertions)
            && (origin == "http://localhost:1420" || origin == "http://127.0.0.1:1420"))
}

#[cfg(test)]
mod studio_origin_tests {
    use super::is_trusted_studio_origin;

    #[test]
    fn remote_harness_origin_cannot_invoke_gui_commands() {
        assert!(!is_trusted_studio_origin("http://127.0.0.1:3080"));
        assert!(!is_trusted_studio_origin("http://127.0.0.1:3081"));
        assert!(!is_trusted_studio_origin("https://example.com"));
    }

    #[test]
    fn packaged_studio_origins_are_allowed() {
        assert!(is_trusted_studio_origin("tauri://localhost"));
        assert!(is_trusted_studio_origin("http://tauri.localhost"));
    }
}

// configure tauri builder
pub fn builder() -> tauri::Builder<tauri::Wry> {
    let builder = tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            let studio_data = crate::config::get_dsh_data_path(&app_handle);
            let project_service =
                crate::service::project::ProjectService::new_with_domain_pack_root(
                    studio_data.join("projects"),
                    studio_data.join("domain-packs"),
                )
                .map_err(std::io::Error::other)?;
            app.manage(project_service);
            build_main_window(&app_handle)?;
            tray(&app_handle)?;
            setup(app_handle.clone());
            Ok(())
        })
        // 点击关闭按钮时隐藏到托盘而不是退出程序
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            // 移动/缩放主窗口时记录几何，重启后据此恢复（见 config::window_state）
            tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                crate::config::save_geometry(window);
            }
            _ => {}
        });

    // 单例模式：多次双击图标（或重复启动）时不会新开窗口，而是把
    // 已存在的（可能已隐藏到托盘）主窗口调到前台，实现“单例 + 复用后台窗口”。
    // 该回调在首次启动时也会以当前进程的参数触发一次（幂等，仅 show/focus），
    // 之后每次二次启动都会派发到这里，重新展示后台运行的主窗口。
    // 仅在生产环境（release）启用：debug 开发调试时若启用单例，
    // 二次启动的调试进程会被吞掉（例如 tauri dev 多实例调试），
    // 因此开发环境跳过该插件。
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        crate::utils::show_main_window(app);
    }));

    builder
        // Opener plugin
        .plugin(tauri_plugin_opener::init())
        // Notification plugin（Windows 上以 tauri-winrt-notification 实现点击回调，
        // 注册官方插件保留跨平台回退能力）
        .plugin(tauri_plugin_notification::init())
        // FS plugin
        .plugin(tauri_plugin_fs::init())
        // Simple Store plugin
        .plugin(tauri_plugin_store::Builder::new().build())
        // Clipboard plugin
        .plugin(tauri_plugin_clipboard_manager::init())
}
