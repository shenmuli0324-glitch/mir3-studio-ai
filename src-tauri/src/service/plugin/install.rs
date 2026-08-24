//! 预装插件安装：校验选中项、准备环境（pnpm/dsh shim、按需补齐捆绑 pnpm、
//! 停止运行中的服务），随后调用 `dsh plugin --profile web add <specs...>`，
//! 成功后执行 Windows 极简模式专项修复。
//!
//! pnpm v11 对两类构建脚本默认不放行、缺白名单时报硬错误：
//! 1. git 托管插件的 `prepare` 构建（`ERR_PNPM_GIT_DEP_PREPARE_NOT_ALLOWED`）——
//!    其允许键（depPath = `name@<pkgResolutionId>`）随 pnpm 的克隆方式变化
//!    （git+ssh#sha / codeload tar.gz），无法预先确定；
//! 2. 传递依赖的原生构建（如 `node-pty`，`ERR_PNPM_IGNORED_BUILDS`）。
//! 因此在安装失败时从 pnpm 错误输出解析它建议的 `allowBuilds` 键，写入 profile
//! 的 `pnpm-workspace.yaml` 后重试，直至成功或无可解析项。

use crate::config;
use crate::service::cli;
use crate::service::core;
use crate::service::download;
use crate::service::download::Installable;
use crate::service::profile::active_profile;
use crate::service::workflow;
use serde_yaml::{Mapping, Value};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use super::errors;
use super::installed::{is_installed, profile_dir};
use super::preset::load_presets;
use super::process::{run_plugin_process, PreinstallLogPayload, PREINSTALL_LOG_EVENT};
use super::recovery::is_actionable_plugin_ref;
use super::uninstall_recovery;

/// 允许构建重试的上限。每次重试解决 pnpm 报出的一个允许键（git depPath 或
/// 传递构建包名），多个 git 插件 / 多个原生依赖各占一次，上限封顶防死循环。
const MAX_ALLOW_LIST_RETRIES: usize = 8;

/// 可安全用于插件安装的用户 pnpm 最低主版本。
///
/// pnpm 10+ 才从 `pnpm-workspace.yaml` 读取 `autoInstallPeers`（9 及更早只读
/// `.npmrc`）。低于此版本时插件安装必须改用捆绑版 pnpm，否则会出现自动合成
/// peer 后出现官方核心包版本不匹配的假失败。
/// pnpm 9/10/11 都保留 workspace-root 安装保护；Profile 本身就是刻意的
/// workspace 根，因此安装命令必须显式传 `--workspace-root`，不能依赖版本绕过。
const MIN_TRUSTED_PNPM_MAJOR: u32 = 10;

