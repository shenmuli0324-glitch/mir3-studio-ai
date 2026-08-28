use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;
use tauri::{AppHandle, Emitter};

use super::runtime::get_dsh_data_path;

/// dsh 主题偏好（对应 `$MIR3_STUDIO_HOME/settings.yaml` 的 `ui-theme.preference`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DshTheme {
    Dark,
    Light,
    System,
}

const DEFAULT_THEME: DshTheme = DshTheme::Dark;
const FULL_CONTENT_CHECK_TICKS: u16 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileStamp {
    exists: bool,
    len: u64,
    modified: Option<SystemTime>,
}

struct ThemeWatchState {
    initialized: bool,
    stamp: FileStamp,
    last_theme: Option<DshTheme>,
    unchanged_ticks: u16,
}

static WATCH_STATE: OnceLock<Mutex<ThemeWatchState>> = OnceLock::new();

fn file_stamp(path: &Path) -> FileStamp {
    match fs::metadata(path) {
        Ok(metadata) => FileStamp {
            exists: true,
            len: metadata.len(),
            modified: metadata.modified().ok(),
        },
        Err(_) => FileStamp {
            exists: false,
            len: 0,
            modified: None,
        },
    }
}

/// 读取 dsh 主题偏好；settings.yaml 缺失或解析失败时回退为深色
pub fn get_dsh_theme(app_handle: &AppHandle) -> DshTheme {
    let settings_path = get_dsh_data_path(app_handle).join("settings.yaml");
    let content = match fs::read_to_string(&settings_path) {
        Ok(content) => content,
        Err(err) => {
            log::debug!("failed to read dsh settings.yaml: {}", err);
            return DEFAULT_THEME;
        }
    };
    parse_theme_preference(&content).unwrap_or(DEFAULT_THEME)
}

/// 从 settings.yaml 文本中提取 `ui-theme.preference`（light/dark/system）
fn parse_theme_preference(content: &str) -> Option<DshTheme> {
    let mut in_ui_theme = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("ui-theme:") {
            in_ui_theme = true;
            continue;
        }
        if !in_ui_theme {
            continue;
        }
        // ui-theme 段结束（遇到无缩进的新顶层 key）
        if !line.starts_with(' ') && !line.starts_with('\t') {
            return None;
        }
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            if let Some(value) = trimmed.strip_prefix("preference:") {
                return match value.trim() {
                    "light" => Some(DshTheme::Light),
                    "dark" => Some(DshTheme::Dark),
                    "system" => Some(DshTheme::System),
                    _ => None,
                };
            }
        }
    }
    None
}

/// 主题偏好变化时向前端推送 `dsh-theme-updated` 事件（仅在变化时触发一次）
pub fn check_and_emit_theme(app_handle: &AppHandle) {
    let settings_path = get_dsh_data_path(app_handle).join("settings.yaml");
    let stamp = file_stamp(&settings_path);
    let mut state = WATCH_STATE
        .get_or_init(|| {
            Mutex::new(ThemeWatchState {
                initialized: false,
                stamp: FileStamp {
                    exists: false,
                    len: 0,
                    modified: None,
                },
                last_theme: None,
                unchanged_ticks: 0,
            })
        })
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if state.initialized && state.stamp == stamp {
        state.unchanged_ticks = state.unchanged_ticks.saturating_add(1);
        if state.unchanged_ticks < FULL_CONTENT_CHECK_TICKS {
            return;
        }
    }
    state.initialized = true;
    state.stamp = stamp;
    state.unchanged_ticks = 0;
    let theme = get_dsh_theme(app_handle);
    if state.last_theme == Some(theme) {
        return;
    }
    state.last_theme = Some(theme);
    log::debug!("dsh theme preference changed: {:?}", theme);
    let _ = app_handle.emit("dsh-theme-updated", &theme);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_stamp_changes_when_settings_are_replaced() {
        let dir = std::env::temp_dir().join(format!(
            "mir3-theme-stamp-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = dir.join("settings.yaml");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let missing = file_stamp(&path);
        fs::write(&path, "ui-theme:\n  preference: dark\n").unwrap();
        let present = file_stamp(&path);
        assert_ne!(missing, present);
        assert!(!missing.exists);
        assert!(present.exists);
        fs::remove_dir_all(dir).ok();
    }
}
