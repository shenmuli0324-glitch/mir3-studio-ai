//! 已安装插件监控：轮询 profile 插件文件（`package.json` + `node_modules` 下
//! 各直接依赖清单），内容变化时解析为结构化列表并通过 `dsh-plugins-updated`
//! 事件实时推送给前端（`use-dsh-plugins` hook 消费）。
//!
//! 采用与主题轮询（`config/theme.rs`）一致的低频元数据兜底：mtime/大小未变
//! 时不读取 JSON；pnpm add/remove/install 期间的连续写盘由 2s 防抖合并。
//!
//! 模块划分参考 [`super::installed`]（预装插件检测）：installed 聚焦预设清单
//! 的勾选态，这里解析「实际已安装」的插件元信息（名称/版本/描述/仓库地址/
//! 是否启动加载），供前端做已安装列表展示与后续插件管理。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use super::errors::{self, PluginError};
use super::installed::{profile_dir, ProfilePackageJson};
use super::preset::{load_presets, PreinstallPluginInfo};

/// 前端监听的事件名（插件列表变化时推送）
pub(crate) const PLUGINS_UPDATED_EVENT: &str = "dsh-plugins-updated";

/// 防抖窗口：pnpm 安装/卸载会在数秒内连续写盘，窗口内只保留最新指纹，
/// 避免每个 tick 都推送一次中间态
const DEBOUNCE: Duration = Duration::from_secs(2);
const FULL_CONTENT_CHECK_TICKS: u16 = 30;

/// 已安装插件（序列化为 camelCase 给前端）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DshPlugin {
    /// 依赖键（npm 包名），前端主键
    pub id: String,
    /// 展示名：插件 package.json 的 name，缺失时回落预设清单/依赖键
    pub name: String,
    /// 已安装版本（解析失败时为空字符串）
    pub version: String,
    pub description: String,
    /// 仓库地址（repository.url / homepage），缺失时回落预设清单
    pub repo_url: String,
    /// 是否在 `dsh.profile.bundles` 中（启动时自动加载）
    pub bundled: bool,
    /// 预设清单中的「推荐」标记（绿色 chip）
    pub recommended: bool,
    /// 预设清单中的「修复」标记（黄色 chip）
    pub fix: bool,
    /// Studio 随包维护的第一方必需插件，普通管理界面不可升级或卸载。
    pub system: bool,
    /// 第一方插件随包携带的本地更新记录；第三方插件不由 Studio 解释此字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog: Option<String>,
    /// 异常信息（安装/升级/卸载失败或页面运行期上报）；`None` = 正常
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<PluginError>,
}

/// 用于强类型解析插件自身 package.json 的辅助结构
#[derive(Deserialize, Default)]
struct PluginPackageJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    repository: Option<RepositoryField>,
}

/// repository 字段兼容两种形态：字符串 URL 或 `{ "type": "git", "url": ... }` 对象
#[derive(Deserialize)]
#[serde(untagged)]
enum RepositoryField {
    Url(String),
    Object { url: Option<String> },
}

/// 插件在 node_modules 下的目录：`node_modules/<id>`（scoped 包 id 形如
/// `@scope/pkg`，join 会按分隔符展开成 `node_modules/@scope/pkg`）
fn plugin_dir(profile: &Path, id: &str) -> PathBuf {
    profile.join("node_modules").join(id)
}

/// 规范化仓库地址，便于系统浏览器直接打开：
/// `git+https://...` / `git://...` → `https://...`，去掉末尾 `.git`
fn normalize_repo_url(url: &str) -> String {
    let mut normalized = url.trim().to_string();
    if let Some(rest) = normalized.strip_prefix("git+") {
        normalized = rest.to_string();
    }
    if let Some(rest) = normalized.strip_prefix("git://") {
        normalized = format!("https://{rest}");
    }
    if let Some(rest) = normalized.strip_suffix(".git") {
        normalized = rest.to_string();
    }
    normalized
}

