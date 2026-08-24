use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Studio 记录的 996 项目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3Project {
    pub id: String,
    pub name: String,
    pub root: String,
    pub client_root: String,
    pub engine_root: String,
    pub active_workspace_root: String,
    pub engine_version: Option<String>,
    pub client_version: Option<String>,
    pub status: ProjectStatus,
    pub warnings: Vec<String>,
    pub last_scan_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    Valid,
    Warning,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectValidation {
    pub root: String,
    pub valid: bool,
    pub client_root: Option<String>,
    pub engine_root: Option<String>,
    pub engine_version: Option<String>,
    pub warnings: Vec<String>,
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

/// 解析并验证 996 项目根；只读，不在项目内创建任何文件。
pub fn validate_project_root(path: &Path) -> Result<ProjectValidation, String> {
    let root = fs::canonicalize(path)
        .map_err(|e| format!("PROJECT_PATH_INVALID: {}: {e}", path.display()))?;
    if !root.is_dir() {
        return Err(format!("PROJECT_NOT_DIRECTORY: {}", root.display()));
    }
    let client = root.join("客户端");
    let engine = root.join("引擎");
    if !client.is_dir() || !engine.is_dir() {
        return Err(
            "PROJECT_LAYOUT_INVALID: 项目根目录必须直接包含“客户端”和“引擎”文件夹".to_string(),
        );
    }

    let mut warnings = Vec::new();
    if !engine.join("Mir200").is_dir() {
        warnings.push("未检测到 引擎/Mir200".to_string());
    }
    if !engine.join("GameCenter.exe").is_file() {
        warnings.push("未检测到 引擎/GameCenter.exe".to_string());
    }
    if !engine.join("Config.json").is_file() && !engine.join("Config.ini").is_file() {
        warnings.push("未检测到引擎 Config.json 或 Config.ini".to_string());
    }
    if !client.join("996M3_Client.exe").is_file() && !client.join("game.exe").is_file() {
        warnings.push("未检测到客户端启动程序".to_string());
    }
    if !client.join("dev").is_dir() {
        warnings.push("未检测到 客户端/dev，Lua 开发索引可能为空".to_string());
    }

    Ok(ProjectValidation {
        root: path_string(&root),
        valid: true,
        client_root: Some(path_string(&client)),
        engine_root: Some(path_string(&engine)),
        engine_version: detect_engine_version(&engine),
        warnings,
    })
}

pub fn project_from_validation(validation: ProjectValidation) -> Result<Mir3Project, String> {
    let root = PathBuf::from(&validation.root);
    let name = root
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "PROJECT_NAME_INVALID: 无法从目录读取项目名称".to_string())?
        .to_string();
    let now = now_millis();
    let client_root = validation
        .client_root
        .ok_or_else(|| "PROJECT_CLIENT_MISSING: 缺少客户端目录".to_string())?;
    let engine_root = validation
        .engine_root
        .ok_or_else(|| "PROJECT_ENGINE_MISSING: 缺少引擎目录".to_string())?;
    let status = if validation.warnings.is_empty() {
        ProjectStatus::Valid
    } else {
        ProjectStatus::Warning
    };
    Ok(Mir3Project {
        id: project_id_for_path(&root),
        name,
        root: validation.root.clone(),
        client_root,
        engine_root,
        active_workspace_root: validation.root,
        engine_version: validation.engine_version,
        client_version: None,
        status,
        warnings: validation.warnings,
        last_scan_at: None,
        created_at: now,
        updated_at: now,
    })
}

/// 项目 id 由规范路径稳定派生；同一路径重复导入保持幂等，移动后由重新关联保留旧 id。
pub fn project_id_for_path(path: &Path) -> String {
    let mut hasher = Sha256::new();
    #[cfg(windows)]
    hasher.update(path_string(path).to_lowercase().as_bytes());
    #[cfg(not(windows))]
    hasher.update(path_string(path).as_bytes());
    let digest = hasher.finalize();
    format!(
        "mir3-{}",
        digest[..10]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

/// 验证 Workspace 位于项目根内。canonicalize 同时解析符号链接和 Windows reparse point。
pub fn validate_workspace_path(project_root: &Path, candidate: &Path) -> Result<PathBuf, String> {
    let root = fs::canonicalize(project_root)
        .map_err(|e| format!("WORKSPACE_PROJECT_MISSING: {}: {e}", project_root.display()))?;
    let selected = fs::canonicalize(candidate)
        .map_err(|e| format!("WORKSPACE_PATH_INVALID: {}: {e}", candidate.display()))?;
    if !selected.is_dir() {
        return Err("WORKSPACE_NOT_DIRECTORY: 工作区必须是目录".to_string());
    }
    if !path_is_within(&root, &selected) {
        return Err("WORKSPACE_OUTSIDE_PROJECT: 工作区不能超出当前 996 项目".to_string());
    }
    Ok(selected)
}

pub(crate) fn path_is_within(root: &Path, selected: &Path) -> bool {
    #[cfg(windows)]
    {
        let root = path_string(root).to_lowercase();
        let selected = path_string(selected).to_lowercase();
        return selected == root
            || selected
                .strip_prefix(&root)
                .is_some_and(|tail| tail.starts_with(['\\', '/']));
    }
    #[cfg(not(windows))]
    selected.starts_with(root)
}

pub fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn detect_engine_version(engine: &Path) -> Option<String> {
    let candidates = ["mir_version.txt", "version.txt", "Config.json"];
    for name in candidates {
        let path = engine.join(name);
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        if name.ends_with(".json") {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                for key in ["version", "engineVersion", "engine_version"] {
                    if let Some(version) = value.get(key).and_then(|item| item.as_str()) {
                        if !version.trim().is_empty() {
                            return Some(version.trim().to_string());
                        }
                    }
                }
            }
        } else if let Some(line) = content.lines().map(str::trim).find(|line| !line.is_empty()) {
            return Some(line.chars().take(128).collect());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mir3-domain-{name}-{}", std::process::id()))
    }

    #[test]
    fn validates_chinese_996_layout_and_workspace_boundary() {
        let root = fixture("layout");
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200")).unwrap();
        fs::write(root.join("引擎/GameCenter.exe"), b"MZ").unwrap();
        fs::write(root.join("客户端/game.exe"), b"MZ").unwrap();
        let validation = validate_project_root(&root).unwrap();
        assert!(validation.valid);
        assert!(validate_workspace_path(&root, &root.join("客户端/dev")).is_ok());
        assert!(validate_workspace_path(&root, root.parent().unwrap()).is_err());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rejects_non_996_layout() {
        let root = fixture("invalid");
        fs::create_dir_all(&root).unwrap();
        assert!(validate_project_root(&root).is_err());
        fs::remove_dir_all(root).ok();
    }
}