/// 校验并安装选中的预装插件：`dsh plugin --profile <当前档案> add <ids...>`
pub async fn install(app_handle: &AppHandle, ids: &[String]) -> Result<(), String> {
    if ids.is_empty() {
        return Err("PREINSTALL_EMPTY: no plugins selected".to_string());
    }

    // 单次读取预设并构建查找表，提升算法效率至 O(N)
    let presets = load_presets(app_handle);
    let preset_map: HashMap<&str, &str> = presets
        .iter()
        .map(|p| (p.id.as_str(), p.spec.as_str()))
        .collect();

    let mut specs = Vec::with_capacity(ids.len());
    for id in ids {
        let spec = preset_map
            .get(id.as_str())
            .ok_or_else(|| format!("PREINSTALL_INVALID_ID: {id}"))?;
        // 统一把 `github:user/repo` 规范为显式 `git+https://...`，绕开 pnpm 对
        // GitHub 简写「HTTPS 探测失败即回退 SSH」的已知缺陷（pnpm issue
        // #3948 / #7243 / #13276）：公开仓库一旦落进 git+ssh，在没有 SSH 配置
        // 的桌面机上必然 `Host key verification failed` / `Permission denied (publickey)`。
        specs.push(normalize_git_spec(spec));
    }

    // 确保 pnpm/dsh shim 存在
    cli::ensure_shims(app_handle)?;

    let node = config::get_node_binary_path(app_handle);
    // 活动核心的 dsh 入口：本地核心存在时用本地 CLI，否则预打包
    let dsh_bin = core::active_dsh_binary(app_handle);
    if !node.exists() {
        return Err("NODE_NOT_FOUND: Node.js runtime missing".to_string());
    }
    if !dsh_bin.exists() {
        return Err("HARNESS_NOT_FOUND: MIR3 AI Core CLI missing".to_string());
    }

    let window = app_handle
        .get_webview_window("main")
        .ok_or("WINDOW_NOT_FOUND: main window missing")?;

    // 选定/补齐安装用的 pnpm：返回是否应强制使用捆绑版（版本感知，见 ensure_pnpm）
    let prefer_bundled_pnpm = ensure_pnpm(app_handle, &window).await?;

    // 安装前停止运行中的服务，避免资源冲突。记录原状态，失败路径也必须恢复。
    let harness_was_running = workflow::has_owned_process();
    if harness_was_running {
        // 停服务会让用户感到"重启"，先在日志面板讲清缘由（issue #48）
        let _ = window.emit(
            PREINSTALL_LOG_EVENT,
            PreinstallLogPayload {
                line: "[MIR3 AI Core] 正在停止运行中的服务（安装插件需要短暂重启）…".to_string(),
            },
        );
        log::info!("Stopping running harness service before installing plugins");
        if let Err(e) = workflow::stop(app_handle.clone()).await {
            log::warn!("failed to stop harness before plugin install: {e}");
        }
    }

    let envs = build_plugin_envs(app_handle, prefer_bundled_pnpm);

    // Profile 的 pnpm-workspace.yaml 把 `.` 声明为 workspace 根；pnpm 9/10/11
    // 都要求添加依赖时显式确认根目标，否则报 ERR_PNPM_ADDING_TO_ROOT。
    let args = build_install_args(&dsh_bin, &active_profile(app_handle), &specs);

    let cwd = config::get_dsh_install_path(app_handle);
    // 日志打印实际传给 dsh 的 spec（此前打印 id 会误导排查：安装用的是 spec）
    log::info!("Running dsh plugin install for {specs:?}");

    // `dsh plugin add` 在 profile 目录里驱动 pnpm。pnpm v11 会拦下 git 托管
    // 插件的 prepare 构建与传递原生依赖（见模块头注），其允许键不可预知，因此
    // 失败时解析输出里印出的 `allowBuilds` 键写回 profile 的 pnpm-workspace.yaml
    // 后重试，直至成功或再无键可加。
    let mut retries = 0usize;
    let mut last_output = String::new();
    let exit_code = loop {
        let (code, captured) = run_plugin_process(&node, &args, &cwd, &envs, &window).await?;
        if code == 0 {
            break 0;
        }
        last_output = captured;

        let new_keys = parse_allowlist_keys(&last_output);
        if new_keys.is_empty() || retries >= MAX_ALLOW_LIST_RETRIES {
            log::error!(
                "dsh plugin install failed with exit code {code}; no more allowBuilds entries to add"
            );
            break code;
        }

        retries += 1;
        add_allow_build_keys(app_handle, &new_keys)?;
        log::info!("pnpm allowBuilds updated with {new_keys:?}, retrying ({retries})");
        let _ = window.emit(
            PREINSTALL_LOG_EVENT,
            PreinstallLogPayload {
                line: "[pnpm] 已放行插件构建（allowBuilds），重试安装…".to_string(),
            },
        );
    };

    if exit_code != 0 {
        log::error!("dsh plugin install failed with exit code {exit_code}");
        // 区分 git 传输层失败与 allowBuilds 构建门禁：前者是 pnpm 走了 git+ssh
        // （用户环境无 SSH 配置），后者才是补充白名单可自愈的。传输层错误给出
        // 可读指引，避免用户被 dsh 那条 allowBuilds 提示误导。
        let hint = git_transport_hint(&last_output);
        let message = pick_error_message(&last_output, hint);
        // 批量安装失败时给本次选中的每个插件记一条错误（前端据此展示异常标记，
        // 可针对单个插件重试更新/卸载）
        for id in ids {
            if let Err(e) = errors::record(app_handle, id, "install", &message) {
                log::warn!("failed to record plugin error for {id}: {e}");
            }
        }
        let failure = if let Some(hint) = hint {
            log::warn!("git transport failure detected during plugin install: {hint}");
            let _ = window.emit(
                PREINSTALL_LOG_EVENT,
                PreinstallLogPayload {
                    line: format!("[pnpm] {hint}"),
                },
            );
            format!(
                "PREINSTALL_FAILED: MIR3 AI Core plugin process exited with code {exit_code} ({hint})"
            )
        } else {
            format!("PREINSTALL_FAILED: MIR3 AI Core plugin process exited with code {exit_code}")
        };

        // 成功路径由前端拉起服务；失败路径在返回错误前恢复安装前运行的 MIR3 AI Core。
        if harness_was_running {
            let _ = window.emit(
                PREINSTALL_LOG_EVENT,
                PreinstallLogPayload {
                    line: "[MIR3 AI Core] 插件安装失败，正在恢复服务…".to_string(),
                },
            );
            if let Err(restart_error) = workflow::start(app_handle.clone()).await {
                log::error!(
                    "failed to restore MIR3 AI Core after plugin install failure: {restart_error}"
                );
                return Err(format!(
                    "{failure}; HARNESS_RESTART_FAILED: {restart_error}"
                ));
            }
        }
        return Err(failure);
    }

    // 安装成功：清除这些插件的历史错误记录
    for id in ids {
        if let Err(e) = errors::clear(app_handle, id) {
            log::warn!("failed to clear plugin error for {id}: {e}");
        }
    }

    // Windows 极简模式专项修复
    if ids.iter().any(|id| id == "dsh-win-terminal-inspector") {
        if let Err(e) = workflow::win_inspector::apply(app_handle) {
            log::warn!("win inspector apply failed after install: {e}");
        }
    }

    // 告知用户安装阶段结束；随后的服务重启由前端 continueAfterPreinstall 负责
    let _ = window.emit(
        PREINSTALL_LOG_EVENT,
        PreinstallLogPayload {
            line: format!("[MIR3 AI Core] 已安装 {} 个插件", ids.len()),
        },
    );

    log::info!("Preinstall plugins installed successfully: {ids:?}");
    Ok(())
}

/// Profile 是刻意设计的 pnpm workspace 根，因此 `add` 必须显式确认根目标。
fn build_install_args(dsh_bin: &Path, profile: &str, specs: &[String]) -> Vec<OsString> {
    let mut args = vec![
        dsh_bin.as_os_str().to_os_string(),
        OsString::from("plugin"),
        OsString::from("--profile"),
        OsString::from(profile),
        OsString::from("add"),
        OsString::from("--workspace-root"),
    ];
    args.extend(specs.iter().map(OsString::from));
    args
}

/// 构建 `dsh plugin` 子进程的环境变量：隔离 $MIR3_STUDIO_HOME、关闭遥测与颜色，
/// PATH 前置 shim 目录与 node 目录；用户 pnpm 过旧时强制捆绑版（见 ensure_pnpm）。
fn build_plugin_envs(app_handle: &AppHandle, prefer_bundled_pnpm: bool) -> HashMap<String, String> {
    let node = config::get_node_binary_path(app_handle);
    let bin_dir = cli::get_internal_bin_dir(app_handle);
    let mut envs = HashMap::from([
        (
            config::core_compat::CORE_HOME_ENV.to_string(),
            config::get_dsh_data_path(app_handle)
                .to_string_lossy()
                .into_owned(),
        ),
        ("DSH_TELEMETRY_DISABLED".to_string(), "1".to_string()),
        ("NO_COLOR".to_string(), "1".to_string()),
    ]);
    // 用户 pnpm 过旧/不可探测时强制 pnpm shim 优先捆绑版，避免 8/9 的
    // autoInstallPeers 语义破坏插件安装（workspace-root 由参数显式确认）。
    if prefer_bundled_pnpm {
        envs.insert("DSH_PREFER_BUNDLED_PNPM".to_string(), "1".to_string());
    }

    let mut paths = vec![bin_dir];
    // 保持 pnpm 候选顺序与版本探测一致，避免探测命中 Corepack pnpm 10，
    // 实际 shim 却先命中 Node 安装目录旁的 pnpm 9。
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    if let Some(node_dir) = node.parent() {
        if !paths.iter().any(|path| path == node_dir) {
            paths.push(node_dir.to_path_buf());
        }
    }

    if let Ok(joined) = std::env::join_paths(paths) {
        envs.insert("PATH".to_string(), joined.to_string_lossy().into_owned());
    }
    envs
}

