use crate::{decode_supported_text, now_millis, path_is_within, DomainStore};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use similar::TextDiff;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static REPLACE_NONCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DraftStatus {
    Open,
    Applied,
    Discarded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub id: String,
    pub intent: String,
    pub revision: i64,
    pub status: DraftStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftChangeInput {
    pub path: String,
    pub content: Option<String>,
    #[serde(default)]
    pub deleted: bool,
    pub expected_sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DraftBinaryChangeInput {
    pub path: String,
    pub content: Vec<u8>,
    pub expected_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftChangePreview {
    pub path: String,
    pub deleted: bool,
    pub base_sha256: Option<String>,
    pub new_sha256: Option<String>,
    pub unified_diff: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftPreview {
    pub draft: Draft,
    pub changes: Vec<DraftChangePreview>,
    pub diff_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub draft_id: Option<String>,
    pub files: Vec<SnapshotFile>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFile {
    pub path: String,
    pub existed: bool,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositeDraftConfirmation {
    pub draft_id: String,
    pub expected_revision: i64,
    pub expected_diff_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositeApplyResult {
    pub composite_id: String,
    pub draft_ids: Vec<String>,
    pub snapshot: Snapshot,
}

impl DomainStore {
    pub fn open_draft(&self, project_id: &str, intent: &str) -> Result<Draft, String> {
        let trimmed = intent.trim();
        if trimmed.is_empty() {
            return Err("DRAFT_INTENT_EMPTY: intent is required".to_string());
        }
        let now = now_millis();
        let id = generated_id("draft", project_id, trimmed, now);
        let draft = Draft {
            id,
            intent: trimmed.to_string(),
            revision: 0,
            status: DraftStatus::Open,
            created_at: now,
            updated_at: now,
        };
        self.project_connection(project_id)?
            .execute(
                "INSERT INTO drafts(id,intent,revision,status,created_at,updated_at) VALUES(?1,?2,0,'open',?3,?3)",
                params![draft.id, draft.intent, now],
            )
            .map_err(|e| format!("DRAFT_CREATE_FAILED: {e}"))?;
        fs::create_dir_all(self.project_dir(project_id)?.join("drafts").join(&draft.id))
            .map_err(|e| format!("DRAFT_DIRECTORY_FAILED: {e}"))?;
        Ok(draft)
    }

    pub fn list_drafts(&self, project_id: &str) -> Result<Vec<Draft>, String> {
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare("SELECT id,intent,revision,status,created_at,updated_at FROM drafts ORDER BY updated_at DESC")
            .map_err(|e| format!("DRAFT_LIST_FAILED: {e}"))?;
        let rows = statement
            .query_map([], row_to_draft)
            .map_err(|e| format!("DRAFT_LIST_FAILED: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("DRAFT_LIST_FAILED: {e}"))
    }

    pub fn get_draft(&self, project_id: &str, draft_id: &str) -> Result<Draft, String> {
        self.project_connection(project_id)?
            .query_row(
                "SELECT id,intent,revision,status,created_at,updated_at FROM drafts WHERE id=?1",
                [draft_id],
                row_to_draft,
            )
            .optional()
            .map_err(|e| format!("DRAFT_GET_FAILED: {e}"))?
            .ok_or_else(|| format!("DRAFT_NOT_FOUND: {draft_id}"))
    }

    /// MCP 可调用的 Draft 写入：只写外置数据库和 Draft 目录，不改正式项目。
    pub fn patch_draft(
        &self,
        project_id: &str,
        draft_id: &str,
        expected_revision: i64,
        changes: &[DraftChangeInput],
    ) -> Result<DraftPreview, String> {
        let draft = self.get_draft(project_id, draft_id)?;
        if draft.status != DraftStatus::Open {
            return Err("DRAFT_NOT_OPEN: only open drafts can be patched".to_string());
        }
        if draft.revision != expected_revision {
            return Err(format!(
                "DRAFT_REVISION_CONFLICT: expected {expected_revision}, current {}",
                draft.revision
            ));
        }
        if changes.is_empty() {
            return Err("DRAFT_CHANGES_EMPTY: at least one change is required".to_string());
        }
        let project = self.get_project(project_id)?;
        let root = PathBuf::from(&project.root);
        let mut connection = self.project_connection(project_id)?;
        let transaction = connection
            .transaction()
            .map_err(|e| format!("DRAFT_TRANSACTION_FAILED: {e}"))?;
        for change in changes {
            validate_relative_path(&change.path)?;
            self.assert_draft_path_writable(project_id, draft_id, &change.path)?;
            let target = safe_project_target(&root, &change.path)?;
            let existing = fs::read(&target).ok();
            let base_hash = existing.as_deref().map(hash_bytes);
            if change.expected_sha256.is_some() && change.expected_sha256 != base_hash {
                return Err(format!(
                    "DRAFT_BASE_CONFLICT: {} changed since it was indexed",
                    change.path
                ));
            }
            if !change.deleted && change.content.is_none() {
                return Err(format!("DRAFT_CONTENT_MISSING: {}", change.path));
            }
            transaction
                .execute(
                    "INSERT INTO draft_changes(draft_id,path,base_sha256,content,deleted) VALUES(?1,?2,?3,?4,?5)
                     ON CONFLICT(draft_id,path) DO UPDATE SET base_sha256=excluded.base_sha256,content=excluded.content,deleted=excluded.deleted",
                    params![
                        draft_id,
                        change.path.replace('\\', "/"),
                        base_hash,
                        change.content.as_deref().map(str::as_bytes),
                        i64::from(change.deleted),
                    ],
                )
                .map_err(|e| format!("DRAFT_PATCH_FAILED: {e}"))?;
        }
        let next_revision = draft.revision + 1;
        transaction
            .execute(
                "UPDATE drafts SET revision=?2,updated_at=?3 WHERE id=?1",
                params![draft_id, next_revision, now_millis()],
            )
            .map_err(|e| format!("DRAFT_UPDATE_FAILED: {e}"))?;
        transaction
            .commit()
            .map_err(|e| format!("DRAFT_COMMIT_FAILED: {e}"))?;
        self.preview_draft(project_id, draft_id)
    }

    /// Studio 安全编辑器使用的原始字节 Draft 写入。它与文本 MCP 共用同一 Draft、
    /// revision 和人工确认链路，但不会把 GB18030/BOM 文本强制转换成 UTF-8。
    pub fn patch_draft_bytes(
        &self,
        project_id: &str,
        draft_id: &str,
        expected_revision: i64,
        changes: &[DraftBinaryChangeInput],
    ) -> Result<DraftPreview, String> {
        let draft = self.get_draft(project_id, draft_id)?;
        if draft.status != DraftStatus::Open {
            return Err("DRAFT_NOT_OPEN: only open drafts can be patched".to_string());
        }
        if draft.revision != expected_revision {
            return Err(format!(
                "DRAFT_REVISION_CONFLICT: expected {expected_revision}, current {}",
                draft.revision
            ));
        }
        if changes.is_empty() {
            return Err("DRAFT_CHANGES_EMPTY: at least one change is required".to_string());
        }
        let project = self.get_project(project_id)?;
        let root = PathBuf::from(&project.root);
        let mut connection = self.project_connection(project_id)?;
        let transaction = connection
            .transaction()
            .map_err(|e| format!("DRAFT_TRANSACTION_FAILED: {e}"))?;
        for change in changes {
            validate_relative_path(&change.path)?;
            self.assert_draft_path_writable(project_id, draft_id, &change.path)?;
            let target = safe_project_target(&root, &change.path)?;
            let existing = fs::read(&target).ok();
            let base_hash = existing.as_deref().map(hash_bytes);
            if change.expected_sha256.is_some() && change.expected_sha256 != base_hash {
                return Err(format!(
                    "DRAFT_BASE_CONFLICT: {} changed since it was opened",
                    change.path
                ));
            }
            transaction
                .execute(
                    "INSERT INTO draft_changes(draft_id,path,base_sha256,content,deleted) VALUES(?1,?2,?3,?4,0)
                     ON CONFLICT(draft_id,path) DO UPDATE SET base_sha256=excluded.base_sha256,content=excluded.content,deleted=0",
                    params![
                        draft_id,
                        change.path.replace('\\', "/"),
                        base_hash,
                        change.content,
                    ],
                )
                .map_err(|e| format!("DRAFT_PATCH_FAILED: {e}"))?;
        }
        let next_revision = draft.revision + 1;
        transaction
            .execute(
                "UPDATE drafts SET revision=?2,updated_at=?3 WHERE id=?1",
                params![draft_id, next_revision, now_millis()],
            )
            .map_err(|e| format!("DRAFT_UPDATE_FAILED: {e}"))?;
        transaction
            .commit()
            .map_err(|e| format!("DRAFT_COMMIT_FAILED: {e}"))?;
        self.preview_draft(project_id, draft_id)
    }

    pub fn draft_change_bytes(
        &self,
        project_id: &str,
        draft_id: &str,
        path: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        validate_relative_path(path)?;
        self.project_connection(project_id)?
            .query_row(
                "SELECT content FROM draft_changes WHERE draft_id=?1 AND path=?2 AND deleted=0",
                params![draft_id, path.replace('\\', "/")],
                |row| row.get::<_, Option<Vec<u8>>>(0),
            )
            .optional()
            .map(|value| value.flatten())
            .map_err(|e| format!("DRAFT_CHANGE_READ_FAILED: {e}"))
    }

    pub fn preview_draft(&self, project_id: &str, draft_id: &str) -> Result<DraftPreview, String> {
        let draft = self.get_draft(project_id, draft_id)?;
        let project = self.get_project(project_id)?;
        let root = PathBuf::from(&project.root);
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare("SELECT path,base_sha256,content,deleted FROM draft_changes WHERE draft_id=?1 ORDER BY path")
            .map_err(|e| format!("DRAFT_PREVIEW_FAILED: {e}"))?;
        let rows = statement
            .query_map([draft_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, i64>(3)? != 0,
                ))
            })
            .map_err(|e| format!("DRAFT_PREVIEW_FAILED: {e}"))?;
        let mut changes = Vec::new();
        let mut digest = Sha256::new();
        digest.update(project_id.as_bytes());
        digest.update(draft_id.as_bytes());
        digest.update(draft.revision.to_le_bytes());
        for row in rows {
            let (path, base_sha256, content, deleted) =
                row.map_err(|e| format!("DRAFT_PREVIEW_FAILED: {e}"))?;
            let target = safe_project_target(&root, &path)?;
            let old = fs::read(&target).unwrap_or_default();
            let new_hash = (!deleted).then(|| hash_bytes(content.as_deref().unwrap_or_default()));
            let unified_diff = text_diff(&path, &old, content.as_deref(), deleted);
            digest.update(path.as_bytes());
            digest.update(base_sha256.as_deref().unwrap_or("").as_bytes());
            digest.update(new_hash.as_deref().unwrap_or("").as_bytes());
            digest.update([u8::from(deleted)]);
            changes.push(DraftChangePreview {
                path,
                deleted,
                base_sha256,
                new_sha256: new_hash,
                unified_diff,
            });
        }
        Ok(DraftPreview {
            draft,
            changes,
            diff_hash: format!("{:x}", digest.finalize()),
        })
    }

    /// 仅供 Tauri 人工确认路径调用；MCP 不暴露此方法。
    pub fn apply_draft(
        &self,
        project_id: &str,
        draft_id: &str,
        expected_revision: i64,
        expected_diff_hash: &str,
    ) -> Result<Snapshot, String> {
        let preview = self.preview_draft(project_id, draft_id)?;
        if preview.draft.status != DraftStatus::Open {
            return Err("DRAFT_NOT_OPEN: draft is no longer open".to_string());
        }
        if preview.draft.revision != expected_revision || preview.diff_hash != expected_diff_hash {
            return Err("DRAFT_CONFIRMATION_STALE: preview changed; review it again".to_string());
        }
        let project = self.get_project(project_id)?;
        let root = PathBuf::from(&project.root);
        for change in &preview.changes {
            let target = safe_project_target(&root, &change.path)?;
            let current = fs::read(&target).ok().as_deref().map(hash_bytes);
            if current != change.base_sha256 {
                return Err(format!(
                    "DRAFT_BASE_CONFLICT: {} changed after preview",
                    change.path
                ));
            }
        }
        let paths: Vec<String> = preview
            .changes
            .iter()
            .map(|change| change.path.clone())
            .collect();
        let snapshot = self.create_snapshot(project_id, Some(draft_id), &paths)?;
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT path,content,deleted FROM draft_changes WHERE draft_id=?1 ORDER BY path",
            )
            .map_err(|e| format!("DRAFT_APPLY_FAILED: {e}"))?;
        let rows = statement
            .query_map([draft_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, i64>(2)? != 0,
                ))
            })
            .map_err(|e| format!("DRAFT_APPLY_FAILED: {e}"))?;
        let operations = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("DRAFT_APPLY_FAILED: {e}"))?;
        for (path, content, deleted) in operations {
            let target = safe_project_target(&root, &path)?;
            let result = if deleted {
                if target.exists() {
                    fs::remove_file(&target)
                } else {
                    Ok(())
                }
            } else {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("DRAFT_APPLY_FAILED: {}: {e}", parent.display()))?;
                }
                replace_file_safely(&target, &content.unwrap_or_default())
            };
            if let Err(error) = result {
                let _ = self.restore_snapshot(project_id, &snapshot.id);
                return Err(format!("DRAFT_APPLY_FAILED: {}: {error}", target.display()));
            }
        }
        self.project_connection(project_id)?
            .execute(
                "UPDATE drafts SET status='applied',updated_at=?2 WHERE id=?1",
                params![draft_id, now_millis()],
            )
            .map_err(|e| format!("DRAFT_UPDATE_FAILED: {e}"))?;
        Ok(snapshot)
    }

    /// 将多个领域 Draft 作为一个组合变更原子应用。
    ///
    /// 全部基线和确认信息会在第一次写入前完成检查；任意写入或数据库提交失败时，
    /// 使用同一组合快照恢复全部文件，避免跨系统任务只提交一部分。
    pub fn apply_composite_drafts(
        &self,
        project_id: &str,
        composite_id: &str,
        confirmations: &[CompositeDraftConfirmation],
    ) -> Result<CompositeApplyResult, String> {
        if composite_id.trim().is_empty() || confirmations.len() < 2 {
            return Err(
                "COMPOSITE_DRAFT_INVALID: composite id and at least two drafts are required"
                    .to_string(),
            );
        }
        let project = self.get_project(project_id)?;
        let root = PathBuf::from(&project.root);
        let mut all_paths = Vec::new();
        let mut operations = Vec::new();
        let mut draft_ids = Vec::new();
        for confirmation in confirmations {
            if draft_ids.contains(&confirmation.draft_id) {
                return Err(format!(
                    "COMPOSITE_DRAFT_DUPLICATE: {}",
                    confirmation.draft_id
                ));
            }
            let preview = self.preview_draft(project_id, &confirmation.draft_id)?;
            if preview.draft.status != DraftStatus::Open
                || preview.draft.revision != confirmation.expected_revision
                || preview.diff_hash != confirmation.expected_diff_hash
            {
                return Err(format!(
                    "COMPOSITE_CONFIRMATION_STALE: {}",
                    confirmation.draft_id
                ));
            }
            for change in &preview.changes {
                if all_paths.contains(&change.path) {
                    return Err(format!("COMPOSITE_PATH_CONFLICT: {}", change.path));
                }
                let target = safe_project_target(&root, &change.path)?;
                let current = fs::read(&target).ok().as_deref().map(hash_bytes);
                if current != change.base_sha256 {
                    return Err(format!(
                        "DRAFT_BASE_CONFLICT: {} changed after preview",
                        change.path
                    ));
                }
                all_paths.push(change.path.clone());
            }
            let connection = self.project_connection(project_id)?;
            let mut statement = connection
                .prepare(
                    "SELECT path,content,deleted FROM draft_changes WHERE draft_id=?1 ORDER BY path",
                )
                .map_err(|error| format!("COMPOSITE_DRAFT_READ_FAILED: {error}"))?;
            let rows = statement
                .query_map([&confirmation.draft_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<Vec<u8>>>(1)?,
                        row.get::<_, i64>(2)? != 0,
                    ))
                })
                .map_err(|error| format!("COMPOSITE_DRAFT_READ_FAILED: {error}"))?;
            operations.extend(
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|error| format!("COMPOSITE_DRAFT_READ_FAILED: {error}"))?,
            );
            draft_ids.push(confirmation.draft_id.clone());
        }
        let snapshot = self.create_snapshot(project_id, None, &all_paths)?;
        for (path, content, deleted) in operations {
            let target = safe_project_target(&root, &path)?;
            let result = if deleted {
                if target.exists() {
                    fs::remove_file(&target)
                } else {
                    Ok(())
                }
            } else {
                if let Some(parent) = target.parent() {
                    if let Err(error) = fs::create_dir_all(parent) {
                        let _ = self.restore_snapshot(project_id, &snapshot.id);
                        return Err(format!(
                            "COMPOSITE_APPLY_FAILED: {}: {error}",
                            parent.display()
                        ));
                    }
                }
                replace_file_safely(&target, &content.unwrap_or_default())
            };
            if let Err(error) = result {
                let _ = self.restore_snapshot(project_id, &snapshot.id);
                return Err(format!(
                    "COMPOSITE_APPLY_FAILED: {}: {error}",
                    target.display()
                ));
            }
        }
        let mut connection = self.project_connection(project_id)?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("COMPOSITE_TRANSACTION_FAILED: {error}"))?;
        for draft_id in &draft_ids {
            if let Err(error) = transaction.execute(
                "UPDATE drafts SET status='applied',updated_at=?2 WHERE id=?1 AND status='open'",
                params![draft_id, now_millis()],
            ) {
                drop(transaction);
                let _ = self.restore_snapshot(project_id, &snapshot.id);
                return Err(format!("COMPOSITE_STATUS_FAILED: {error}"));
            }
        }
        if let Err(error) = transaction.commit() {
            let _ = self.restore_snapshot(project_id, &snapshot.id);
            return Err(format!("COMPOSITE_COMMIT_FAILED: {error}"));
        }
        Ok(CompositeApplyResult {
            composite_id: composite_id.to_string(),
            draft_ids,
            snapshot,
        })
    }

    pub fn discard_draft(&self, project_id: &str, draft_id: &str) -> Result<Draft, String> {
        let draft = self.get_draft(project_id, draft_id)?;
        if draft.status != DraftStatus::Open {
            return Err("DRAFT_NOT_OPEN: only open drafts can be discarded".to_string());
        }
        self.project_connection(project_id)?
            .execute(
                "UPDATE drafts SET status='discarded',updated_at=?2 WHERE id=?1",
                params![draft_id, now_millis()],
            )
            .map_err(|e| format!("DRAFT_DISCARD_FAILED: {e}"))?;
        self.get_draft(project_id, draft_id)
    }

    pub fn create_snapshot(
        &self,
        project_id: &str,
        draft_id: Option<&str>,
        paths: &[String],
    ) -> Result<Snapshot, String> {
        let project = self.get_project(project_id)?;
        let root = PathBuf::from(&project.root);
        let now = now_millis();
        let id = generated_id("snapshot", project_id, draft_id.unwrap_or("manual"), now);
        let directory = self.project_dir(project_id)?.join("snapshots").join(&id);
        fs::create_dir_all(directory.join("files"))
            .map_err(|e| format!("SNAPSHOT_CREATE_FAILED: {e}"))?;
        let mut files = Vec::new();
        for relative in paths {
            validate_relative_path(relative)?;
            let source = safe_project_target(&root, relative)?;
            let existed = source.is_file();
            let bytes = existed
                .then(|| fs::read(&source))
                .transpose()
                .map_err(|e| format!("SNAPSHOT_READ_FAILED: {}: {e}", source.display()))?;
            if let Some(bytes) = &bytes {
                let target = directory.join("files").join(relative);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("SNAPSHOT_CREATE_FAILED: {e}"))?;
                }
                fs::write(target, bytes).map_err(|e| format!("SNAPSHOT_WRITE_FAILED: {e}"))?;
            }
            files.push(SnapshotFile {
                path: relative.clone(),
                existed,
                sha256: bytes.as_deref().map(hash_bytes),
            });
        }
        let snapshot = Snapshot {
            id: id.clone(),
            draft_id: draft_id.map(str::to_string),
            files,
            created_at: now,
        };
        let manifest = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| format!("SNAPSHOT_SERIALIZE_FAILED: {e}"))?;
        fs::write(directory.join("manifest.json"), format!("{manifest}\n"))
            .map_err(|e| format!("SNAPSHOT_WRITE_FAILED: {e}"))?;
        self.project_connection(project_id)?
            .execute(
                "INSERT INTO snapshots(id,draft_id,manifest,created_at) VALUES(?1,?2,?3,?4)",
                params![id, draft_id, manifest, now],
            )
            .map_err(|e| format!("SNAPSHOT_DATABASE_FAILED: {e}"))?;
        Ok(snapshot)
    }

    pub fn list_snapshots(&self, project_id: &str) -> Result<Vec<Snapshot>, String> {
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare("SELECT manifest FROM snapshots ORDER BY created_at DESC")
            .map_err(|e| format!("SNAPSHOT_LIST_FAILED: {e}"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("SNAPSHOT_LIST_FAILED: {e}"))?;
        let mut snapshots = Vec::new();
        for row in rows {
            let manifest = row.map_err(|e| format!("SNAPSHOT_LIST_FAILED: {e}"))?;
            snapshots.push(
                serde_json::from_str(&manifest).map_err(|e| format!("SNAPSHOT_INVALID: {e}"))?,
            );
        }
        Ok(snapshots)
    }

    pub fn restore_snapshot(
        &self,
        project_id: &str,
        snapshot_id: &str,
    ) -> Result<Snapshot, String> {
        let snapshot = self
            .list_snapshots(project_id)?
            .into_iter()
            .find(|snapshot| snapshot.id == snapshot_id)
            .ok_or_else(|| format!("SNAPSHOT_NOT_FOUND: {snapshot_id}"))?;
        let project = self.get_project(project_id)?;
        let root = PathBuf::from(&project.root);
        let directory = self
            .project_dir(project_id)?
            .join("snapshots")
            .join(snapshot_id)
            .join("files");
        for file in &snapshot.files {
            let target = safe_project_target(&root, &file.path)?;
            if file.existed {
                let source = directory.join(&file.path);
                let bytes = fs::read(&source)
                    .map_err(|e| format!("SNAPSHOT_READ_FAILED: {}: {e}", source.display()))?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("SNAPSHOT_RESTORE_FAILED: {e}"))?;
                }
                replace_file_safely(&target, &bytes)
                    .map_err(|e| format!("SNAPSHOT_RESTORE_FAILED: {e}"))?;
            } else if target.exists() {
                fs::remove_file(&target).map_err(|e| format!("SNAPSHOT_RESTORE_FAILED: {e}"))?;
            }
        }
        Ok(snapshot)
    }
}