/// 读取并解析插件自身的 package.json；缺失/损坏时返回 None（不阻断整体解析）
fn read_plugin_meta(dir: &Path) -> Option<PluginPackageJson> {
    let content = std::fs::read_to_string(dir.join("package.json")).ok()?;
    serde_json::from_str(&content).ok()
}

/// 解析 profile 目录下实际已安装的插件列表（纯函数，便于单元测试）。
///
/// 只列出 profile package.json `dependencies` 中的直接依赖——node_modules 里
/// 还有大量传递依赖（clsx/zod 等），它们不是用户安装的 dsh 插件，不应展示。
fn parse_plugins(profile: &Path, presets: &[PreinstallPluginInfo]) -> Vec<DshPlugin> {
    let manifest_content = match std::fs::read_to_string(profile.join("package.json")) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };
    let manifest: ProfilePackageJson = match serde_json::from_str(&manifest_content) {
        Ok(manifest) => manifest,
        Err(_) => return Vec::new(),
    };

    let bundled: HashSet<&str> = manifest
        .dsh
        .as_ref()
        .and_then(|dsh| dsh.profile.as_ref())
        .map(|profile| profile.bundles.iter().map(String::as_str).collect())
        .unwrap_or_default();

    let mut preset_map: HashMap<&str, &PreinstallPluginInfo> = HashMap::new();
    for preset in presets {
        preset_map.insert(preset.id.as_str(), preset);
        if let Some(package) = preset.package.as_deref() {
            preset_map.insert(package, preset);
        }
    }

    let mut dep_ids: Vec<&String> = manifest.dependencies.keys().collect();
    // 稳定排序：启动加载（bundles）的插件在前，其余按 id 字典序
    dep_ids.sort_by_key(|id| (!bundled.contains(id.as_str()), id.as_str()));

    dep_ids
        .into_iter()
        .map(|id| {
            let preset = preset_map.get(id.as_str());
            let meta = read_plugin_meta(&plugin_dir(profile, id));
            let repo_url = meta
                .as_ref()
                .and_then(|m| match &m.repository {
                    Some(RepositoryField::Url(url)) => Some(url.clone()),
                    Some(RepositoryField::Object { url }) => url.clone(),
                    None => m.homepage.clone(),
                })
                .or_else(|| preset.map(|p| p.repo_url.clone()))
                .map(|url| normalize_repo_url(&url))
                .unwrap_or_default();
            let system = super::system::is_system_plugin(id);
            let locally_bundled = preset.is_some_and(|value| value.spec.starts_with("bundled:"));
            let changelog = if system || locally_bundled {
                std::fs::read_to_string(plugin_dir(profile, id).join("CHANGELOG.md")).ok()
            } else {
                None
            };
            DshPlugin {
                id: id.clone(),
                name: meta
                    .as_ref()
                    .and_then(|m| m.name.clone())
                    .or_else(|| preset.map(|p| p.name.clone()))
                    .unwrap_or_else(|| id.clone()),
                version: meta
                    .as_ref()
                    .and_then(|m| m.version.clone())
                    .unwrap_or_default(),
                description: meta
                    .as_ref()
                    .and_then(|m| m.description.clone())
                    .or_else(|| preset.map(|p| p.description.clone()))
                    .unwrap_or_default(),
                repo_url,
                bundled: bundled.contains(id.as_str()),
                recommended: preset.map(|p| p.recommended).unwrap_or(false),
                fix: preset.map(|p| p.fix).unwrap_or(false),
                system,
                changelog,
                error: None,
            }
        })
        .collect()
}

/// 已安装插件列表（含解析后的元信息与错误记录），前端首次加载/手动刷新用
pub fn list(app_handle: &AppHandle) -> Vec<DshPlugin> {
    let presets = load_presets(app_handle);
    let mut plugins = parse_plugins(&profile_dir(app_handle), &presets);
    // 合并错误注册表：错误记录变化不反映在文件指纹里，这里每次列表重建时并入
    let registry = errors::load(app_handle);
    for plugin in &mut plugins {
        plugin.error = registry.get(&plugin.id).cloned();
    }
    plugins
}