/// 升级单个插件：`dsh plugin --profile <当前档案> update <id>`
pub async fn update(app_handle: &AppHandle, id: &str) -> Result<(), String> {
    if super::system::is_system_plugin(id) {
        return Err("PLUGIN_SYSTEM_MANAGED: MIR3 Core Plugin is managed by Studio".to_string());
    }
    run_single_plugin_command(
        app_handle,
        id,
        "update",
        &["update".to_string(), id.to_string()],
    )
    .await
}

/// 卸载单个插件：`dsh plugin --profile <当前档案> remove <id>`
pub async fn remove(app_handle: &AppHandle, id: &str) -> Result<(), String> {
    if super::system::is_system_plugin(id) {
        return Err("PLUGIN_SYSTEM_MANAGED: MIR3 Core Plugin cannot be removed".to_string());
    }
    run_single_plugin_command(
        app_handle,
        id,
        "remove",
        &["remove".to_string(), id.to_string()],
    )
    .await?;
    // `dsh plugin remove` 以子进程退出码为准，可能出现「命令成功但插件仍在」的
    // 边界（如 bundle 层残留、pnpm 静默失败）：核验 profile 清单，若插件仍被引用
    // 则回落到离线卸载（直接改清单 + 删目录 + 清 lockfile），确保插件真正移除
    // （参考 dsh-market 的「卸载后核验」约定：确认插件离开 profile 才算成功）。
    if is_installed(app_handle, id) {
        // 第三方可卸载插件才允许离线兜底；核心/官方等受保护包即使残留也不强删
        // （`uninstall_recovery` 对它们会拒绝），仅记录告警，避免把已成功的卸载
        // 误报为失败。
        if is_actionable_plugin_ref(id) {
            log::warn!(
                "dsh plugin remove reported success but {id} is still referenced by profile manifest; forcing offline uninstall"
            );
            uninstall_recovery(app_handle, id)?;
        } else {
            log::warn!(
                "dsh plugin remove reported success but protected package {id} is still referenced by profile manifest; skipping offline uninstall"
            );
        }
    }
    Ok(())
}

/// 执行单个插件的升级/卸载：准备环境 → 停止服务 → 运行 `dsh plugin` →
/// 失败记录错误、成功清除错误。
async fn run_single_plugin_command(
    app_handle: &AppHandle,
    id: &str,
    action: &str,
    sub_args: &[String],
) -> Result<(), String> {
    if id.is_empty() {
        return Err("PLUGIN_EMPTY_ID: plugin id is empty".to_string());
    }
    let window = app_handle
        .get_webview_window("main")
        .ok_or("WINDOW_NOT_FOUND: main window missing")?;

    cli::ensure_shims(app_handle)?;

    let node = config::get_node_binary_path(app_handle);
    let dsh_bin = core::active_dsh_binary(app_handle);
    if !node.exists() {
        return Err("NODE_NOT_FOUND: Node.js runtime missing".to_string());
    }
    if !dsh_bin.exists() {
        return Err("HARNESS_NOT_FOUND: MIR3 AI Core CLI missing".to_string());
    }

    let prefer_bundled_pnpm = ensure_pnpm(app_handle, &window).await?;

    // 插件操作会改写 profile，先停止运行中的服务（与安装一致）
    if workflow::has_owned_process() {
        let _ = window.emit(
            PREINSTALL_LOG_EVENT,
            PreinstallLogPayload {
                line: format!("[MIR3 AI Core] 正在停止运行中的服务（{action}插件需要短暂重启）…"),
            },
        );
        if let Err(e) = workflow::stop(app_handle.clone()).await {
            log::warn!("failed to stop harness before plugin {action}: {e}");
        }
    }

    let envs = build_plugin_envs(app_handle, prefer_bundled_pnpm);

    let mut args = vec![
        dsh_bin.as_os_str().to_os_string(),
        OsString::from("plugin"),
        OsString::from("--profile"),
        OsString::from(active_profile(app_handle)),
        OsString::from(action),
    ];
    args.extend(sub_args.iter().map(OsString::from));

    let cwd = config::get_dsh_install_path(app_handle);
    log::info!("Running dsh plugin {action} for {id}");
    let (exit_code, output) = run_plugin_process(&node, &args, &cwd, &envs, &window).await?;

    if exit_code != 0 {
        log::error!("dsh plugin {action} failed for {id} with exit code {exit_code}");
        let message = pick_error_message(&output, git_transport_hint(&output));
        if let Err(e) = errors::record(app_handle, id, action, &message) {
            log::warn!("failed to record plugin error for {id}: {e}");
        }
        return Err(format!(
            "PLUGIN_{}_FAILED: MIR3 AI Core plugin process exited with code {exit_code}",
            action.to_uppercase()
        ));
    }

    // 成功：清除历史错误；卸载 win-terminal-inspector 时顺带清理 patch 挂载
    if let Err(e) = errors::clear(app_handle, id) {
        log::warn!("failed to clear plugin error for {id}: {e}");
    }
    if action == "remove" && id == "dsh-win-terminal-inspector" {
        if let Err(e) = workflow::win_inspector::apply(app_handle) {
            log::warn!("win inspector patch prune failed after remove: {e}");
        }
    }
    log::info!("dsh plugin {action} succeeded for {id}");
    Ok(())
}

