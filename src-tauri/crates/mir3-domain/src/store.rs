use crate::{
    now_millis, path_string, project_from_validation, validate_project_root,
    validate_workspace_path, Mir3Project, ProjectStatus,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::safe_files::CachedXlsWorkbook;

const SCHEMA_VERSION: i64 = 1;

/// 领域数据入口；实际项目只读，所有产品数据写入 data_root。
#[derive(Debug, Clone)]
pub struct DomainStore {
    data_root: PathBuf,
    pub(crate) xls_cache: Arc<Mutex<HashMap<String, Arc<CachedXlsWorkbook>>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDirectory {
    pub path: String,
    pub name: String,
    pub project_root: bool,
}

impl DomainStore {
    pub fn new(data_root: impl Into<PathBuf>) -> Result<Self, String> {
        let store = Self {
            data_root: data_root.into(),
            xls_cache: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&store.data_root)
            .map_err(|e| format!("PROJECT_DATA_CREATE_FAILED: {e}"))?;
        store.init_registry()?;
        Ok(store)
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn project_dir(&self, project_id: &str) -> Result<PathBuf, String> {
        validate_id(project_id)?;
        Ok(self.data_root.join(project_id))
    }

    pub fn project_db_path(&self, project_id: &str) -> Result<PathBuf, String> {
        Ok(self.project_dir(project_id)?.join("project.sqlite"))
    }

    pub fn import_project(&self, path: &Path) -> Result<Mir3Project, String> {
        let validation = validate_project_root(path)?;
        let mut project = project_from_validation(validation)?;
        if let Some(existing) = self.project_by_root(&project.root)? {
            project.id = existing.id;
            project.created_at = existing.created_at;
            project.active_workspace_root = existing.active_workspace_root;
            project.last_scan_at = existing.last_scan_at;
        }
        self.prepare_project_storage(&project.id)?;
        self.upsert_project(&project)?;
        self.activate_project(&project.id)?;
        Ok(project)
    }

    pub fn list_projects(&self) -> Result<Vec<Mir3Project>, String> {
        let connection = self.registry()?;
        let mut statement = connection
            .prepare(
                "SELECT id,name,root,client_root,engine_root,active_workspace_root,engine_version,client_version,status,warnings,last_scan_at,created_at,updated_at
                 FROM projects ORDER BY updated_at DESC, name COLLATE NOCASE",
            )
            .map_err(db_error)?;
        let rows = statement.query_map([], row_to_project).map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
    }

    pub fn get_project(&self, project_id: &str) -> Result<Mir3Project, String> {
        validate_id(project_id)?;
        self.registry()?
            .query_row(
                "SELECT id,name,root,client_root,engine_root,active_workspace_root,engine_version,client_version,status,warnings,last_scan_at,created_at,updated_at FROM projects WHERE id=?1",
                [project_id],
                row_to_project,
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| format!("PROJECT_NOT_FOUND: {project_id}"))
    }

    pub fn active_project(&self) -> Result<Option<Mir3Project>, String> {
        let connection = self.registry()?;
        let active: Option<String> = connection
            .query_row(
                "SELECT value FROM metadata WHERE key='active_project_id'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?;
        active.map(|id| self.get_project(&id)).transpose()
    }

    pub fn activate_project(&self, project_id: &str) -> Result<Mir3Project, String> {
        let project = self.get_project(project_id)?;
        self.registry()?
            .execute(
                "INSERT INTO metadata(key,value) VALUES('active_project_id',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [project_id],
            )
            .map_err(db_error)?;
        Ok(project)
    }

    pub fn relink_project(&self, project_id: &str, path: &Path) -> Result<Mir3Project, String> {
        let existing = self.get_project(project_id)?;
        let validation = validate_project_root(path)?;
        let mut project = project_from_validation(validation)?;
        project.id = existing.id;
        project.created_at = existing.created_at;
        project.last_scan_at = existing.last_scan_at;
        self.upsert_project(&project)?;
        Ok(project)
    }

    pub fn remove_project(&self, project_id: &str) -> Result<(), String> {
        validate_id(project_id)?;
        let connection = self.registry()?;
        let changed = connection
            .execute("DELETE FROM projects WHERE id=?1", [project_id])
            .map_err(db_error)?;
        if changed == 0 {
            return Err(format!("PROJECT_NOT_FOUND: {project_id}"));
        }
        connection
            .execute(
                "DELETE FROM metadata WHERE key='active_project_id' AND value=?1",
                [project_id],
            )
            .map_err(db_error)?;
        Ok(())
    }

    pub fn validate_project(&self, project_id: &str) -> Result<Mir3Project, String> {
        let existing = self.get_project(project_id)?;
        match validate_project_root(Path::new(&existing.root)) {
            Ok(validation) => {
                let mut refreshed = project_from_validation(validation)?;
                refreshed.id = existing.id;
                refreshed.created_at = existing.created_at;
                refreshed.active_workspace_root = existing.active_workspace_root;
                refreshed.last_scan_at = existing.last_scan_at;
                self.upsert_project(&refreshed)?;
                Ok(refreshed)
            }
            Err(error) => {
                let mut missing = existing;
                missing.status = ProjectStatus::Missing;
                missing.warnings = vec![error];
                missing.updated_at = now_millis();
                self.upsert_project(&missing)?;
                Ok(missing)
            }
        }
    }

    pub fn select_workspace(&self, project_id: &str, path: &Path) -> Result<Mir3Project, String> {
        let mut project = self.get_project(project_id)?;
        let selected = validate_workspace_path(Path::new(&project.root), path)?;
        project.active_workspace_root = path_string(&selected);
        project.updated_at = now_millis();
        self.upsert_project(&project)?;
        Ok(project)
    }

    /// 返回项目根或指定父目录下一层目录，供受限目录浏览器使用。
    pub fn workspace_directories(
        &self,
        project_id: &str,
        parent: Option<&Path>,
    ) -> Result<Vec<WorkspaceDirectory>, String> {
        let project = self.get_project(project_id)?;
        let root = PathBuf::from(&project.root);
        let selected = match parent {
            Some(parent) => validate_workspace_path(&root, parent)?,
            None => {
                fs::canonicalize(&root).map_err(|e| format!("WORKSPACE_PROJECT_MISSING: {e}"))?
            }
        };
        let mut directories = Vec::new();
        if selected
            == fs::canonicalize(&root).map_err(|e| format!("WORKSPACE_PROJECT_MISSING: {e}"))?
        {
            directories.push(WorkspaceDirectory {
                path: path_string(&selected),
                name: project.name.clone(),
                project_root: true,
            });
        }
        let entries = fs::read_dir(&selected)
            .map_err(|e| format!("WORKSPACE_LIST_FAILED: {}: {e}", selected.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Ok(canonical) = validate_workspace_path(&root, &path) else {
                continue;
            };
            directories.push(WorkspaceDirectory {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: path_string(&canonical),
                project_root: false,
            });
        }
        directories.sort_by(|left, right| {
            right
                .project_root
                .cmp(&left.project_root)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(directories)
    }

    pub(crate) fn project_connection(&self, project_id: &str) -> Result<Connection, String> {
        let path = self.project_db_path(project_id)?;
        let connection = Connection::open(path).map_err(db_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
            .map_err(db_error)?;
        Ok(connection)
    }

    pub(crate) fn update_last_scan(&self, project_id: &str, at: i64) -> Result<(), String> {
        self.registry()?
            .execute(
                "UPDATE projects SET last_scan_at=?2, updated_at=?2 WHERE id=?1",
                params![project_id, at],
            )
            .map_err(db_error)?;
        Ok(())
    }

    fn project_by_root(&self, root: &str) -> Result<Option<Mir3Project>, String> {
        self.registry()?
            .query_row(
                "SELECT id,name,root,client_root,engine_root,active_workspace_root,engine_version,client_version,status,warnings,last_scan_at,created_at,updated_at FROM projects WHERE root=?1",
                [root],
                row_to_project,
            )
            .optional()
            .map_err(db_error)
    }

    fn upsert_project(&self, project: &Mir3Project) -> Result<(), String> {
        let warnings = serde_json::to_string(&project.warnings)
            .map_err(|e| format!("PROJECT_WARNINGS_SERIALIZE_FAILED: {e}"))?;
        self.registry()?
            .execute(
                "INSERT INTO projects(id,name,root,client_root,engine_root,active_workspace_root,engine_version,client_version,status,warnings,last_scan_at,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name,root=excluded.root,client_root=excluded.client_root,engine_root=excluded.engine_root,active_workspace_root=excluded.active_workspace_root,engine_version=excluded.engine_version,client_version=excluded.client_version,status=excluded.status,warnings=excluded.warnings,last_scan_at=excluded.last_scan_at,updated_at=excluded.updated_at",
                params![
                    project.id,
                    project.name,
                    project.root,
                    project.client_root,
                    project.engine_root,
                    project.active_workspace_root,
                    project.engine_version,
                    project.client_version,
                    status_string(project.status),
                    warnings,
                    project.last_scan_at,
                    project.created_at,
                    project.updated_at,
                ],
            )
            .map_err(db_error)?;
        Ok(())
    }

    fn prepare_project_storage(&self, project_id: &str) -> Result<(), String> {
        let dir = self.project_dir(project_id)?;
        for name in ["wiki", "drafts", "snapshots", "logs"] {
            fs::create_dir_all(dir.join(name))
                .map_err(|e| format!("PROJECT_DATA_CREATE_FAILED: {e}"))?;
        }
        let connection = Connection::open(dir.join("project.sqlite")).map_err(db_error)?;
        connection.execute_batch(PROJECT_SCHEMA).map_err(db_error)?;
        Ok(())
    }

    fn registry(&self) -> Result<Connection, String> {
        let connection =
            Connection::open(self.data_root.join("registry.sqlite")).map_err(db_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
            .map_err(db_error)?;
        Ok(connection)
    }

    fn init_registry(&self) -> Result<(), String> {
        let connection = self.registry()?;
        connection
            .execute_batch(REGISTRY_SCHEMA)
            .map_err(db_error)?;
        connection
            .execute(
                "INSERT INTO metadata(key,value) VALUES('schema_version',?1) ON CONFLICT(key) DO NOTHING",
                [SCHEMA_VERSION.to_string()],
            )
            .map_err(db_error)?;
        Ok(())
    }
}

const REGISTRY_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS projects(
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  root TEXT NOT NULL UNIQUE,
  client_root TEXT NOT NULL,
  engine_root TEXT NOT NULL,
  active_workspace_root TEXT NOT NULL,
  engine_version TEXT,
  client_version TEXT,
  status TEXT NOT NULL,
  warnings TEXT NOT NULL DEFAULT '[]',
  last_scan_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
"#;

const PROJECT_SCHEMA: &str = r#"
PRAGMA foreign_keys=ON;
PRAGMA journal_mode=WAL;
CREATE TABLE IF NOT EXISTS files(
  path TEXT PRIMARY KEY,
  role TEXT NOT NULL,
  category TEXT NOT NULL,
  extension TEXT,
  size INTEGER NOT NULL,
  modified_at INTEGER NOT NULL,
  sha256 TEXT,
  content TEXT
);
CREATE INDEX IF NOT EXISTS idx_files_category ON files(category);
CREATE INDEX IF NOT EXISTS idx_files_role ON files(role);
CREATE TABLE IF NOT EXISTS knowledge(
  id TEXT PRIMARY KEY,
  status TEXT NOT NULL,
  kind TEXT NOT NULL,
  summary TEXT NOT NULL,
  body TEXT NOT NULL,
  engine_version TEXT,
  evidence TEXT NOT NULL DEFAULT '[]',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_knowledge_status ON knowledge(status);
CREATE TABLE IF NOT EXISTS drafts(
  id TEXT PRIMARY KEY,
  intent TEXT NOT NULL,
  revision INTEGER NOT NULL,
  status TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS draft_changes(
  draft_id TEXT NOT NULL,
  path TEXT NOT NULL,
  base_sha256 TEXT,
  content BLOB,
  deleted INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(draft_id,path),
  FOREIGN KEY(draft_id) REFERENCES drafts(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS snapshots(
  id TEXT PRIMARY KEY,
  draft_id TEXT,
  manifest TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
"#;

fn row_to_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<Mir3Project> {
    let status: String = row.get(8)?;
    let warnings: String = row.get(9)?;
    Ok(Mir3Project {
        id: row.get(0)?,
        name: row.get(1)?,
        root: row.get(2)?,
        client_root: row.get(3)?,
        engine_root: row.get(4)?,
        active_workspace_root: row.get(5)?,
        engine_version: row.get(6)?,
        client_version: row.get(7)?,
        status: parse_status(&status),
        warnings: serde_json::from_str(&warnings).unwrap_or_default(),
        last_scan_at: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn status_string(status: ProjectStatus) -> &'static str {
    match status {
        ProjectStatus::Valid => "valid",
        ProjectStatus::Warning => "warning",
        ProjectStatus::Missing => "missing",
    }
}

fn parse_status(status: &str) -> ProjectStatus {
    match status {
        "valid" => ProjectStatus::Valid,
        "warning" => ProjectStatus::Warning,
        _ => ProjectStatus::Missing,
    }
}

fn validate_id(value: &str) -> Result<(), String> {
    if value.starts_with("mir3-")
        && value.len() <= 64
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        Ok(())
    } else {
        Err("PROJECT_ID_INVALID: invalid project id".to_string())
    }
}

fn db_error(error: rusqlite::Error) -> String {
    format!("PROJECT_DATABASE_FAILED: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_is_idempotent_and_data_stays_outside_project() {
        let base = std::env::temp_dir().join(format!("mir3-store-{}", std::process::id()));
        let project = base.join("项目/木立");
        let data = base.join("data");
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200")).unwrap();
        let store = DomainStore::new(&data).unwrap();
        let first = store.import_project(&project).unwrap();
        let second = store.import_project(&project).unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(store.list_projects().unwrap().len(), 1);
        assert!(!project.join(".mir3-ai").exists());
        assert!(data.join(&first.id).join("project.sqlite").is_file());
        fs::remove_dir_all(base).ok();
    }
}