fn row_to_draft(row: &rusqlite::Row<'_>) -> rusqlite::Result<Draft> {
    let status: String = row.get(3)?;
    Ok(Draft {
        id: row.get(0)?,
        intent: row.get(1)?,
        revision: row.get(2)?,
        status: match status.as_str() {
            "applied" => DraftStatus::Applied,
            "discarded" => DraftStatus::Discarded,
            _ => DraftStatus::Open,
        },
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn generated_id(prefix: &str, project_id: &str, seed: &str, now: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project_id.as_bytes());
    hasher.update(seed.as_bytes());
    hasher.update(now.to_le_bytes());
    let suffix = format!("{:x}", hasher.finalize());
    format!("{prefix}-{now}-{}", &suffix[..10])
}

fn validate_relative_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() {
        return Err("DRAFT_PATH_INVALID: path must be relative".to_string());
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("DRAFT_PATH_INVALID: path traversal is not allowed".to_string());
    }
    Ok(())
}

fn safe_project_target(root: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_relative_path(relative)?;
    let canonical_root =
        fs::canonicalize(root).map_err(|e| format!("PROJECT_PATH_INVALID: {e}"))?;
    let candidate = canonical_root.join(relative);
    let mut existing = candidate.as_path();
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or_else(|| "DRAFT_PATH_INVALID: missing parent".to_string())?;
    }
    let canonical_existing =
        fs::canonicalize(existing).map_err(|e| format!("DRAFT_PATH_INVALID: {e}"))?;
    if !path_is_within(&canonical_root, &canonical_existing) {
        return Err("DRAFT_PATH_OUTSIDE: path escapes project root".to_string());
    }
    Ok(candidate)
}