/// 从 dsh/pnpm 失败输出中提取可展示的错误消息：优先 git 传输层提示；
/// 否则挑出命中错误标记的行（最多 8 行），没有则取输出尾部，ANSI 清洗后
/// 截断到 2000 字符。
fn pick_error_message(output: &str, hint: Option<&str>) -> String {
    if let Some(hint) = hint {
        return hint.to_string();
    }
    let cleaned: Vec<String> = output
        .split('\n')
        .filter_map(|line| {
            let trimmed = strip_ansi(line);
            let trimmed = trimmed.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .filter(|line| {
            line.contains("ERR_")
                || line.contains("error")
                || line.contains("Error")
                || line.contains("failed")
                || line.contains("✖")
                || line.contains("warning")
        })
        .take(8)
        .collect();
    let base = if cleaned.is_empty() {
        output.trim().to_string()
    } else {
        cleaned.join("\n")
    };
    base.chars().take(2000).collect()
}

/// 去除 ANSI 转义序列（`\x1B[...m`，含颜色/样式码）。
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next(); // '['
            while let Some(&n) = chars.peek() {
                if n.is_ascii_digit() || n == ';' {
                    chars.next();
                } else {
                    break;
                }
            }
            if chars.peek() == Some(&'m') {
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 确保插件安装使用的 pnpm 可用，返回是否应强制使用捆绑版
/// （true 时调用方注入 `DSH_PREFER_BUNDLED_PNPM=1`，pnpm shim 优先捆绑版）。
///
/// 版本感知策略，避免给已装正确 pnpm 的用户增加下载步骤：
/// - 档案 store 主版本已知 → 只接受与其一致的 pnpm（用户版或捆绑版）。
///   pnpm 10 与 11 的 store 布局互不兼容（`.../store/v10` vs `v11`），用与
///   store 主版本不一致的 pnpm 更新已装插件会直接 `ERR_PNPM_UNEXPECTED_STORE`
///   退出码 1 失败——升级失败的根因（此前捆绑版 v11 一存在就强制使用，
///   对 v10 store 的档案必然失败）；
/// - 用户 pnpm 主版本 == store 主版本 → 复用用户 pnpm，零额外步骤；
/// - 捆绑版 pnpm 主版本 == store 主版本 → 用捆绑版（不下载）；
/// - store 未知（全新档案/未装过依赖）或无可匹配版本 → 用户 pnpm ≥ 10 优先，
///   否则捆绑版已存在则用，再否则下载捆绑版并强制使用。
///
/// 用户 pnpm 过旧（8/9：不按当前 Profile 需要的方式读取 autoInstallPeers；
/// corepack shim 在 Node 24 上还可能 ERR_INVALID_THIS 崩溃）或版本不可探测
/// → 走捆绑版。workspace-root 保护由安装参数显式确认。
async fn ensure_pnpm(app_handle: &AppHandle, window: &WebviewWindow) -> Result<bool, String> {
    // 档案的 node_modules 由哪个 pnpm 主版本创建（.modules.yaml 的 storeDir 段）
    let store_major = profile_store_major(app_handle);
    let user_major = user_pnpm_major_version(app_handle);

    // 1) store 主版本已知 → 优先选与 store 一致的 pnpm（用户版或捆绑版）
    if let Some(store) = store_major {
        if user_major == Some(store) {
            log::info!("Reusing user-installed pnpm (major {store}) matching profile store");
            return Ok(false);
        }
        if bundled_pnpm_major(app_handle) == Some(store) {
            log::info!("Using bundled pnpm (major {store}) matching profile store");
            return Ok(true);
        }
        log::warn!(
            "No pnpm matches profile store major {store} (user {user_major:?}), falling back to user pnpm"
        );
    }

    // 2) store 未知（全新档案/未装过依赖）或无可匹配版本 → 用户 pnpm ≥ 10 优先
    match user_major {
        Some(major) if major >= MIN_TRUSTED_PNPM_MAJOR => {
            log::info!("Reusing user-installed pnpm (major {major}) for plugin install");
            return Ok(false);
        }
        Some(major) => {
            log::warn!(
                "User pnpm major {major} < {MIN_TRUSTED_PNPM_MAJOR} (missing autoInstallPeers/workspace-root semantics), using bundled pnpm"
            );
        }
        None => {
            log::warn!(
                "User pnpm version not detectable (broken/blocked shim?), using bundled pnpm"
            );
        }
    }

    // 捆绑版已存在 → 直接用（零额外下载）；否则下载。
    if config::get_pnpm_binary_path(app_handle).exists() {
        return Ok(true);
    }

    let _ = window.emit(
        PREINSTALL_LOG_EVENT,
        PreinstallLogPayload {
            line: "[pnpm] bundled pnpm not found, downloading before plugin install".to_string(),
        },
    );

    let tracker = download::ProgressTracker::new(window, 2);
    let url = download::Pnpm.get_download_url()?;
    let name = url.split('/').next_back().unwrap_or(&url).to_string();
    let buffer = download::download_file(&tracker, url)
        .await
        .map_err(|e| format!("PNPM_DOWNLOAD_FAILED: {e}"))?;
    download::verify_sha256(&buffer, config::PNPM_SHA256)
        .map_err(|e| format!("PNPM_INTEGRITY_FAILED: {e}"))?;
    let dest = download::Pnpm.get_install_path(app_handle);

    download::ensure_extract(&tracker, name, buffer, dest)
        .await
        .map_err(|e| format!("PNPM_EXTRACT_FAILED: {e}"))?;

    let _ = window.emit(
        PREINSTALL_LOG_EVENT,
        PreinstallLogPayload {
            line: "[pnpm] bundled pnpm ready".to_string(),
        },
    );
    Ok(true)
}

/// 用户 pnpm 主版本号（解析 `pnpm --version` 首个点分字段）；不存在或不可运行
/// （corepack shim 在 Node 24 上 ERR_INVALID_THIS 崩溃等）返回 None。
fn user_pnpm_major_version(app_handle: &AppHandle) -> Option<u32> {
    let pnpm = cli::find_user_pnpm(app_handle)?;
    let output = std::process::Command::new(&pnpm)
        .arg("--version")
        // Corepack 会按 cwd 查找 packageManager；必须在真正执行 pnpm 的 Profile
        // 目录探测，避免源码目录和 Profile 目录得到不同版本。
        .current_dir(profile_dir(app_handle))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.split('.').next()?.trim().parse::<u32>().ok()
}

/// 档案 `node_modules` 使用的 pnpm store 主版本（`<profile>/node_modules/.modules.yaml`
/// 的 `storeDir` 路径段，如 `...\store\v10` → 10）。
///
/// pnpm 10 与 11 的 store 布局互不兼容：用与 store 主版本不一致的 pnpm 更新
/// 已装插件会 `ERR_PNPM_UNEXPECTED_STORE` 退出。档案尚未安装过依赖（没有
/// node_modules）时返回 `None`，由调用方走"全新档案"逻辑。
fn profile_store_major(app_handle: &AppHandle) -> Option<u32> {
    let modules_yaml = profile_dir(app_handle)
        .join("node_modules")
        .join(".modules.yaml");
    let content = std::fs::read_to_string(modules_yaml).ok()?;
    parse_store_major_from_modules_yaml(&content)
}

/// 从 `.modules.yaml` 文本解析 store 主版本（纯函数，便于单测）。
fn parse_store_major_from_modules_yaml(content: &str) -> Option<u32> {
    let store_dir = content
        .lines()
        .find_map(|line| line.trim().strip_prefix("storeDir:").map(str::trim))?;
    // storeDir 形如 `C:\Users\xx\AppData\Local\pnpm\store\v10`，取末段 `v10` 的数字
    let major = store_dir
        .trim_matches(['"', '\''])
        .rsplit(['\\', '/'])
        .next()?
        .strip_prefix('v')?;
    major.parse().ok()
}

/// 捆绑版 pnpm 的主版本（读 `dependencies/pnpm/package.json` 的 version 字段）；
/// 未安装或清单缺失返回 None。
fn bundled_pnpm_major(app_handle: &AppHandle) -> Option<u32> {
    let manifest = config::get_pnpm_install_path(app_handle).join("package.json");
    let content = std::fs::read_to_string(manifest).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    value
        .get("version")?
        .as_str()?
        .split('.')
        .next()?
        .parse()
        .ok()
}

/// 从 pnpm 失败输出中解析需写入 `allowBuilds` 的键集合：
/// - git 托管插件 prepare 被拦时，pnpm 会提示 `allowBuilds:\n  <depPath>: true`，
///   原样采纳 depPath（形式随克隆方式变化，只能是运行期报出的值）；
/// - 传递原生依赖被忽略构建（`Ignored build scripts:`）时，取其 `name@version` 的包名。
fn parse_allowlist_keys(output: &str) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let lines: Vec<&str> = output.lines().collect();

    // 1) git depPath 允许键：跟随 `allowBuilds:` 示例行后的缩进 `<key>: true`。
    for (idx, line) in lines.iter().enumerate() {
        if line.trim() == "allowBuilds:" {
            if let Some(next) = lines.get(idx + 1) {
                if let Some(key) = extract_allow_line_key(next) {
                    if !keys.iter().any(|k| k == &key) {
                        keys.push(key);
                    }
                }
            }
        }
    }

    // 2) 传递原生构建包名：`Ignored build scripts: <name>@<ver>, ...`。
    for line in &lines {
        if let Some(sub) = line.split("Ignored build scripts:").nth(1) {
            for token in sub.split([',', ' ']) {
                let token = token.trim();
                if token.is_empty() {
                    continue;
                }
                let name = token.split('@').next().unwrap_or(token).trim();
                if !name.is_empty() && !keys.iter().any(|k| k == name) {
                    keys.push(name.to_string());
                }
            }
        }
    }

    keys
}

/// 若 `line` 形如 `  <key>: true`（有缩进），返回 `<key>`（去缩进与后缀）。
/// pnpm 报出的 depPath 键本身不带引号，这里只做剥离该行格式。
fn extract_allow_line_key(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if trimmed.len() == line.len() {
        return None; // 无缩进，不是白名单条目
    }
    let suffix = trimmed.strip_suffix(": true")?;
    let key = suffix.trim_end();
    if key.is_empty() {
        return None;
    }
    Some(key.to_string())
}

/// profile 下的 `pnpm-workspace.yaml` 路径（$MIR3_STUDIO_HOME/profiles/<当前档案>）
fn profile_workspace_path(app_handle: &AppHandle) -> PathBuf {
    profile_dir(app_handle).join("pnpm-workspace.yaml")
}

/// 把新的 `allowBuilds` 键合并写回 profile 的 `pnpm-workspace.yaml`。
///
/// 用 YAML 库（serde_yaml）整体改写而非字符串拼接，避免格式错乱：
/// - 键（git depPath 含 `@`/`/`/`:`/`#`）由库自动按需加引号，不再手工拼；
/// - 已存在的同名键会被就地覆盖，不会残留占位值。
///
/// TODO(v1): 移除对旧版损坏文件（issue #49）的自愈逻辑。v1 起只解析干净配置，
/// `apply_allow_build_keys` 中解析失败后的「同键去重再解析」与
/// `collapse_allow_builds_duplicates` 一并删除。
///
/// 防御性修复：旧版本用字符串拼接可能留下「重复映射键」的损坏文件
/// （最多见的是 `node-pty: set this to true or false` 占位行与真正的
/// `'node-pty': true` 并存，见 issue #49）。此处解析失败时先做一次
/// `allowBuilds` 同键去重再解析，把损坏文件自愈回合法 YAML。
fn add_allow_build_keys(app_handle: &AppHandle, keys: &[String]) -> Result<(), String> {
    let path = profile_workspace_path(app_handle);
    let dir = path
        .parent()
        .ok_or("PREINSTALL_BAD_PROFILE_DIR: no profile dir")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("PREINSTALL_MKDIR: {e}"))?;

    let content = if path.exists() {
        std::fs::read_to_string(&path).map_err(|e| format!("PREINSTALL_READ_WORKSPACE: {e}"))?
    } else {
        // 与 dsh `initProfile` 生成的基础模板保持一致（尚无 allowBuilds）。
        "packages:\n  - .\n\nnodeLinker: hoisted\nautoInstallPeers: false\n".to_string()
    };

    let rendered = apply_allow_build_keys(&content, keys)?;
    if rendered == content {
        return Ok(()); // 无变化（所有键已就位），避免无意义写盘
    }

    log::info!(
        "pnpm-workspace.yaml rewritten with allowBuilds {keys:?} at {}",
        path.display()
    );
    std::fs::write(&path, rendered).map_err(|e| format!("PREINSTALL_WRITE_WORKSPACE: {e}"))
}

/// 把新的 `allowBuilds` 键合并进 `pnpm-workspace.yaml` 文本并返回新文本。
///
/// 用 YAML 库（serde_yaml）整体改写而非字符串拼接，避免格式错乱：
/// - 键（git depPath 含 `@`/`/`/`:`/`#`）由库自动按需加引号，不再手工拼；
/// - 已存在的同名键会被就地覆盖为 `true`，不会残留占位值，也不会产生重复键。
///
/// 防御性修复：旧版本用字符串拼接可能留下「重复映射键」的损坏文件
/// （最多见的是 `node-pty: set this to true or false` 占位行与真正的
/// `'node-pty': true` 并存，见 issue #49）。此处先尝试严格解析；解析失败时
/// 做一次 `allowBuilds` 同键去重再解析，把损坏文件自愈回合法 YAML。
fn apply_allow_build_keys(content: &str, keys: &[String]) -> Result<String, String> {
    // 先尝试严格解析。旧的损坏文件（重复映射键）严格解析会失败：
    // 把 `allowBuilds` 内同名键去重（保留最后写入的值）后再解析，自愈损坏状态。
    let mut repaired = false;
    let mut doc: Value = match serde_yaml::from_str(content) {
        Ok(v) => v,
        Err(first_err) => {
            let normalized = collapse_allow_builds_duplicates(content);
            if normalized == content {
                return Err(format!("PREINSTALL_WORKSPACE_INVALID_YAML: {first_err}"));
            }
            repaired = true;
            serde_yaml::from_str(&normalized)
                .map_err(|e| format!("PREINSTALL_WORKSPACE_INVALID_YAML: {e}"))?
        }
    };

    // 空/注释-only 内容解析为 `Value::Null`，视为全新空配置（pnpm-workspace.yaml
    // 可加载的最小映射）；其余非映射内容才是真正的损坏。
    if doc.is_null() {
        doc = Value::Mapping(Mapping::new());
    }

    let map = doc.as_mapping_mut().ok_or_else(|| {
        "PREINSTALL_WORKSPACE_NOT_MAP: pnpm-workspace.yaml must be a mapping".to_string()
    })?;

    let allow_key = Value::String("allowBuilds".to_string());
    if !map.contains_key(&allow_key) {
        map.insert(allow_key.clone(), Value::Mapping(Mapping::new()));
    }
    let allow_builds = map
        .get_mut(&allow_key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| {
            "PREINSTALL_WORKSPACE_ALLOWBUILDS_NOT_MAP: allowBuilds must be a mapping".to_string()
        })?;

    let mut dirty = false;
    for key in keys {
        let k = Value::String(key.clone());
        if allow_builds.get(&k) == Some(&Value::Bool(true)) {
            continue; // 已是 true，幂等跳过
        }
        // 直接覆盖旧值（含占位值/旧 false），由库负责按需加引号
        allow_builds.insert(k, Value::Bool(true));
        dirty = true;
    }
    if !dirty && !repaired {
        return Ok(content.to_string());
    }
    // 有键新增，或损坏文件已被自愈归一化——两种都要落回解析后的完整文档，
    // 否则会把损坏的原始文本原样返回。

    serde_yaml::to_string(&doc).map_err(|e| format!("PREINSTALL_WORKSPACE_RENDER: {e}"))
}

/// 把损坏的 `allowBuilds` 映射（同一键出现多次）去重为合法 YAML。
///
/// 仅作为旧版字符串拼接遗留损坏（重复映射键，见 issue #49）的兜底归一化：
/// 扫描 `allowBuilds:` 之后、下一个顶层键之前的缩进 `key: value` 行，同一键
/// 只保留最后一次出现的行（与 YAML「后者覆盖前者」语义一致），其余行原样保留。
fn collapse_allow_builds_duplicates(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut in_allow = false;
    // 记录（键 → 该键所有行的索引），用于去重
    let mut key_indexes: HashMap<String, Vec<usize>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed == "allowBuilds:" {
            in_allow = true;
            continue;
        }
        if in_allow {
            let is_indent = line.starts_with(' ') || line.starts_with('\t');
            let is_comment = trimmed.starts_with('#');
            if !is_indent || is_comment {
                in_allow = false; // 遇到顶层键或注释即离开 allowBuilds
                continue;
            }
            // 缩进的 `key: value` 行 → 提取键（冒号前）
            if let Some(col) = trimmed.find(':') {
                let key = trimmed[..col].trim().trim_matches(['\'', '"']);
                if !key.is_empty() {
                    if !key_indexes.contains_key(key) {
                        order.push(key.to_string());
                    }
                    key_indexes.entry(key.to_string()).or_default().push(idx);
                }
            }
        }
    }

    // 每个键只保留最后一个出现行，其余标记删除
    let mut keep: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for key in &order {
        if let Some(idxs) = key_indexes.get(key) {
            if let Some(&last) = idxs.last() {
                keep.insert(last);
            }
        }
    }
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    for (idx, line) in lines.iter().enumerate() {
        if key_indexes.values().any(|v| v.contains(&idx)) && !keep.contains(&idx) {
            continue; // 是被去重掉的重复键行
        }
        out.push(line);
    }
    // 避免重复键里夹带的空行粘连成异常空行：去掉去重区（allowBuilds 段）的连续空行
    out.join("\n")
}

