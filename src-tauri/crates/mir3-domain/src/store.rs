use crate::{
    now_millis, path_string, project_from_validation, validate_project_root,
    validate_workspace_path, Mir3Project, ProjectStatus,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};

use crate::safe_files::CachedXlsWorkbook;
use fs2::FileExt;
use sha2::{Digest, Sha256};

const SCHEMA_VERSION: i64 = 2;

#[cfg(test)]
type TestBarrierGate = Arc<Mutex<Option<(Arc<std::sync::Barrier>, Arc<std::sync::Barrier>)>>>;

/// 领域数据入口；实际项目只读，所有产品数据写入 data_root。
#[derive(Debug, Clone)]
pub struct DomainStore {
    data_root: PathBuf,
    domain_pack_root: PathBuf,
    read_only_reason: Option<Arc<str>>,
    pub(crate) xls_cache: Arc<Mutex<HashMap<String, Arc<CachedXlsWorkbook>>>>,
    draft_mutation_reservations: Arc<Mutex<HashMap<String, DraftMutationReservationState>>>,
    #[cfg(test)]
    pub(crate) composite_apply_test_barrier: TestBarrierGate,
    #[cfg(test)]
    pub(crate) composite_apply_crash_after_writes: Arc<std::sync::atomic::AtomicIsize>,
    #[cfg(test)]
    pub(crate) composite_apply_crash_after_commit: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(test)]
    pub(crate) composite_capability_crash_after_operation: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(test)]
    pub(crate) snapshot_restore_crash_after_files: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(test)]
    pub(crate) governance_copy_test_gate: TestBarrierGate,
    #[cfg(test)]
    pub(crate) trusted_fixture_engine_override: bool,
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
        let data_root = data_root.into();
        let domain_pack_root = data_root.join("domain-packs");
        Self::new_with_domain_pack_root(data_root, domain_pack_root)
    }

    /// 项目数据库与可升级领域包必须显式分根，避免运行时误读编译期内置包。
    pub fn new_with_domain_pack_root(
        data_root: impl Into<PathBuf>,
        domain_pack_root: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let mut store = Self {
            data_root: data_root.into(),
            domain_pack_root: domain_pack_root.into(),
            read_only_reason: None,
            xls_cache: Arc::new(Mutex::new(HashMap::new())),
            draft_mutation_reservations: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            composite_apply_test_barrier: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            composite_apply_crash_after_writes: Arc::new(std::sync::atomic::AtomicIsize::new(-1)),
            #[cfg(test)]
            composite_apply_crash_after_commit: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(test)]
            composite_capability_crash_after_operation: Arc::new(
                std::sync::atomic::AtomicBool::new(false),
            ),
            #[cfg(test)]
            snapshot_restore_crash_after_files: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(test)]
            governance_copy_test_gate: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            trusted_fixture_engine_override: false,
        };
        fs::create_dir_all(&store.data_root)
            .map_err(|e| format!("PROJECT_DATA_CREATE_FAILED: {e}"))?;
        let registry_existed = store.data_root.join("registry.sqlite").is_file();
        if let Err(error) = store.init_registry() {
            if !registry_existed {
                return Err(error);
            }
            store.read_only_reason = Some(Arc::from(error));
            return Ok(store);
        }
        if let Err(error) = store.migrate_existing_projects() {
            store.read_only_reason = Some(Arc::from(error));
        } else if let Err(error) = store.recover_composite_apply_journals() {
            store.read_only_reason = Some(Arc::from(error));
        } else if let Err(error) = store.recover_composite_capability_journals() {
            store.read_only_reason = Some(Arc::from(error));
        } else if let Err(error) = store.recover_snapshot_governance_journals() {
            store.read_only_reason = Some(Arc::from(error));
        }
        Ok(store)
    }

    /// 仅单元测试可显式声明其临时项目是受信 fixture；生产构建不存在此入口和字段。
    #[cfg(test)]
    pub(crate) fn new_trusted_fixture(data_root: impl Into<PathBuf>) -> Result<Self, String> {
        let data_root = data_root.into();
        let domain_pack_root = data_root.join("domain-packs");
        Self::new_trusted_fixture_with_domain_pack_root(data_root, domain_pack_root)
    }

    /// 带独立领域包根的受信测试 fixture 构造器。
    #[cfg(test)]
    pub(crate) fn new_trusted_fixture_with_domain_pack_root(
        data_root: impl Into<PathBuf>,
        domain_pack_root: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let mut store = Self::new_with_domain_pack_root(data_root, domain_pack_root)?;
        store.trusted_fixture_engine_override = true;
        Ok(store)
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn domain_pack_root(&self) -> &Path {
        &self.domain_pack_root
    }

    /// 未知 Schema 或迁移失败时保留读取能力，同时由同一连接层拒绝全部写入。
    pub fn read_only_reason(&self) -> Option<&str> {
        self.read_only_reason.as_deref()
    }

    pub(crate) fn ensure_writable(&self) -> Result<(), String> {
        match &self.read_only_reason {
            Some(reason) => Err(format!("DOMAIN_KERNEL_READONLY: {reason}")),
            None => Ok(()),
        }
    }

    pub fn project_dir(&self, project_id: &str) -> Result<PathBuf, String> {
        validate_id(project_id)?;
        Ok(self.data_root.join(project_id))
    }

    pub fn project_db_path(&self, project_id: &str) -> Result<PathBuf, String> {
        Ok(self.project_dir(project_id)?.join("project.sqlite"))
    }

    pub fn import_project(&self, path: &Path) -> Result<Mir3Project, String> {
        self.ensure_writable()?;
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
        self.ensure_writable()?;
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
        self.ensure_writable()?;
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
        self.ensure_writable()?;
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
        self.ensure_writable()?;
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
        self.ensure_writable()?;
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
        let connection = if self.read_only_reason.is_some() {
            Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(db_error)?
        } else {
            Connection::open(path).map_err(db_error)?
        };
        configure_connection(&connection, self.read_only_reason.is_some())?;
        Ok(connection)
    }

    pub(crate) fn update_last_scan(&self, project_id: &str, at: i64) -> Result<(), String> {
        self.ensure_writable()?;
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
        self.ensure_writable()?;
        let dir = self.project_dir(project_id)?;
        for name in ["wiki", "drafts", "snapshots", "logs"] {
            fs::create_dir_all(dir.join(name))
                .map_err(|e| format!("PROJECT_DATA_CREATE_FAILED: {e}"))?;
        }
        let path = dir.join("project.sqlite");
        migrate_database(&path, PROJECT_SCHEMA, "PROJECT_DATABASE")?;
        Connection::open(&path)
            .map_err(db_error)?
            .execute(
                "INSERT OR IGNORE INTO draft_domains(draft_id,legacy)
                 SELECT id,1 FROM drafts",
                [],
            )
            .map_err(db_error)?;
        Ok(())
    }

    pub(crate) fn registry(&self) -> Result<Connection, String> {
        let path = self.data_root.join("registry.sqlite");
        let connection = if self.read_only_reason.is_some() {
            Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(db_error)?
        } else {
            Connection::open(path).map_err(db_error)?
        };
        configure_connection(&connection, self.read_only_reason.is_some())?;
        Ok(connection)
    }

    fn init_registry(&self) -> Result<(), String> {
        let path = self.data_root.join("registry.sqlite");
        migrate_database(&path, REGISTRY_SCHEMA, "REGISTRY_DATABASE")
    }

    fn migrate_existing_projects(&self) -> Result<(), String> {
        for project in self.list_projects()? {
            self.prepare_project_storage(&project.id)?;
        }
        Ok(())
    }

    pub(crate) fn reserve_draft_mutations(
        &self,
        project_id: &str,
        draft_ids: &[String],
    ) -> Result<DraftMutationReservation, String> {
        let lock_root = self.project_dir(project_id)?.join("draft-locks");
        fs::create_dir_all(&lock_root)
            .map_err(|error| format!("DRAFT_RESERVATION_DIRECTORY_FAILED: {error}"))?;
        let targets = draft_ids
            .iter()
            .map(|draft_id| {
                (
                    draft_reservation_key(project_id, draft_id),
                    draft_lock_path(&lock_root, draft_id),
                )
            })
            .collect::<Vec<_>>();
        self.reserve_mutation_targets(targets)
    }

    /// 组合成员绑定、联合 Apply 与恢复共享项目级跨进程锁，避免集合检查后再并发加入 Draft。
    pub(crate) fn reserve_composite_mutation(
        &self,
        project_id: &str,
    ) -> Result<DraftMutationReservation, String> {
        let lock_root = self.project_dir(project_id)?.join("draft-locks");
        fs::create_dir_all(&lock_root)
            .map_err(|error| format!("COMPOSITE_RESERVATION_DIRECTORY_FAILED: {error}"))?;
        self.reserve_mutation_targets(vec![(
            format!("{project_id}:__composite__"),
            lock_root.join("composite.lock"),
        )])
    }

    fn reserve_mutation_targets(
        &self,
        mut targets: Vec<(String, PathBuf)>,
    ) -> Result<DraftMutationReservation, String> {
        let owner = thread::current().id();
        targets.sort_by(|left, right| left.0.cmp(&right.0));
        targets.dedup_by(|left, right| left.0 == right.0);
        let mut reservations = self
            .draft_mutation_reservations
            .lock()
            .map_err(|_| "DRAFT_RESERVATION_LOCK_FAILED: reservation lock poisoned".to_string())?;
        let mut keys = Vec::with_capacity(targets.len());
        for (key, path) in targets {
            if let Some(reservation) = reservations.get_mut(&key) {
                if reservation.owner != owner {
                    release_draft_reservations(&mut reservations, &keys, owner);
                    return Err(format!("DRAFT_MUTATION_RESERVED: {key}"));
                }
                reservation.depth += 1;
                keys.push(key);
                continue;
            }
            let file = match fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(&path)
            {
                Ok(file) => file,
                Err(error) => {
                    release_draft_reservations(&mut reservations, &keys, owner);
                    return Err(format!("DRAFT_RESERVATION_OPEN_FAILED: {error}"));
                }
            };
            if let Err(error) = file.try_lock_exclusive() {
                release_draft_reservations(&mut reservations, &keys, owner);
                return Err(format!("DRAFT_MUTATION_RESERVED: {key}: {error}"));
            }
            reservations.insert(
                key.clone(),
                DraftMutationReservationState {
                    owner,
                    depth: 1,
                    _file: file,
                },
            );
            keys.push(key);
        }
        drop(reservations);
        Ok(DraftMutationReservation {
            reservations: self.draft_mutation_reservations.clone(),
            keys,
            owner,
        })
    }

    pub(crate) fn reserve_draft_mutation(
        &self,
        project_id: &str,
        draft_id: &str,
    ) -> Result<DraftMutationReservation, String> {
        self.reserve_draft_mutations(project_id, &[draft_id.to_string()])
    }

    #[cfg(test)]
    pub(crate) fn wait_governance_copy_test_gate(&self) -> Result<(), String> {
        let gate = self
            .governance_copy_test_gate
            .lock()
            .map_err(|_| "GOVERNANCE_COPY_TEST_GATE_FAILED: gate lock poisoned".to_string())?
            .clone();
        if let Some((entered, release)) = gate {
            entered.wait();
            release.wait();
        }
        Ok(())
    }
}