/// 主动推送一次插件列表（插件安装/升级/卸载/错误记录后调用，不等指纹轮询
/// 防抖；错误数据变化不改变文件指纹，必须显式推送）。
///
/// 同时把监控指纹同步到当前状态，避免紧接着的下一次轮询重复推送同一列表。
pub fn force_emit(app_handle: &AppHandle) {
    let snapshot = fingerprint_snapshot(app_handle);
    let mut state = STATE
        .get_or_init(|| {
            Mutex::new(WatchState {
                last_fp: None,
                last_emit: None,
                pending_fp: None,
                initialized: false,
                profile: None,
                paths: Vec::new(),
                stamps: Vec::new(),
                unchanged_ticks: 0,
            })
        })
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    state.pending_fp = None;
    state.last_fp = snapshot.fingerprint;
    state.paths = snapshot.paths;
    state.stamps = path_stamps(&state.paths);
    state.profile = Some(profile_dir(app_handle));
    state.initialized = true;
    state.unchanged_ticks = 0;
    drop(state);
    emit(app_handle);
}

/// 变化指纹：profile package.json 与各直接依赖插件 package.json 的内容拼接。
///
/// pnpm add/remove/install 会重写 profile 清单（依赖与 bundles）并落盘插件包，
/// 任一变化都会改变指纹；profile 未初始化（首次运行）时返回 None。
struct FingerprintSnapshot {
    fingerprint: Option<String>,
    paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathStamp {
    path: PathBuf,
    exists: bool,
    len: u64,
    modified: Option<std::time::SystemTime>,
}

fn path_stamps(paths: &[PathBuf]) -> Vec<PathStamp> {
    paths
        .iter()
        .map(|path| match std::fs::metadata(path) {
            Ok(metadata) => PathStamp {
                path: path.clone(),
                exists: true,
                len: metadata.len(),
                modified: metadata.modified().ok(),
            },
            Err(_) => PathStamp {
                path: path.clone(),
                exists: false,
                len: 0,
                modified: None,
            },
        })
        .collect()
}

fn profile_has_changed(previous: Option<&Path>, current: &Path) -> bool {
    previous != Some(current)
}

fn fingerprint_snapshot(app_handle: &AppHandle) -> FingerprintSnapshot {
    let dir = profile_dir(app_handle);
    let manifest_path = dir.join("package.json");
    let mut paths = vec![manifest_path.clone()];
    let Ok(manifest) = std::fs::read_to_string(&manifest_path) else {
        return FingerprintSnapshot {
            fingerprint: None,
            paths,
        };
    };
    let Ok(parsed) = serde_json::from_str::<ProfilePackageJson>(&manifest) else {
        return FingerprintSnapshot {
            fingerprint: None,
            paths,
        };
    };
    let mut dep_ids: Vec<&String> = parsed.dependencies.keys().collect();
    dep_ids.sort();

    let mut parts = vec![manifest];
    for id in dep_ids {
        let path = plugin_dir(&dir, id).join("package.json");
        paths.push(path.clone());
        if let Ok(content) = std::fs::read_to_string(path) {
            parts.push(content);
        }
    }
    FingerprintSnapshot {
        fingerprint: Some(parts.join("\n---\n")),
        paths,
    }
}

/// 监控状态：指纹 + 防抖窗口（仅 check_and_emit 单线程轮询访问）
struct WatchState {
    /// 上次已推送的指纹（内容一致则跳过）
    last_fp: Option<String>,
    /// 上次推送时间（用于防抖合并）
    last_emit: Option<Instant>,
    /// 防抖窗口内待推送的最新指纹
    pending_fp: Option<Option<String>>,
    /// 当前监控的活动 profile；切换后必须立刻重建路径与指纹。
    profile: Option<PathBuf>,
    /// 上次完整解析后需要观察的清单路径及其元数据。
    paths: Vec<PathBuf>,
    stamps: Vec<PathStamp>,
    initialized: bool,
    unchanged_ticks: u16,
}

static STATE: OnceLock<Mutex<WatchState>> = OnceLock::new();

/// 低频兜底轮询入口：先比较文件元数据，只有变化时才重读清单与计算指纹。
pub fn check_and_emit(app_handle: &AppHandle) {
    let mut state = STATE
        .get_or_init(|| {
            Mutex::new(WatchState {
                last_fp: None,
                last_emit: None,
                pending_fp: None,
                initialized: false,
                profile: None,
                paths: Vec::new(),
                stamps: Vec::new(),
                unchanged_ticks: 0,
            })
        })
        .lock()
        .unwrap_or_else(|error| error.into_inner());

    // 防抖窗口内已有变化时，即使文件不再变化也要在窗口结束后补推。
    if state.pending_fp.is_some()
        && state
            .last_emit
            .is_none_or(|last| last.elapsed() >= DEBOUNCE)
    {
        state.last_emit = Some(Instant::now());
        state.last_fp = state.pending_fp.take().unwrap_or(None);
        drop(state);
        emit(app_handle);
        return;
    }

    let current_profile = profile_dir(app_handle);
    let profile_changed = profile_has_changed(state.profile.as_deref(), &current_profile);
    let paths = if profile_changed || state.paths.is_empty() {
        vec![current_profile.join("package.json")]
    } else {
        state.paths.clone()
    };
    let stamps = path_stamps(&paths);
    if state.initialized && !profile_changed && state.stamps == stamps {
        state.unchanged_ticks = state.unchanged_ticks.saturating_add(1);
        if state.unchanged_ticks < FULL_CONTENT_CHECK_TICKS {
            return;
        }
    }

    let snapshot = fingerprint_snapshot(app_handle);
    state.profile = Some(current_profile);
    state.paths = snapshot.paths;
    state.stamps = path_stamps(&state.paths);
    state.initialized = true;
    state.unchanged_ticks = 0;
    let fp = snapshot.fingerprint;

    if state.last_fp.as_deref() == fp.as_deref() {
        return;
    }
    // 指纹变化：先记下待推送值，再判断是否已过防抖窗口（安装过程中连续
    // 变化时合并为一次推送，窗口结束前的变化会在后续 tick 补推）
    state.pending_fp = Some(fp);
    let can_emit = state
        .last_emit
        .is_none_or(|last| last.elapsed() >= DEBOUNCE);
    if !can_emit {
        return;
    }
    state.last_emit = Some(Instant::now());
    state.last_fp = state.pending_fp.take().unwrap_or(None);
    drop(state);
    emit(app_handle);
}

/// 解析并推送插件列表；profile 被移除（指纹为 None）时推送空列表让前端清空
fn emit(app_handle: &AppHandle) {
    let plugins = list(app_handle);
    log::debug!(
        "dsh plugins changed, emitting {} plugin(s) to frontend",
        plugins.len()
    );
    let _ = app_handle.emit(PLUGINS_UPDATED_EVENT, &plugins);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造临时 profile：package.json + node_modules 下的插件包清单
    /// （tag 用于区分不同测试的临时目录，避免并行执行时互相清理）
    fn build_profile(tag: &str, packages: &[(&str, &str)]) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dsh-watch-test-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("node_modules")).unwrap();
        let mut manifest = serde_json::json!({
            "name": "dsh-profile-web",
            "private": true,
            "dependencies": {},
            "dsh": { "profile": { "bundles": [] } }
        });
        let mut deps = serde_json::Map::new();
        let mut bundles = Vec::new();
        for (id, meta_json) in packages {
            deps.insert((*id).to_string(), serde_json::Value::String("1.0.0".into()));
            let pkg_dir = dir.join("node_modules").join(id);
            std::fs::create_dir_all(&pkg_dir).unwrap();
            std::fs::write(pkg_dir.join("package.json"), *meta_json).unwrap();
            if meta_json.contains("\"dsh\"") {
                bundles.push((*id).to_string());
            }
        }
        manifest["dependencies"] = serde_json::Value::Object(deps);
        manifest["dsh"]["profile"]["bundles"] =
            serde_json::Value::Array(bundles.into_iter().map(serde_json::Value::String).collect());
        std::fs::write(
            dir.join("package.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        dir
    }

    fn presets_for_test() -> Vec<PreinstallPluginInfo> {
        vec![PreinstallPluginInfo {
            id: "dshmarket".into(),
            spec: "dshmarket".into(),
            name: "DSH Market".into(),
            description: "Visual plugin market".into(),
            repo_url: "https://github.com/dsh-market/dsh-market".into(),
            recommended: true,
            fix: false,
            default_checked: false,
            win_only: false,
            package: None,
        }]
    }

    #[test]
    fn parse_plugins_lists_direct_deps_with_meta() {
        let dir = build_profile(
            "meta",
            &[
                (
                    "dshmarket",
                    r#"{"name":"dshmarket","version":"1.13.1","description":"market","repository":{"type":"git","url":"git+https://github.com/dsh-market/dsh-market.git"},"dsh":{"bundle":{}}}"#,
                ),
                (
                    "@anionex/dsh-turn-rewind",
                    r#"{"name":"@anionex/dsh-turn-rewind","version":"0.1.1","description":"rewind"}"#,
                ),
            ],
        );
        let plugins = parse_plugins(&dir, &presets_for_test());
        assert_eq!(plugins.len(), 2);

        let market = plugins.iter().find(|p| p.id == "dshmarket").unwrap();
        assert!(market.bundled);
        assert!(market.recommended);
        assert_eq!(market.version, "1.13.1");
        assert_eq!(market.repo_url, "https://github.com/dsh-market/dsh-market");

        let rewind = plugins
            .iter()
            .find(|p| p.id == "@anionex/dsh-turn-rewind")
            .unwrap();
        assert!(!rewind.bundled);
        assert!(!rewind.recommended);
        assert_eq!(rewind.name, "@anionex/dsh-turn-rewind");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_plugins_falls_back_to_preset_and_sorts_bundled_first() {
        let dir = build_profile(
            "fallback",
            &[
                ("dsh-at-file", r#"{"name":"dsh-at-file"}"#),
                ("dshmarket", r#"{"name":"dshmarket","dsh":{"bundle":{}}}"#),
            ],
        );
        let plugins = parse_plugins(&dir, &presets_for_test());
        // bundled（dshmarket）在前
        assert_eq!(plugins[0].id, "dshmarket");
        // 无版本/描述时回落预设清单
        let market = &plugins[0];
        assert_eq!(market.version, "");
        assert_eq!(market.description, "Visual plugin market");
        assert_eq!(market.repo_url, "https://github.com/dsh-market/dsh-market");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_plugins_returns_empty_without_manifest() {
        let dir = std::env::temp_dir().join(format!("dsh-watch-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(parse_plugins(&dir, &[]).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn system_plugin_exposes_bundled_changelog() {
        let dir = build_profile(
            "system-changelog",
            &[(
                super::super::system::PACKAGE_NAME,
                r#"{"name":"@mir3-studio/dsh-mir3-core","version":"0.2.0","dsh":{"client":{}}}"#,
            )],
        );
        let changelog = "# Updates\n\n## 0.2.0\n";
        std::fs::write(
            plugin_dir(&dir, super::super::system::PACKAGE_NAME).join("CHANGELOG.md"),
            changelog,
        )
        .unwrap();

        let plugins = parse_plugins(&dir, &[]);
        assert_eq!(plugins.len(), 1);
        assert!(plugins[0].system);
        assert_eq!(plugins[0].changelog.as_deref(), Some(changelog));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn metadata_stamp_detects_manifest_creation() {
        let dir = std::env::temp_dir().join(format!(
            "dsh-watch-stamp-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = dir.join("package.json");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let missing = path_stamps(std::slice::from_ref(&path));
        std::fs::write(&path, "{}").unwrap();
        let present = path_stamps(std::slice::from_ref(&path));
        assert_ne!(missing, present);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn active_profile_change_rebases_watched_paths() {
        let old = Path::new("profiles/old");
        let new = Path::new("profiles/new");
        assert!(!profile_has_changed(Some(old), old));
        assert!(profile_has_changed(Some(old), new));
        assert!(profile_has_changed(None, new));
    }
}