/// 把 `github:owner/repo[#ref]` 一类的 GitHub 简写规范为显式 HTTPS 依赖形式
/// （`git+https://github.com/owner/repo.git[#ref]`）。
///
/// 动机：pnpm 解析 GitHub 简写时，「HTTPS 可达性探测一旦失败就回退 git+ssh」
/// 是已知缺陷（issue #3948 / #7243 / #13276，官方已 accepted 仍未修）。公开仓库
/// 一旦落进 git+ssh，在无 SSH 配置的桌面机上（非交互子进程无法应答 known_hosts
/// 询问）必然硬失败。规范为显式 `git+https:` 后 pnpm 直接走 HTTPS 克隆，绕开该
/// 回退；非 `github:` 形式（如纯 npm 包名）原样返回。
fn normalize_git_spec(spec: &str) -> String {
    let Some(rest) = spec.strip_prefix("github:") else {
        return spec.to_string();
    };
    let (path, fragment) = match rest.split_once('#') {
        Some((p, f)) => (p.trim_end_matches('/'), Some(f)),
        None => (rest.trim_end_matches('/'), None),
    };
    let mut repo = path.to_string();
    if !repo.ends_with(".git") {
        repo.push_str(".git");
    }
    let mut url = format!("git+https://github.com/{repo}");
    if let Some(fragment) = fragment {
        url.push('#');
        url.push_str(fragment);
    }
    url
}

