//! Harness 当前项目运行数据隔离。
//!
//! Harness 只支持单一 `$DSH_HOME`，而设置、插件与凭证属于全局数据，工作区、
//! Session 与 Agent 运行记录必须按 MIR3 项目隔离。本模块在核心停止期间，仅把
//! `$DSH_HOME/storages` 和 `$DSH_HOME/sessions` 两个运行目录切换到当前项目槽位；
//! 其余全局目录保持原路径，因此插件、模型和用户全局设置仍跨项目共享。

use mir3_domain::Mir3Project;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

const SCOPE_DIR: &str = "harness-projects";
const LEGACY_DIR: &str = "legacy-v1";
const UNBOUND_SCOPE: &str = "unbound";

/// 准备当前项目的 Harness 运行目录，并把旧的混合数据一次性按项目拆分。
pub fn prepare(
    dsh_home: &Path,
    projects: &[Mir3Project],
    active: Option<&Mir3Project>,
) -> Result<(), String> {
    let scopes = dsh_home.join(SCOPE_DIR);
    fs::create_dir_all(&scopes).map_err(|error| format!("HARNESS_SCOPE_CREATE_FAILED: {error}"))?;
    migrate_legacy_runtime(dsh_home, &scopes, projects)?;

    let scope_id = active
        .map(|project| project.id.as_str())
        .unwrap_or(UNBOUND_SCOPE);
    let scope = scopes.join(safe_scope_id(scope_id)?);
    let storages = scope.join("storages");
    let sessions = scope.join("sessions");
    fs::create_dir_all(&storages)
        .and_then(|_| fs::create_dir_all(&sessions))
        .map_err(|error| format!("HARNESS_SCOPE_CREATE_FAILED: {error}"))?;
    replace_directory_link(&dsh_home.join("storages"), &storages)?;
    replace_directory_link(&dsh_home.join("sessions"), &sessions)?;
    log::info!(
        "Harness runtime scope selected: project={}, root={}",
        scope_id,
        scope.display()
    );
    Ok(())
}

fn safe_scope_id(value: &str) -> Result<&str, String> {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Ok(value);
    }
    Err("HARNESS_SCOPE_ID_INVALID: project id contains unsupported characters".to_string())
}

fn migrate_legacy_runtime(
    dsh_home: &Path,
    scopes: &Path,
    projects: &[Mir3Project],
) -> Result<(), String> {
    let legacy = scopes.join(LEGACY_DIR);
    let legacy_storages = legacy.join("storages");
    let legacy_sessions = legacy.join("sessions");
    move_real_directory_once(&dsh_home.join("storages"), &legacy_storages)?;
    move_real_directory_once(&dsh_home.join("sessions"), &legacy_sessions)?;
    if !legacy.exists() {
        return Ok(());
    }

    let workspace_path = legacy_storages.join("workspace.json");
    let projection_path = legacy_storages.join("session_projcache.json");
    let workspace_document = read_json_if_exists(&workspace_path)?;
    let projection_document = read_json_if_exists(&projection_path)?;
    for project in projects {
        let project_scope = scopes.join(safe_scope_id(&project.id)?);
        let project_storages = project_scope.join("storages");
        let project_sessions = project_scope.join("sessions");
        fs::create_dir_all(&project_storages)
            .and_then(|_| fs::create_dir_all(&project_sessions))
            .map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))?;
        let mut session_ids = HashSet::new();
        if let Some(document) = workspace_document.as_ref() {
            let filtered =
                filter_workspace_document(document, Path::new(&project.root), &mut session_ids);
            write_json_if_missing(&project_storages.join("workspace.json"), &filtered)?;
        }
        if let Some(document) = projection_document.as_ref() {
            let filtered =
                filter_projection_document(document, Path::new(&project.root), &mut session_ids);
            write_json_if_missing(&project_storages.join("session_projcache.json"), &filtered)?;
        }
        copy_matching_sessions(&legacy_sessions, &project_sessions, &session_ids)?;
    }
    Ok(())
}

fn move_real_directory_once(source: &Path, destination: &Path) -> Result<(), String> {
    let Ok(metadata) = fs::symlink_metadata(source) else {
        return Ok(());
    };
    if metadata.file_type().is_symlink() {
        remove_directory_link(source)?;
        return Ok(());
    }
    if destination.exists() {
        return Err(format!(
            "HARNESS_SCOPE_MIGRATION_CONFLICT: both {} and {} exist",
            source.display(),
            destination.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))?;
    }
    fs::rename(source, destination)
        .map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))
}