pub(crate) struct DraftMutationReservation {
    reservations: Arc<Mutex<HashMap<String, DraftMutationReservationState>>>,
    keys: Vec<String>,
    owner: ThreadId,
}

#[derive(Debug)]
struct DraftMutationReservationState {
    owner: ThreadId,
    depth: usize,
    _file: fs::File,
}

impl Drop for DraftMutationReservation {
    fn drop(&mut self) {
        if let Ok(mut reservations) = self.reservations.lock() {
            for key in &self.keys {
                let remove = reservations.get_mut(key).is_some_and(|reservation| {
                    if reservation.owner != self.owner {
                        return false;
                    }
                    reservation.depth -= 1;
                    reservation.depth == 0
                });
                if remove {
                    reservations.remove(key);
                }
            }
        }
    }
}

fn draft_reservation_key(project_id: &str, draft_id: &str) -> String {
    format!("{project_id}:{draft_id}")
}

fn draft_lock_path(root: &Path, draft_id: &str) -> PathBuf {
    let mut digest = Sha256::new();
    digest.update(draft_id.as_bytes());
    root.join(format!("{:x}.lock", digest.finalize()))
}

fn release_draft_reservations(
    reservations: &mut HashMap<String, DraftMutationReservationState>,
    keys: &[String],
    owner: ThreadId,
) {
    for key in keys.iter().rev() {
        let remove = reservations.get_mut(key).is_some_and(|reservation| {
            if reservation.owner != owner {
                return false;
            }
            reservation.depth -= 1;
            reservation.depth == 0
        });
        if remove {
            reservations.remove(key);
        }
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
CREATE TABLE IF NOT EXISTS shared_user_capabilities(
  scope TEXT NOT NULL,
  id TEXT NOT NULL,
  version TEXT NOT NULL,
  source_project_id TEXT NOT NULL,
  system_id TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL,
  parameter_schema TEXT NOT NULL,
  steps TEXT NOT NULL,
  read_systems TEXT NOT NULL,
  write_systems TEXT NOT NULL,
  status TEXT NOT NULL,
  source_task_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(scope,id,version)
);
CREATE INDEX IF NOT EXISTS idx_shared_user_capabilities_lookup
  ON shared_user_capabilities(system_id,scope,status,id,version);
CREATE TABLE IF NOT EXISTS shared_domain_memories(
  scope TEXT NOT NULL,
  id TEXT NOT NULL,
  source_project_id TEXT NOT NULL,
  system_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  summary TEXT NOT NULL,
  body TEXT NOT NULL,
  status TEXT NOT NULL,
  source_task_id TEXT NOT NULL,
  plugin_version TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(scope,id)
);
CREATE INDEX IF NOT EXISTS idx_shared_domain_memories_lookup
  ON shared_domain_memories(system_id,scope,status,updated_at);
CREATE TABLE IF NOT EXISTS domain_governance_migrations(
  id TEXT PRIMARY KEY,
  system_id TEXT NOT NULL,
  from_version TEXT NOT NULL,
  to_version TEXT NOT NULL,
  status TEXT NOT NULL,
  report TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
"#;

const PROJECT_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
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
CREATE TABLE IF NOT EXISTS draft_operation_evidence(
  draft_id TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  system_id TEXT NOT NULL,
  plugin_version TEXT NOT NULL,
  operation_id TEXT NOT NULL,
  parameters TEXT NOT NULL,
  parameter_schema_hash TEXT NOT NULL,
  revision_before INTEGER NOT NULL,
  revision_after INTEGER NOT NULL,
  replay_change_hash TEXT NOT NULL DEFAULT '',
  replay_evidence_hash TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  PRIMARY KEY(draft_id,sequence),
  FOREIGN KEY(draft_id) REFERENCES drafts(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS snapshots(
  id TEXT PRIMARY KEY,
  draft_id TEXT,
  manifest TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS task_receipts(
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  system_id TEXT NOT NULL,
  summary TEXT NOT NULL,
  status TEXT NOT NULL,
  draft_id TEXT,
  plugin_versions TEXT NOT NULL DEFAULT '{}',
  evidence TEXT NOT NULL DEFAULT '{}',
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_task_receipts_system ON task_receipts(system_id,created_at);
CREATE TABLE IF NOT EXISTS user_capabilities(
  id TEXT NOT NULL,
  version TEXT NOT NULL,
  system_id TEXT NOT NULL,
  scope TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL,
  parameter_schema TEXT NOT NULL,
  steps TEXT NOT NULL,
  read_systems TEXT NOT NULL,
  write_systems TEXT NOT NULL,
  status TEXT NOT NULL,
  source_task_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(id,version)
);
CREATE TABLE IF NOT EXISTS system_sessions(
  task_id TEXT PRIMARY KEY,
  system_id TEXT NOT NULL,
  session_id TEXT NOT NULL UNIQUE,
  plugin_version TEXT NOT NULL,
  draft_id TEXT,
  status TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS task_scope_leases(
  token_hash TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  read_systems TEXT NOT NULL,
  write_systems TEXT NOT NULL,
  draft_ids TEXT NOT NULL,
  plugin_versions TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  revoked INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS composite_tasks(
  composite_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS domain_memories(
  id TEXT PRIMARY KEY,
  system_id TEXT NOT NULL,
  scope TEXT NOT NULL,
  kind TEXT NOT NULL,
  summary TEXT NOT NULL,
  body TEXT NOT NULL,
  status TEXT NOT NULL,
  source_task_id TEXT NOT NULL,
  plugin_version TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_domain_memories_system ON domain_memories(system_id,status,updated_at);
CREATE TABLE IF NOT EXISTS draft_domains(
  draft_id TEXT PRIMARY KEY,
  system_id TEXT,
  composite_id TEXT,
  plugin_version TEXT,
  legacy INTEGER NOT NULL DEFAULT 0,
  FOREIGN KEY(draft_id) REFERENCES drafts(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS domain_governance_migrations(
  id TEXT PRIMARY KEY,
  system_id TEXT NOT NULL,
  from_version TEXT NOT NULL,
  to_version TEXT NOT NULL,
  status TEXT NOT NULL,
  report TEXT NOT NULL,
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

fn configure_connection(connection: &Connection, read_only: bool) -> Result<(), String> {
    if read_only {
        connection
            .execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA query_only=ON; PRAGMA busy_timeout=5000;",
            )
            .map_err(db_error)
    } else {
        connection
            .execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;",
            )
            .map_err(db_error)
    }
}

fn migrate_database(path: &Path, schema: &str, prefix: &str) -> Result<(), String> {
    let existed = path.is_file();
    let mut connection = Connection::open(path).map_err(db_error)?;
    connection
        .execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
        .map_err(db_error)?;
    let current = match connection
        .query_row(
            "SELECT value FROM metadata WHERE key='schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
    {
        Ok(Some(value)) => value
            .parse::<i64>()
            .map_err(|_| format!("{prefix}_SCHEMA_INVALID: schema_version is not an integer"))?,
        Ok(None) => {
            if existed {
                return Err(format!(
                    "{prefix}_SCHEMA_INVALID: schema_version metadata is missing"
                ));
            }
            0
        }
        Err(rusqlite::Error::SqliteFailure(_, Some(message)))
            if !existed && message.contains("no such table") =>
        {
            0
        }
        Err(error) => {
            return Err(format!("{prefix}_SCHEMA_READ_FAILED: {error}"));
        }
    };
    if current > SCHEMA_VERSION {
        return Err(format!(
            "{prefix}_SCHEMA_NEWER: database schema {current} is newer than supported {SCHEMA_VERSION}"
        ));
    }
    let backup = path.with_extension(format!("sqlite.v{current}.bak"));
    if existed && current < SCHEMA_VERSION {
        connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(db_error)?;
        drop(connection);
        if !backup.exists() {
            fs::copy(path, &backup).map_err(|error| format!("{prefix}_BACKUP_FAILED: {error}"))?;
        }
        connection = Connection::open(path).map_err(db_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
            .map_err(db_error)?;
    }
    let migration = (|| -> Result<(), rusqlite::Error> {
        let transaction = connection.transaction()?;
        transaction.execute_batch(schema)?;
        if prefix == "PROJECT_DATABASE" && current < 2 {
            transaction.execute(
                "INSERT OR IGNORE INTO draft_domains(draft_id,legacy)
                 SELECT id,1 FROM drafts",
                [],
            )?;
        }
        transaction.execute(
            "INSERT INTO metadata(key,value) VALUES('schema_version',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [SCHEMA_VERSION.to_string()],
        )?;
        transaction.commit()
    })();
    if let Err(error) = migration {
        drop(connection);
        if existed && backup.is_file() {
            fs::copy(&backup, path).map_err(|restore| {
                format!("{prefix}_RESTORE_FAILED: {restore}; migration: {error}")
            })?;
        }
        return Err(format!("{prefix}_MIGRATION_FAILED: {error}"));
    }
    Ok(())
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

    #[test]
    fn schema_v2_migration_creates_recoverable_backups() {
        let base = std::env::temp_dir().join(format!("mir3-migration-{}", std::process::id()));
        let project = base.join("项目/木立");
        let data = base.join("data");
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200")).unwrap();
        let store = DomainStore::new(&data).unwrap();
        let imported = store.import_project(&project).unwrap();
        let registry_path = data.join("registry.sqlite");
        let project_path = data.join(&imported.id).join("project.sqlite");
        store
            .registry()
            .unwrap()
            .execute(
                "UPDATE metadata SET value='1' WHERE key='schema_version'",
                [],
            )
            .unwrap();
        let project_connection = store.project_connection(&imported.id).unwrap();
        project_connection
            .execute(
                "INSERT INTO drafts(id,intent,revision,status,created_at,updated_at)
                 VALUES('draft-old','旧地图修改',0,'open',1,1)",
                [],
            )
            .unwrap();
        project_connection
            .execute(
                "UPDATE metadata SET value='1' WHERE key='schema_version'",
                [],
            )
            .unwrap();
        project_connection
            .execute("DROP TABLE domain_memories", [])
            .unwrap();
        drop(project_connection);
        drop(store);

        let reopened = DomainStore::new(&data).unwrap();
        assert!(registry_path.with_extension("sqlite.v1.bak").is_file());
        assert!(project_path.with_extension("sqlite.v1.bak").is_file());
        let table: String = reopened
            .project_connection(&imported.id)
            .unwrap()
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='domain_memories'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table, "domain_memories");
        let legacy: i64 = reopened
            .project_connection(&imported.id)
            .unwrap()
            .query_row(
                "SELECT legacy FROM draft_domains WHERE draft_id='draft-old'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(legacy, 1);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn failed_schema_migration_restores_backup_without_partial_ddl() {
        let base = std::env::temp_dir().join(format!(
            "mir3-migration-rollback-{}-{}",
            std::process::id(),
            now_millis()
        ));
        fs::create_dir_all(&base).unwrap();
        let path = base.join("fixture.sqlite");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
                 INSERT INTO metadata(key,value) VALUES('schema_version','1');
                 CREATE TABLE sentinel(value TEXT NOT NULL);
                 INSERT INTO sentinel(value) VALUES('preserved');",
            )
            .unwrap();
        drop(connection);

        let error = migrate_database(
            &path,
            "CREATE TABLE partial_write(value TEXT); THIS IS NOT VALID SQL;",
            "TEST_DATABASE",
        )
        .unwrap_err();
        assert!(error.starts_with("TEST_DATABASE_MIGRATION_FAILED:"));
        let backup = path.with_extension("sqlite.v1.bak");
        assert!(backup.is_file());
        let restored = Connection::open(&path).unwrap();
        assert_eq!(
            restored
                .query_row("SELECT value FROM sentinel", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "preserved"
        );
        assert_eq!(
            restored
                .query_row(
                    "SELECT value FROM metadata WHERE key='schema_version'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "1"
        );
        let partial_count: i64 = restored
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='partial_write'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(partial_count, 0);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn newer_registry_schema_starts_readonly_without_hiding_projects() {
        let base = std::env::temp_dir().join(format!(
            "mir3-readonly-schema-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let project = base.join("项目/木立");
        let data = base.join("data");
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200")).unwrap();
        fs::write(project.join("引擎/Mir200/Quest.txt"), "questId=Q1\n").unwrap();
        let store = DomainStore::new(&data).unwrap();
        let imported = store.import_project(&project).unwrap();
        store.scan_project(&imported.id, || false).unwrap();
        store
            .registry()
            .unwrap()
            .execute(
                "UPDATE metadata SET value='999' WHERE key='schema_version'",
                [],
            )
            .unwrap();
        drop(store);

        let reopened = DomainStore::new(&data).unwrap();
        assert!(reopened
            .read_only_reason()
            .is_some_and(|reason| reason.starts_with("REGISTRY_DATABASE_SCHEMA_NEWER:")));
        assert_eq!(reopened.list_projects().unwrap().len(), 1);
        let files = reopened
            .query_domain_files(
                &imported.id,
                "quest",
                &crate::DomainFileQuery {
                    text: String::new(),
                    limit: Some(10),
                    offset: None,
                },
            )
            .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].access, "readonly");
        assert!(reopened
            .describe_domain_system(&imported.id, "quest")
            .unwrap()
            .diagnostics
            .iter()
            .any(|value| value.starts_with("DOMAIN_KERNEL_READONLY:")));
        let denied = reopened.import_project(&project).unwrap_err();
        assert!(denied.starts_with("DOMAIN_KERNEL_READONLY:"));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn newer_project_schema_keeps_registry_and_project_reads_available() {
        let base = std::env::temp_dir().join(format!(
            "mir3-readonly-project-schema-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let project = base.join("项目/只读");
        let data = base.join("data");
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200/Envir/Shop")).unwrap();
        fs::write(
            project.join("引擎/Mir200/Envir/Shop/shop.txt"),
            "shopId=1\n",
        )
        .unwrap();
        let store = DomainStore::new(&data).unwrap();
        let imported = store.import_project(&project).unwrap();
        store.scan_project(&imported.id, || false).unwrap();
        store
            .project_connection(&imported.id)
            .unwrap()
            .execute(
                "UPDATE metadata SET value='999' WHERE key='schema_version'",
                [],
            )
            .unwrap();
        drop(store);

        let reopened = DomainStore::new(&data).unwrap();
        assert!(reopened
            .read_only_reason()
            .is_some_and(|reason| reason.starts_with("PROJECT_DATABASE_SCHEMA_NEWER:")));
        assert_eq!(reopened.list_projects().unwrap().len(), 1);
        assert_eq!(reopened.index_stats(&imported.id).unwrap().total_files, 1);
        assert!(reopened
            .open_draft(&imported.id, "denied")
            .unwrap_err()
            .starts_with("DOMAIN_KERNEL_READONLY:"));
        fs::remove_dir_all(base).ok();
    }
}