/// 从 pnpm 失败输出里识别 git 传输层错误（区别于 allowBuilds 构建门禁），命中时
/// 返回一句可读的成因/指引。pnpm 在这些场景下已经退到 git+ssh，再去补 allowBuilds
/// 白名单是无效且误导的。
fn git_transport_hint(output: &str) -> Option<&'static str> {
    const SIGNALS: &[(&str, &str)] = &[
        (
            "host key verification failed",
            "git fell back to SSH and could not verify GitHub's host key (no known_hosts entry; the process ran non-interactively). Make sure GitHub is reachable over HTTPS.",
        ),
        (
            "permission denied (publickey)",
            "git fell back to SSH but no GitHub SSH key is configured (Permission denied (publickey)). Reach GitHub over HTTPS instead.",
        ),
        (
            "could not read from remote repository",
            "pnpm could not read from the git remote — commonly a git+ssh transport failure. Ensure GitHub is reachable over HTTPS.",
        ),
        (
            "ssh: connect to host",
            "pnpm tried to reach GitHub over SSH (port 22) and the connection was refused. Use HTTPS instead.",
        ),
    ];
    let lower = output.to_ascii_lowercase();
    SIGNALS
        .iter()
        .find(|(sig, _)| lower.contains(sig))
        .map(|(_, hint)| *hint)
}