fn filter_workspace_document(
    document: &Value,
    project_root: &Path,
    session_ids: &mut HashSet<String>,
) -> Value {
    let mut filtered = document.clone();
    let Some(workspaces) = filtered
        .pointer_mut("/tables/workspaces")
        .and_then(Value::as_object_mut)
    else {
        return filtered;
    };
    workspaces.retain(|_, workspace| {
        let owned = workspace
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| is_within(project_root, Path::new(path)));
        if owned {
            if let Some(ids) = workspace.get("sessionIds").and_then(Value::as_array) {
                session_ids.extend(ids.iter().filter_map(Value::as_str).map(str::to_string));
            }
        }
        owned
    });
    let workspace_ids: HashSet<String> = workspaces.keys().cloned().collect();
    if let Some(ids) = filtered
        .pointer_mut("/global/workspaceIds")
        .and_then(Value::as_array_mut)
    {
        ids.retain(|id| id.as_str().is_some_and(|id| workspace_ids.contains(id)));
    }
    if let Some(ids) = filtered
        .pointer_mut("/global/archivedSessionIds")
        .and_then(Value::as_array_mut)
    {
        ids.retain(|id| id.as_str().is_some_and(|id| session_ids.contains(id)));
    }
    filtered
}

fn filter_projection_document(
    document: &Value,
    project_root: &Path,
    session_ids: &mut HashSet<String>,
) -> Value {
    let mut filtered = document.clone();
    let Some(rows) = filtered
        .pointer_mut("/tables/sessions")
        .and_then(Value::as_object_mut)
    else {
        return filtered;
    };
    rows.retain(|session_id, row| {
        let owned = row
            .pointer("/identity/cwd")
            .and_then(Value::as_str)
            .is_some_and(|cwd| is_within(project_root, Path::new(cwd)));
        if owned {
            session_ids.insert(session_id.clone());
        }
        owned
    });
    filtered
}

fn is_within(root: &Path, candidate: &Path) -> bool {
    let root = normalize_path(root);
    let candidate = normalize_path(candidate);
    candidate == root
        || if root == "/" {
            candidate.starts_with('/')
        } else {
            candidate.starts_with(&format!("{root}/"))
        }
}

fn normalize_path(path: &Path) -> String {
    let mut lexical = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                lexical.pop();
            }
            _ => lexical.push(component.as_os_str()),
        }
    }
    let normalized = lexical.to_string_lossy().replace('\\', "/");
    let normalized = if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    };
    let trimmed = normalized.trim_end_matches('/');
    if trimmed.is_empty() && normalized.starts_with('/') {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

fn copy_matching_sessions(
    legacy_sessions: &Path,
    project_sessions: &Path,
    session_ids: &HashSet<String>,
) -> Result<(), String> {
    if !legacy_sessions.is_dir() || session_ids.is_empty() {
        return Ok(());
    }
    for cwd_entry in fs::read_dir(legacy_sessions)
        .map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))?
    {
        let cwd_entry =
            cwd_entry.map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))?;
        if !cwd_entry.path().is_dir() {
            continue;
        }
        for session_entry in fs::read_dir(cwd_entry.path())
            .map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))?
        {
            let session_entry = session_entry
                .map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))?;
            let session_id = session_entry.file_name().to_string_lossy().into_owned();
            if !session_ids.contains(&session_id) || !session_entry.path().is_dir() {
                continue;
            }
            let destination = project_sessions
                .join(cwd_entry.file_name())
                .join(session_entry.file_name());
            copy_directory_if_missing(&session_entry.path(), &destination)?;
        }
    }
    Ok(())
}

fn copy_directory_if_missing(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Ok(());
    }
    fs::create_dir_all(destination)
        .map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))?;
    for entry in
        fs::read_dir(source).map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))?
    {
        let entry = entry.map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))?;
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory_if_missing(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)
                .map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))?;
        }
    }
    Ok(())
}

fn read_json_if_exists(path: &Path) -> Result<Option<Value>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes =
        fs::read(path).map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))
}

fn write_json_if_missing(path: &Path, value: &Value) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))?;
    fs::write(path, bytes).map_err(|error| format!("HARNESS_SCOPE_MIGRATION_FAILED: {error}"))
}

fn replace_directory_link(link: &Path, target: &Path) -> Result<(), String> {
    if fs::symlink_metadata(link).is_ok() {
        remove_directory_link(link)?;
    }
    create_directory_link(target, link)
}

#[cfg(unix)]
fn create_directory_link(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link)
        .map_err(|error| format!("HARNESS_SCOPE_LINK_FAILED: {error}"))
}

#[cfg(windows)]
fn create_directory_link(target: &Path, link: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .creation_flags(0x08000000)
        .status()
        .map_err(|error| format!("HARNESS_SCOPE_LINK_FAILED: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "HARNESS_SCOPE_LINK_FAILED: mklink exited with {status}"
        ))
    }
}

#[cfg(unix)]
fn remove_directory_link(path: &Path) -> Result<(), String> {
    fs::remove_file(path).map_err(|error| format!("HARNESS_SCOPE_LINK_FAILED: {error}"))
}

