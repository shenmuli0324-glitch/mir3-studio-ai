//! 应用全局配置、系统偏好与 CLI Link 集成。
//!
//! 桌面端自身设置（端口/自启/语言/主题/侧边栏）的读写，以及命令行集成的
//! 状态查询；命令行集成开关的落库顺序与 CLI Link 的文件/PATH 操作绑定。

use crate::config;
use crate::service::cli;
use tauri::AppHandle;

/// 当前桌面端配置
#[tauri::command]
pub async fn get_app_config(app_handle: AppHandle) -> Result<config::Setting, String> {
    Ok(config::get_store_dat_setting(&app_handle))
}

/// 更新桌面端配置
#[tauri::command]
pub async fn update_app_config(
    app_handle: AppHandle,
    port: Option<u16>,
    auto_start: Option<bool>,
    cli_link_enabled: Option<bool>,
) -> Result<config::Setting, String> {
    if let Some(port) = port {
        if port == 0 {
            return Err("port must be a positive number".to_string());
        }
    }
    // 命令行集成：先执行文件系统/PATH 操作，成功后再持久化开关，
    // 失败时配置保持不变，避免"开关已开但 shim 未生成"的不一致状态。
    if let Some(enabled) = cli_link_enabled {
        if enabled {
            cli::ensure(&app_handle)?;
        } else {
            cli::remove(&app_handle)?;
        }
    }
    let setting = config::update_setting(&app_handle, |setting| {
        if let Some(port) = port {
            setting.port = port;
        }
        if let Some(auto_start) = auto_start {
            setting.auto_start = auto_start;
        }
        if let Some(enabled) = cli_link_enabled {
            setting.cli_link_enabled = enabled;
        }
    });
    Ok(setting)
}

/// 命令行集成状态（shim 文件与 PATH 注册情况）
#[tauri::command]
pub fn get_cli_link_status(app_handle: AppHandle) -> Result<cli::CliLinkStatus, String> {
    Ok(cli::get_status(&app_handle))
}

/// 保存界面语言偏好
#[tauri::command]
pub fn set_language(app_handle: AppHandle, lang: String) {
    config::update_setting(&app_handle, |setting| setting.language = lang.clone());
    config::i18n::set_language(match lang.as_str() {
        "en" | "en-US" => config::i18n::Lang::En,
        _ => config::i18n::Lang::Zh,
    });
}

/// 当前 dsh 主题偏好（light/dark/system），用于让桌面外壳跟随内嵌页面主题
#[tauri::command]
pub fn get_dsh_theme(app_handle: AppHandle) -> config::DshTheme {
    config::get_dsh_theme(&app_handle)
}