#[cfg(test)]
mod tests {
    use super::build_install_args;
    use super::{
        apply_allow_build_keys, collapse_allow_builds_duplicates, extract_allow_line_key,
        git_transport_hint, normalize_git_spec, parse_allowlist_keys,
        parse_store_major_from_modules_yaml,
    };
    use std::path::Path;

    #[test]
    fn install_args_explicitly_target_profile_workspace_root() {
        let args = build_install_args(
            Path::new("/opt/dsh/bin.js"),
            "web",
            &["example-plugin".to_string()],
        );
        let rendered: Vec<String> = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            rendered,
            vec![
                "/opt/dsh/bin.js",
                "plugin",
                "--profile",
                "web",
                "add",
                "--workspace-root",
                "example-plugin",
            ]
        );
    }

    #[test]
    fn store_major_parsed_from_modules_yaml() {
        // 真实 pnpm v10 写入的 .modules.yaml：storeDir 指向 store\v10
        let content = format!(
            "\
lockfileVersion: '9.0'
settings:
  autoInstallPeers: true
  excludeLinksFromLockfile: false
dependencies:
  '{}': 0.0.4
  '{}': 0.0.4
storeDir: C:\\Users\\test\\AppData\\Local\\pnpm\\store\\v10
virtualStoreDir: node_modules/.pnpm
",
            crate::config::core_compat::WEB_PROFILE_BUNDLES[0],
            crate::config::core_compat::WEB_PROFILE_BUNDLES[1]
        );
        assert_eq!(parse_store_major_from_modules_yaml(&content), Some(10));
    }

    #[test]
    fn store_major_supports_unix_and_quoted_paths() {
        assert_eq!(
            parse_store_major_from_modules_yaml(
                "storeDir: /home/test/.local/share/pnpm/store/v11\n"
            ),
            Some(11)
        );
        assert_eq!(
            parse_store_major_from_modules_yaml("storeDir: \"C:\\\\pnpm store\\\\v3\"\n"),
            Some(3)
        );
    }

    #[test]
    fn store_major_missing_when_no_store_dir() {
        // 档案尚未装过依赖：无 storeDir 段 → None
        assert_eq!(
            parse_store_major_from_modules_yaml("lockfileVersion: '9.0'\n"),
            None
        );
        assert_eq!(parse_store_major_from_modules_yaml(""), None);
        assert_eq!(
            parse_store_major_from_modules_yaml("storeDir: C:\\Users\\x\\pnpm\\store\n"),
            None
        );
    }

    #[test]
    fn parse_git_dep_path_key() {
        let out = "\
[ERR_PNPM_GIT_DEP_PREPARE_NOT_ALLOWED] Failed to prepare git-hosted package fetched from \"...\"
The git-hosted package \"dsh-better-sidebar@0.14.0\" needs to execute build scripts but is not in the \"allowBuilds\" allowlist.
...
allowBuilds:
  dsh-better-sidebar@git+ssh://git@github.com/omdsh-dev/DSH-better-sidebar.git#6c89: true
";
        let keys = parse_allowlist_keys(out);
        assert!(keys.contains(
            &"dsh-better-sidebar@git+ssh://git@github.com/omdsh-dev/DSH-better-sidebar.git#6c89"
                .to_string()
        ));
        assert!(!keys.contains(&"dsh-better-sidebar".to_string()));
    }

    #[test]
    fn parse_ignored_builds_name() {
        let out = "[ERR_PNPM_IGNORED_BUILDS] Ignored build scripts: node-pty@1.1.0\n";
        let keys = parse_allowlist_keys(out);
        assert_eq!(keys, vec!["node-pty".to_string()]);
    }

    #[test]
    fn parse_empty_when_irrelevant() {
        let out = "everything looks fine output\nno allowlist here\n";
        assert!(parse_allowlist_keys(out).is_empty());
    }

    #[test]
    fn allow_line_key_requires_indent() {
        let key = extract_allow_line_key("  node-pty: true");
        assert_eq!(key.as_deref(), Some("node-pty"));

        // 无缩进（顶层键）不应被当作白名单条目
        assert_eq!(extract_allow_line_key("packages:"), None);
        assert_eq!(extract_allow_line_key("allowBuilds:"), None);
    }

    // ---- 归并写回 pnpm-workspace.yaml（issue #49 回归）----

    /// 从渲染结果里解析出单一 `allowBuilds` 映射，便于断言。
    fn allow_builds_map(yaml: &str) -> serde_yaml::Mapping {
        let doc: serde_yaml::Value = serde_yaml::from_str(yaml).expect("output must be valid YAML");
        doc.get("allowBuilds")
            .and_then(serde_yaml::Value::as_mapping)
            .expect("allowBuilds must be a mapping")
            .clone()
    }

    #[test]
    fn apply_adds_new_key_when_absent() {
        let base = "packages:\n  - .\n\nnodeLinker: hoisted\nautoInstallPeers: false\n";
        // 无 allowBuilds 时首次写入
        let out = apply_allow_build_keys(base, &["node-pty".to_string()]).unwrap();
        let map = allow_builds_map(&out);
        assert_eq!(map.get("node-pty"), Some(&serde_yaml::Value::Bool(true)));
        // 顶级基础设置被保留
        let doc: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert!(doc.get("packages").is_some());
        assert!(doc.get("nodeLinker").is_some());
    }

    #[test]
    fn apply_is_idempotent_and_does_not_duplicate() {
        // 已放行的键再次写入：结果不变（幂等、不产生重复键）
        let base = "packages:\n  - .\nnodeLinker: hoisted\nautoInstallPeers: false\nallowBuilds:\n  node-pty: true\n";
        let out = apply_allow_build_keys(base, &["node-pty".to_string()]).unwrap();
        assert_eq!(out, base);
    }

    #[test]
    fn apply_quotes_git_dep_path_keys() {
        let dep =
            "dsh-better-sidebar@git+ssh://git@github.com/omdsh-dev/DSH-better-sidebar.git#6c89"
                .to_string();
        // 空内容也能生成合法配置
        let out = apply_allow_build_keys("", &[dep.clone()]).unwrap();
        let map = allow_builds_map(&out);
        assert_eq!(
            map.get(&serde_yaml::Value::String(dep)),
            Some(&serde_yaml::Value::Bool(true))
        );
        // 库负责正确加引号，键原样（含 @ / : / #）可回读
        let doc: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            doc["allowBuilds"][&serde_yaml::Value::String(
                "dsh-better-sidebar@git+ssh://git@github.com/omdsh-dev/DSH-better-sidebar.git#6c89"
                    .to_string()
            )],
            serde_yaml::Value::Bool(true)
        );
    }

    #[test]
    fn apply_overwrites_placeholder_value_without_duplicate() {
        // 关键回归：旧版字符串拼接可能留下占位键 `node-pty: set this to true or false`
        // 与真实键并存。若解析保留重复键，或解析失败被去重兜底，最终都必须只保留
        // 一个 `node-pty: true`（不允许重复映射键）。
        let corrupted =
            "allowBuilds:\n  'dsh-better-sidebar@https://code...': true\n  node-pty: set this to true or false\n  'node-pty': true\n";
        let out = apply_allow_build_keys(corrupted, &["node-pty".to_string()]).unwrap();
        let map = allow_builds_map(&out);
        // 恰好只有一个 node-pty 键，值是 true（覆盖了占位值）
        assert_eq!(map.get("node-pty"), Some(&serde_yaml::Value::Bool(true)));
        // 序列化后全局不允许再出现“重复键”的等价行（node-pty 只出现一次）
        let node_pty_keys = out
            .lines()
            .filter(|l| {
                l.trim_start().starts_with("node-pty") || l.trim_start().starts_with("'node-pty'")
            })
            .count();
        assert_eq!(node_pty_keys, 1);
    }

    #[test]
    fn collapse_dedupes_allow_builds_keys() {
        let corrupted =
            "packages:\n  - .\nallowBuilds:\n  node-pty: set this to true or false\n  'node-pty': true\n  keep: true\n";
        let normalized = collapse_allow_builds_duplicates(corrupted);
        // 重复的 node-pty 只剩最后一个（值 true），同键不再重复
        let node_pty = normalized
            .lines()
            .filter(|l| {
                l.trim_start().starts_with("node-pty") || l.trim_start().starts_with("'node-pty'")
            })
            .count();
        assert_eq!(node_pty, 1);
        assert!(normalized.contains("keep"));
        // 去重结果必须是合法 YAML，且能被后续解析
        let out = apply_allow_build_keys(&normalized, &["node-pty".to_string()]).unwrap();
        assert_eq!(
            allow_builds_map(&out).get("node-pty"),
            Some(&serde_yaml::Value::Bool(true))
        );
    }

    // ---- git GitHub 简写规范化（issue #51 根因绕行）----

    #[test]
    fn normalize_github_shorthand_to_git_https() {
        assert_eq!(
            normalize_git_spec("github:baihejiangnan/dsh-session-context-menu"),
            "git+https://github.com/baihejiangnan/dsh-session-context-menu.git"
        );
    }

    #[test]
    fn normalize_github_shorthand_preserves_ref_and_dedup_git_suffix() {
        assert_eq!(
            normalize_git_spec("github:omdsh-dev/DSH-better-sidebar#next"),
            "git+https://github.com/omdsh-dev/DSH-better-sidebar.git#next"
        );
        // 已带 .git 不重复追加
        assert_eq!(
            normalize_git_spec("github:user/repo.git"),
            "git+https://github.com/user/repo.git"
        );
        // 尾部多余斜杠剥掉
        assert_eq!(
            normalize_git_spec("github:user/repo/"),
            "git+https://github.com/user/repo.git"
        );
    }

    #[test]
    fn normalize_non_github_spec_passes_through() {
        assert_eq!(normalize_git_spec("dshmarket"), "dshmarket");
        assert_eq!(
            normalize_git_spec("git+https://github.com/foo/bar.git"),
            "git+https://github.com/foo/bar.git"
        );
    }

    // ---- git 传输层错误识别（区别于 allowBuilds 门禁）----

    #[test]
    fn git_transport_hint_detects_host_key_failure() {
        let out = "git ls-remote \"git+ssh://git@github.com/foo.git\" HEAD\nHost key verification failed.\nfatal: Could not read from remote repository.\n";
        assert!(git_transport_hint(out).is_some());
    }

    #[test]
    fn git_transport_hint_detects_publickey_and_ssh() {
        assert!(git_transport_hint("git@github.com: Permission denied (publickey)").is_some());
        assert!(
            git_transport_hint("ssh: connect to host github.com port 22: Connection refused")
                .is_some()
        );
    }

    #[test]
    fn git_transport_hint_none_for_allowbuilds_output() {
        // allowBuilds 场景（prepare 构建被拦）不应误判为传输层错误
        let out = "[ERR_PNPM_GIT_DEP_PREPARE_NOT_ALLOWED] ...\nallowBuilds:\n  node-pty: true\n";
        assert!(git_transport_hint(out).is_none());
    }
}