#[cfg(windows)]
fn remove_directory_link(path: &Path) -> Result<(), String> {
    fs::remove_dir(path).map_err(|error| format!("HARNESS_SCOPE_LINK_FAILED: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use mir3_domain::ProjectStatus;
    use serde_json::json;

    #[test]
    fn workspace_and_projection_documents_are_split_by_project_root() {
        let workspace = json!({
            "unit": { "name": "workspace", "version": 2 },
            "global": {
                "workspaceIds": ["alpha", "beta"],
                "archivedSessionIds": ["session-alpha", "session-beta"]
            },
            "tables": { "workspaces": {
                "alpha": { "path": "/game/alpha", "sessionIds": ["session-alpha"] },
                "beta": { "path": "/game/beta", "sessionIds": ["session-beta"] }
            }}
        });
        let projection = json!({
            "tables": { "sessions": {
                "session-alpha": { "identity": { "cwd": "/game/alpha/client" } },
                "session-beta": { "identity": { "cwd": "/game/beta" } }
            }}
        });
        let mut sessions = HashSet::new();
        let workspace =
            filter_workspace_document(&workspace, Path::new("/game/alpha"), &mut sessions);
        let projection =
            filter_projection_document(&projection, Path::new("/game/alpha"), &mut sessions);

        assert!(workspace.pointer("/tables/workspaces/alpha").is_some());
        assert!(workspace.pointer("/tables/workspaces/beta").is_none());
        assert_eq!(
            workspace.pointer("/global/workspaceIds"),
            Some(&json!(["alpha"]))
        );
        assert!(projection
            .pointer("/tables/sessions/session-alpha")
            .is_some());
        assert!(projection
            .pointer("/tables/sessions/session-beta")
            .is_none());
        assert_eq!(sessions, HashSet::from(["session-alpha".to_string()]));
    }

    #[test]
    fn scope_id_rejects_path_traversal() {
        assert!(safe_scope_id("mir3-alpha_1").is_ok());
        assert!(safe_scope_id("../foreign").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn prepare_migrates_legacy_runtime_and_selects_only_the_active_project() {
        let root = std::env::temp_dir().join(format!(
            "mir3-harness-scope-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let alpha = root.join("projects/alpha");
        let beta = root.join("projects/beta");
        fs::create_dir_all(root.join("storages")).unwrap();
        fs::create_dir_all(root.join("sessions/cwd-alpha/session-alpha")).unwrap();
        fs::create_dir_all(root.join("sessions/cwd-beta/session-beta")).unwrap();
        fs::write(
            root.join("sessions/cwd-alpha/session-alpha/session.jsonl.zstd"),
            b"alpha",
        )
        .unwrap();
        fs::write(
            root.join("sessions/cwd-beta/session-beta/session.jsonl.zstd"),
            b"beta",
        )
        .unwrap();
        fs::write(
            root.join("storages/workspace.json"),
            serde_json::to_vec(&json!({
                "global": { "workspaceIds": ["alpha", "beta"], "archivedSessionIds": [] },
                "tables": { "workspaces": {
                    "alpha": { "path": alpha, "sessionIds": ["session-alpha"] },
                    "beta": { "path": beta, "sessionIds": ["session-beta"] }
                }}
            }))
            .unwrap(),
        )
        .unwrap();
        let projects = vec![
            fixture_project("mir3-alpha", &alpha),
            fixture_project("mir3-beta", &beta),
        ];

        prepare(&root, &projects, projects.first()).unwrap();

        let selected: Value =
            serde_json::from_slice(&fs::read(root.join("storages/workspace.json")).unwrap())
                .unwrap();
        assert!(selected.pointer("/tables/workspaces/alpha").is_some());
        assert!(selected.pointer("/tables/workspaces/beta").is_none());
        assert!(root
            .join("sessions/cwd-alpha/session-alpha/session.jsonl.zstd")
            .is_file());
        assert!(!root.join("sessions/cwd-beta/session-beta").exists());
        assert!(root
            .join("harness-projects/legacy-v1/sessions/cwd-beta/session-beta")
            .is_dir());

        prepare(&root, &projects, projects.get(1)).unwrap();
        let selected: Value =
            serde_json::from_slice(&fs::read(root.join("storages/workspace.json")).unwrap())
                .unwrap();
        assert!(selected.pointer("/tables/workspaces/alpha").is_none());
        assert!(selected.pointer("/tables/workspaces/beta").is_some());
        assert!(root
            .join("sessions/cwd-beta/session-beta/session.jsonl.zstd")
            .is_file());
        assert!(!root.join("sessions/cwd-alpha/session-alpha").exists());

        remove_directory_link(&root.join("storages")).unwrap();
        remove_directory_link(&root.join("sessions")).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    fn fixture_project(id: &str, root: &Path) -> Mir3Project {
        Mir3Project {
            id: id.to_string(),
            name: id.to_string(),
            root: root.to_string_lossy().into_owned(),
            client_root: root.join("客户端").to_string_lossy().into_owned(),
            engine_root: root.join("引擎").to_string_lossy().into_owned(),
            active_workspace_root: root.to_string_lossy().into_owned(),
            engine_version: Some("1.8".to_string()),
            client_version: None,
            status: ProjectStatus::Valid,
            warnings: Vec::new(),
            last_scan_at: None,
            created_at: 1,
            updated_at: 1,
        }
    }
}