/// 在目标同目录完成写入与替换，兼容 Windows 不能直接 rename 覆盖已有文件的语义。
///
/// 旧文件会先移动到临时备份；若新文件换入失败，立即恢复旧文件。上层仍会用
/// Snapshot 回滚此前已经完成的其他文件，因此这里仅保证单文件不会处于半写状态。
fn replace_file_safely(target: &Path, content: &[u8]) -> std::io::Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| std::io::Error::other("target has no parent directory"))?;
    let file_name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("mir3-file");
    let nonce = format!(
        "{}-{}-{}",
        std::process::id(),
        now_millis(),
        REPLACE_NONCE.fetch_add(1, Ordering::Relaxed)
    );
    let temporary = parent.join(format!(".{file_name}.mir3-tmp-{nonce}"));
    let backup = parent.join(format!(".{file_name}.mir3-backup-{nonce}"));

    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    if let Err(error) = output.write_all(content).and_then(|_| output.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    drop(output);

    if !target.exists() {
        return fs::rename(&temporary, target).inspect_err(|_| {
            let _ = fs::remove_file(&temporary);
        });
    }

    if let Err(error) = fs::rename(target, &backup) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    match fs::rename(&temporary, target) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(&backup, target);
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn text_diff(path: &str, old: &[u8], new: Option<&[u8]>, deleted: bool) -> Option<String> {
    let old = decode_supported_text(old)?;
    let new = if deleted {
        String::new()
    } else {
        decode_supported_text(new?)?
    };
    Some(
        TextDiff::from_lines(&old, &new)
            .unified_diff()
            .header(&format!("a/{path}"), &format!("b/{path}"))
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_never_changes_project_until_apply_and_snapshot_restores() {
        let base = std::env::temp_dir().join(format!("mir3-draft-{}", std::process::id()));
        let project = base.join("木立");
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎")).unwrap();
        let target = project.join("客户端/dev/Quest/Main.lua");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "return 1\n").unwrap();
        let store = DomainStore::new(base.join("data")).unwrap();
        let imported = store.import_project(&project).unwrap();
        let draft = store.open_draft(&imported.id, "修改入口").unwrap();
        store
            .bind_draft_domain(&imported.id, &draft.id, "quest", "1.0.0", None)
            .unwrap();
        let preview = store
            .patch_draft(
                &imported.id,
                &draft.id,
                0,
                &[DraftChangeInput {
                    path: "客户端/dev/Quest/Main.lua".to_string(),
                    content: Some("return 2\n".to_string()),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "return 1\n");
        let snapshot = store
            .apply_draft(
                &imported.id,
                &draft.id,
                preview.draft.revision,
                &preview.diff_hash,
            )
            .unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "return 2\n");
        store.restore_snapshot(&imported.id, &snapshot.id).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "return 1\n");
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn scoped_drafts_reject_foreign_files_and_composite_apply_is_atomic() {
        let base = std::env::temp_dir().join(format!("mir3-composite-{}", std::process::id()));
        let project = base.join("木立");
        let quest_path = "引擎/Mir200/Envir/QuestDiary/DailyQuest.txt";
        let shop_path = "引擎/Mir200/Envir/Shop/ShopList.txt";
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200/Envir/QuestDiary")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200/Envir/Shop")).unwrap();
        fs::write(project.join(quest_path), "quest=1\n").unwrap();
        fs::write(project.join(shop_path), "shop=1\n").unwrap();
        let store = DomainStore::new(base.join("data")).unwrap();
        let imported = store.import_project(&project).unwrap();
        store.scan_project(&imported.id, || false).unwrap();

        let quest = store.open_draft(&imported.id, "更新任务").unwrap();
        store
            .bind_draft_domain(&imported.id, &quest.id, "quest", "1.0.0", Some("release-1"))
            .unwrap();
        let denied = store.patch_draft(
            &imported.id,
            &quest.id,
            0,
            &[DraftChangeInput {
                path: shop_path.to_string(),
                content: Some("shop=2\n".to_string()),
                deleted: false,
                expected_sha256: None,
            }],
        );
        assert!(denied
            .unwrap_err()
            .starts_with("DRAFT_DOMAIN_SCOPE_DENIED:"));
        let quest_preview = store
            .patch_draft(
                &imported.id,
                &quest.id,
                0,
                &[DraftChangeInput {
                    path: quest_path.to_string(),
                    content: Some("quest=2\n".to_string()),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap();

        let shop = store.open_draft(&imported.id, "更新商城").unwrap();
        store
            .bind_draft_domain(&imported.id, &shop.id, "shop", "1.0.0", Some("release-1"))
            .unwrap();
        let shop_preview = store
            .patch_draft(
                &imported.id,
                &shop.id,
                0,
                &[DraftChangeInput {
                    path: shop_path.to_string(),
                    content: Some("shop=2\n".to_string()),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap();

        let applied = store
            .apply_composite_drafts(
                &imported.id,
                "release-1",
                &[
                    CompositeDraftConfirmation {
                        draft_id: quest.id,
                        expected_revision: quest_preview.draft.revision,
                        expected_diff_hash: quest_preview.diff_hash,
                    },
                    CompositeDraftConfirmation {
                        draft_id: shop.id,
                        expected_revision: shop_preview.draft.revision,
                        expected_diff_hash: shop_preview.diff_hash,
                    },
                ],
            )
            .unwrap();
        assert_eq!(applied.draft_ids.len(), 2);
        assert_eq!(
            fs::read_to_string(project.join(quest_path)).unwrap(),
            "quest=2\n"
        );
        assert_eq!(
            fs::read_to_string(project.join(shop_path)).unwrap(),
            "shop=2\n"
        );
        store
            .restore_snapshot(&imported.id, &applied.snapshot.id)
            .unwrap();
        assert_eq!(
            fs::read_to_string(project.join(quest_path)).unwrap(),
            "quest=1\n"
        );
        assert_eq!(
            fs::read_to_string(project.join(shop_path)).unwrap(),
            "shop=1\n"
        );
        fs::remove_dir_all(base).ok();
    }
}
