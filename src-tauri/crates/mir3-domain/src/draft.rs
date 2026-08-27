use crate::{decode_supported_text, now_millis, path_is_within, DomainStore};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use similar::TextDiff;
#[cfg(unix)]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static REPLACE_NONCE: AtomicU64 = AtomicU64::new(0);
const COMPOSITE_APPLY_JOURNAL_SCHEMA: u32 = 2;
const COMPOSITE_APPLY_JOURNAL_DIRECTORY: &str = "composite-transactions";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DraftStatus {
    Open,
    Applying,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompositeApplyJournal {
    schema_version: u32,
    project_id: String,
    composite_id: String,
    confirmations: Vec<CompositeDraftConfirmation>,
    snapshot: Snapshot,
    #[serde(default)]
    governance_required: bool,
    #[serde(default)]
    applied_files: Vec<AppliedFileState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppliedFileState {
    path: String,
    sha256: Option<String>,
}

impl DomainStore {
    pub fn open_draft(&self, project_id: &str, intent: &str) -> Result<Draft, String> {
        self.ensure_writable()?;
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
        self.ensure_writable()?;
        let _mutation = self.reserve_draft_mutation(project_id, draft_id)?;
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
        let updated = transaction
            .execute(
                "UPDATE drafts SET revision=?2,updated_at=?3 WHERE id=?1 AND revision=?4 AND status='open'",
                params![draft_id, next_revision, now_millis(), expected_revision],
            )
            .map_err(|e| format!("DRAFT_UPDATE_FAILED: {e}"))?;
        if updated != 1 {
            return Err(format!(
                "DRAFT_REVISION_CONFLICT: expected {expected_revision}, Draft changed concurrently"
            ));
        }
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
        self.ensure_writable()?;
        let _mutation = self.reserve_draft_mutation(project_id, draft_id)?;
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
        let updated = transaction
            .execute(
                "UPDATE drafts SET revision=?2,updated_at=?3 WHERE id=?1 AND revision=?4 AND status='open'",
                params![draft_id, next_revision, now_millis(), expected_revision],
            )
            .map_err(|e| format!("DRAFT_UPDATE_FAILED: {e}"))?;
        if updated != 1 {
            return Err(format!(
                "DRAFT_REVISION_CONFLICT: expected {expected_revision}, Draft changed concurrently"
            ));
        }
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
    pub fn apply_validated_domain_draft(
        &self,
        project_id: &str,
        draft_id: &str,
        expected_revision: i64,
        expected_diff_hash: &str,
    ) -> Result<Snapshot, String> {
        self.ensure_single_draft_apply_allowed(project_id, draft_id)?;
        let report = self.validate_domain_draft(project_id, draft_id)?;
        if !report.valid {
            return Err(format!(
                "DRAFT_VALIDATION_FAILED: {}: {}",
                report.system_id,
                report.diagnostics.join(" | ")
            ));
        }
        self.apply_draft(project_id, draft_id, expected_revision, expected_diff_hash)
    }

    /// 治理入口必须把崩溃日志保留到 Receipt 与 Memory 提交完成。
    pub(crate) fn apply_validated_domain_draft_retaining_governance(
        &self,
        project_id: &str,
        draft_id: &str,
        expected_revision: i64,
        expected_diff_hash: &str,
    ) -> Result<Snapshot, String> {
        self.ensure_single_draft_apply_allowed(project_id, draft_id)?;
        let report = self.validate_domain_draft(project_id, draft_id)?;
        if !report.valid {
            return Err(format!(
                "DRAFT_VALIDATION_FAILED: {}: {}",
                report.system_id,
                report.diagnostics.join(" | ")
            ));
        }
        self.apply_draft_internal(
            project_id,
            draft_id,
            expected_revision,
            expected_diff_hash,
            true,
        )
    }

    /// 已绑定组合任务的 Draft 必须走组合确认入口，避免跨系统变更被拆开提交。
    fn ensure_single_draft_apply_allowed(
        &self,
        project_id: &str,
        draft_id: &str,
    ) -> Result<(), String> {
        let composite_id = self
            .project_connection(project_id)?
            .query_row(
                "SELECT composite_id FROM draft_domains WHERE draft_id=?1 AND legacy=0",
                [draft_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| format!("COMPOSITE_BINDING_READ_FAILED: {error}"))?
            .flatten();
        if let Some(composite_id) = composite_id.filter(|value| !value.trim().is_empty()) {
            return Err(format!(
                "COMPOSITE_DRAFT_APPLY_REQUIRED: {draft_id} must be applied atomically with {composite_id}"
            ));
        }
        Ok(())
    }

    /// 低层 Apply 保留给已完成独立校验的内部事务；桌面确认入口必须调用上层校验门禁。
    pub fn apply_draft(
        &self,
        project_id: &str,
        draft_id: &str,
        expected_revision: i64,
        expected_diff_hash: &str,
    ) -> Result<Snapshot, String> {
        self.apply_draft_internal(
            project_id,
            draft_id,
            expected_revision,
            expected_diff_hash,
            false,
        )
    }

    fn apply_draft_internal(
        &self,
        project_id: &str,
        draft_id: &str,
        expected_revision: i64,
        expected_diff_hash: &str,
        governance_required: bool,
    ) -> Result<Snapshot, String> {
        self.ensure_writable()?;
        let _composite_mutation = self.reserve_composite_mutation(project_id)?;
        self.recover_composite_apply_journals_for_project(project_id)?;
        let _mutation = self.reserve_draft_mutation(project_id, draft_id)?;
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
            self.assert_draft_path_writable(project_id, draft_id, &change.path)?;
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
        let transaction_id = format!("single-{draft_id}");
        let confirmation = CompositeDraftConfirmation {
            draft_id: draft_id.to_string(),
            expected_revision,
            expected_diff_hash: expected_diff_hash.to_string(),
        };
        let journal = CompositeApplyJournal {
            schema_version: COMPOSITE_APPLY_JOURNAL_SCHEMA,
            project_id: project_id.to_string(),
            composite_id: transaction_id,
            confirmations: vec![confirmation.clone()],
            snapshot: snapshot.clone(),
            governance_required,
            applied_files: preview
                .changes
                .iter()
                .map(|change| AppliedFileState {
                    path: change.path.clone(),
                    sha256: change.new_sha256.clone(),
                })
                .collect(),
        };
        let journal_path = self.persist_composite_apply_journal(&journal)?;
        if let Err(error) = self.reserve_composite_drafts(project_id, &[confirmation]) {
            return Err(self.recover_composite_after_failure(&journal_path, &journal, error));
        }
        let connection = self.project_connection(project_id).map_err(|error| {
            self.recover_composite_after_failure(&journal_path, &journal, error)
        })?;
        let mut statement = connection
            .prepare(
                "SELECT path,content,deleted FROM draft_changes WHERE draft_id=?1 ORDER BY path",
            )
            .map_err(|error| {
                self.recover_composite_after_failure(
                    &journal_path,
                    &journal,
                    format!("DRAFT_APPLY_FAILED: {error}"),
                )
            })?;
        let rows = statement
            .query_map([draft_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, i64>(2)? != 0,
                ))
            })
            .map_err(|error| {
                self.recover_composite_after_failure(
                    &journal_path,
                    &journal,
                    format!("DRAFT_APPLY_FAILED: {error}"),
                )
            })?;
        let operations = rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
            self.recover_composite_after_failure(
                &journal_path,
                &journal,
                format!("DRAFT_APPLY_FAILED: {error}"),
            )
        })?;
        for (path, content, deleted) in operations {
            let target = safe_project_target(&root, &path).map_err(|error| {
                self.recover_composite_after_failure(&journal_path, &journal, error)
            })?;
            let result = if deleted {
                if target.exists() {
                    fs::remove_file(&target)
                } else {
                    Ok(())
                }
            } else {
                if let Some(parent) = target.parent() {
                    if let Err(error) = fs::create_dir_all(parent) {
                        return Err(self.recover_composite_after_failure(
                            &journal_path,
                            &journal,
                            format!("DRAFT_APPLY_FAILED: {}: {error}", parent.display()),
                        ));
                    }
                }
                replace_file_safely(&target, &content.unwrap_or_default())
            };
            if let Err(error) = result {
                return Err(self.recover_composite_after_failure(
                    &journal_path,
                    &journal,
                    format!("DRAFT_APPLY_FAILED: {}: {error}", target.display()),
                ));
            }
            if let Some(parent) = target.parent() {
                if let Err(error) = sync_directory_ancestors(parent, &root) {
                    return Err(self.recover_composite_after_failure(
                        &journal_path,
                        &journal,
                        format!("DRAFT_APPLY_SYNC_FAILED: {}: {error}", target.display()),
                    ));
                }
            }
        }
        let connection = self.project_connection(project_id).map_err(|error| {
            self.recover_composite_after_failure(&journal_path, &journal, error)
        })?;
        let update = connection.execute(
            "UPDATE drafts SET status='applied',updated_at=?2 WHERE id=?1 AND status='applying' AND revision=?3",
            params![draft_id, now_millis(), expected_revision],
        );
        match update {
            Ok(1) => {}
            Ok(rows) => {
                return Err(self.recover_composite_after_failure(
                    &journal_path,
                    &journal,
                    format!("DRAFT_STATUS_CONFLICT: expected one open draft, updated {rows}"),
                ));
            }
            Err(error) => {
                return Err(self.recover_composite_after_failure(
                    &journal_path,
                    &journal,
                    format!("DRAFT_UPDATE_FAILED: {error}"),
                ));
            }
        }
        #[cfg(test)]
        if self
            .composite_apply_crash_after_commit
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            panic!("DRAFT_APPLY_POST_COMMIT_CRASH_INJECTED");
        }
        if !governance_required {
            self.remove_composite_apply_journal(&journal_path)?;
        }
        Ok(snapshot)
    }

    /// 将多个领域 Draft 作为一个组合变更原子应用。
    ///
    /// 全部基线和确认信息会在第一次写入前完成检查；任意写入或数据库提交失败时，
    /// 使用同一组合快照恢复全部文件，避免跨系统任务只提交一部分。
    pub fn apply_validated_composite_drafts(
        &self,
        project_id: &str,
        composite_id: &str,
        confirmations: &[CompositeDraftConfirmation],
    ) -> Result<CompositeApplyResult, String> {
        for confirmation in confirmations {
            let report = self.validate_domain_draft(project_id, &confirmation.draft_id)?;
            if !report.valid {
                return Err(format!(
                    "DRAFT_VALIDATION_FAILED: {}: {}",
                    report.system_id,
                    report.diagnostics.join(" | ")
                ));
            }
        }
        self.apply_composite_drafts(project_id, composite_id, confirmations)
    }

    pub(crate) fn apply_validated_composite_drafts_retaining_governance(
        &self,
        project_id: &str,
        composite_id: &str,
        confirmations: &[CompositeDraftConfirmation],
    ) -> Result<CompositeApplyResult, String> {
        for confirmation in confirmations {
            let report = self.validate_domain_draft(project_id, &confirmation.draft_id)?;
            if !report.valid {
                return Err(format!(
                    "DRAFT_VALIDATION_FAILED: {}: {}",
                    report.system_id,
                    report.diagnostics.join(" | ")
                ));
            }
        }
        self.apply_composite_drafts_internal(project_id, composite_id, confirmations, true)
    }

    /// 组合 Apply 的低层实现只接受已经通过上层领域校验门禁的调用。
    pub fn apply_composite_drafts(
        &self,
        project_id: &str,
        composite_id: &str,
        confirmations: &[CompositeDraftConfirmation],
    ) -> Result<CompositeApplyResult, String> {
        self.apply_composite_drafts_internal(project_id, composite_id, confirmations, false)
    }

    fn apply_composite_drafts_internal(
        &self,
        project_id: &str,
        composite_id: &str,
        confirmations: &[CompositeDraftConfirmation],
        governance_required: bool,
    ) -> Result<CompositeApplyResult, String> {
        self.ensure_writable()?;
        if composite_id.trim().is_empty() || confirmations.len() < 2 {
            return Err(
                "COMPOSITE_DRAFT_INVALID: composite id and at least two drafts are required"
                    .to_string(),
            );
        }
        let _composite_mutation = self.reserve_composite_mutation(project_id)?;
        self.recover_composite_apply_journals_for_project(project_id)?;
        let mutation_ids = confirmations
            .iter()
            .map(|confirmation| confirmation.draft_id.clone())
            .collect::<Vec<_>>();
        let _mutations = self.reserve_draft_mutations(project_id, &mutation_ids)?;
        let project = self.get_project(project_id)?;
        let root = PathBuf::from(&project.root);
        let mut all_paths = Vec::new();
        let mut applied_files = Vec::new();
        let mut operations = Vec::new();
        let mut draft_ids = Vec::new();
        for confirmation in confirmations {
            if draft_ids.contains(&confirmation.draft_id) {
                return Err(format!(
                    "COMPOSITE_DRAFT_DUPLICATE: {}",
                    confirmation.draft_id
                ));
            }
            let binding = self
                .project_connection(project_id)?
                .query_row(
                    "SELECT composite_id FROM draft_domains WHERE draft_id=?1 AND legacy=0",
                    [&confirmation.draft_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(|error| format!("COMPOSITE_BINDING_READ_FAILED: {error}"))?
                .flatten();
            if binding.as_deref() != Some(composite_id) {
                return Err(format!(
                    "COMPOSITE_BINDING_MISMATCH: {} is not bound to {composite_id}",
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
                self.assert_draft_path_writable(project_id, &confirmation.draft_id, &change.path)?;
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
                applied_files.push(AppliedFileState {
                    path: change.path.clone(),
                    sha256: change.new_sha256.clone(),
                });
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

        let expected_draft_ids = self
            .list_composite_draft_bindings(project_id, composite_id)?
            .into_iter()
            .map(|binding| binding.draft_id)
            .collect::<Vec<_>>();
        let mut submitted_draft_ids = draft_ids.clone();
        submitted_draft_ids.sort();
        let mut expected_draft_ids = expected_draft_ids;
        expected_draft_ids.sort();
        if submitted_draft_ids != expected_draft_ids {
            return Err(
                "COMPOSITE_DRAFT_SET_MISMATCH: confirmations must cover every open Draft in the composite"
                    .to_string(),
            );
        }

        #[cfg(test)]
        {
            let barriers = self
                .composite_apply_test_barrier
                .lock()
                .map_err(|_| "COMPOSITE_TEST_BARRIER_FAILED: barrier lock is poisoned".to_string())?
                .clone();
            if let Some((entered, release)) = barriers {
                entered.wait();
                release.wait();
            }
        }

        let snapshot = self.create_snapshot(project_id, None, &all_paths)?;
        let journal = CompositeApplyJournal {
            schema_version: COMPOSITE_APPLY_JOURNAL_SCHEMA,
            project_id: project_id.to_string(),
            composite_id: composite_id.to_string(),
            confirmations: confirmations.to_vec(),
            snapshot: snapshot.clone(),
            governance_required,
            applied_files,
        };
        let journal_path = self.persist_composite_apply_journal(&journal)?;
        if let Err(error) = self.reserve_composite_drafts(project_id, confirmations) {
            return Err(self.recover_composite_after_failure(&journal_path, &journal, error));
        }
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
                        return Err(self.recover_composite_after_failure(
                            &journal_path,
                            &journal,
                            format!("COMPOSITE_APPLY_FAILED: {}: {error}", parent.display()),
                        ));
                    }
                }
                replace_file_safely(&target, &content.unwrap_or_default())
            };
            if let Err(error) = result {
                return Err(self.recover_composite_after_failure(
                    &journal_path,
                    &journal,
                    format!("COMPOSITE_APPLY_FAILED: {}: {error}", target.display()),
                ));
            }
            if let Some(parent) = target.parent() {
                if let Err(error) = sync_directory_ancestors(parent, &root) {
                    return Err(self.recover_composite_after_failure(
                        &journal_path,
                        &journal,
                        format!("COMPOSITE_APPLY_SYNC_FAILED: {}: {error}", target.display()),
                    ));
                }
            }
            #[cfg(test)]
            {
                use std::sync::atomic::Ordering as TestOrdering;
                if self.composite_apply_crash_after_writes.fetch_update(
                    TestOrdering::SeqCst,
                    TestOrdering::SeqCst,
                    |remaining| (remaining > 0).then_some(remaining - 1),
                ) == Ok(1)
                {
                    panic!("COMPOSITE_APPLY_CRASH_INJECTED");
                }
            }
        }
        let mut connection = self.project_connection(project_id).map_err(|error| {
            self.recover_composite_after_failure(&journal_path, &journal, error)
        })?;
        let transaction = connection.transaction().map_err(|error| {
            self.recover_composite_after_failure(
                &journal_path,
                &journal,
                format!("COMPOSITE_TRANSACTION_FAILED: {error}"),
            )
        })?;
        for confirmation in confirmations {
            let draft_id = &confirmation.draft_id;
            let updated = transaction.execute(
                "UPDATE drafts SET status='applied',updated_at=?2 WHERE id=?1 AND status='applying' AND revision=?3",
                params![draft_id, now_millis(), confirmation.expected_revision],
            );
            match updated {
                Ok(1) => {}
                Ok(rows) => {
                    drop(transaction);
                    return Err(self.recover_composite_after_failure(
                        &journal_path,
                        &journal,
                        format!(
                            "COMPOSITE_STATUS_CONFLICT: {draft_id} expected one reserved draft, updated {rows}"
                        ),
                    ));
                }
                Err(error) => {
                    drop(transaction);
                    return Err(self.recover_composite_after_failure(
                        &journal_path,
                        &journal,
                        format!("COMPOSITE_STATUS_FAILED: {error}"),
                    ));
                }
            }
        }
        if let Err(error) = transaction.commit() {
            return Err(self.recover_composite_after_failure(
                &journal_path,
                &journal,
                format!("COMPOSITE_COMMIT_FAILED: {error}"),
            ));
        }
        #[cfg(test)]
        if self
            .composite_apply_crash_after_commit
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            panic!("COMPOSITE_APPLY_POST_COMMIT_CRASH_INJECTED");
        }
        if !governance_required {
            self.remove_composite_apply_journal(&journal_path)
                .map_err(|error| format!("COMPOSITE_APPLIED_JOURNAL_CLEANUP_FAILED: {error}"))?;
        }
        Ok(CompositeApplyResult {
            composite_id: composite_id.to_string(),
            draft_ids,
            snapshot,
        })
    }

    /// 在项目文件写入前一次性预留所有目标 Draft，避免竞争失败方用旧快照覆盖成功方。
    fn reserve_composite_drafts(
        &self,
        project_id: &str,
        confirmations: &[CompositeDraftConfirmation],
    ) -> Result<(), String> {
        let mut connection = self.project_connection(project_id)?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("COMPOSITE_RESERVATION_FAILED: {error}"))?;
        for confirmation in confirmations {
            let updated = transaction
                .execute(
                    "UPDATE drafts SET status='applying',updated_at=?2 WHERE id=?1 AND status='open' AND revision=?3",
                    params![confirmation.draft_id, now_millis(), confirmation.expected_revision],
                )
                .map_err(|error| format!("COMPOSITE_RESERVATION_FAILED: {error}"))?;
            if updated != 1 {
                return Err(format!(
                    "COMPOSITE_RESERVATION_CONFLICT: {} expected open revision {}",
                    confirmation.draft_id, confirmation.expected_revision
                ));
            }
        }
        transaction
            .commit()
            .map_err(|error| format!("COMPOSITE_RESERVATION_FAILED: {error}"))
    }

    /// 同步错误路径复用启动恢复协议；补偿失败时保留 Journal 供下次启动继续。
    fn recover_composite_after_failure(
        &self,
        journal_path: &Path,
        journal: &CompositeApplyJournal,
        original: String,
    ) -> String {
        match self.recover_composite_apply_journal(journal_path, journal) {
            Ok(()) => original,
            Err(error) => format!("{original}; COMPOSITE_RECOVERY_FAILED: {error}"),
        }
    }

    /// 启动时在项目锁内恢复所有未完成组合 Apply；任一异常都令 Store 只读而非猜测提交状态。
    pub(crate) fn recover_composite_apply_journals(&self) -> Result<(), String> {
        for project in self.list_projects()? {
            if !self.has_composite_apply_journals(&project.id)? {
                continue;
            }
            let _composite_mutation = self.reserve_composite_mutation(&project.id)?;
            self.recover_composite_apply_journals_for_project(&project.id)?;
        }
        Ok(())
    }

    fn has_composite_apply_journals(&self, project_id: &str) -> Result<bool, String> {
        let directory = self
            .project_dir(project_id)?
            .join(COMPOSITE_APPLY_JOURNAL_DIRECTORY);
        if !directory.is_dir() {
            return Ok(false);
        }
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("COMPOSITE_JOURNAL_LIST_FAILED: {error}"))?
        {
            let path = entry
                .map_err(|error| format!("COMPOSITE_JOURNAL_LIST_FAILED: {error}"))?
                .path();
            let file_name = path.file_name().and_then(|value| value.to_str());
            if path.extension().and_then(|value| value.to_str()) == Some("json")
                || file_name.is_some_and(|value| value.starts_with(".pending-"))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn recover_composite_apply_journals_for_project(&self, project_id: &str) -> Result<(), String> {
        let directory = self
            .project_dir(project_id)?
            .join(COMPOSITE_APPLY_JOURNAL_DIRECTORY);
        if !directory.is_dir() {
            return Ok(());
        }
        let mut paths = fs::read_dir(&directory)
            .map_err(|error| format!("COMPOSITE_JOURNAL_LIST_FAILED: {error}"))?
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|error| format!("COMPOSITE_JOURNAL_LIST_FAILED: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();
        for path in paths {
            let file_name = path.file_name().and_then(|value| value.to_str());
            if file_name.is_some_and(|value| value.starts_with(".pending-")) {
                let metadata = fs::symlink_metadata(&path)
                    .map_err(|error| format!("COMPOSITE_JOURNAL_METADATA_FAILED: {error}"))?;
                if !metadata.file_type().is_file() {
                    return Err(format!("COMPOSITE_JOURNAL_INVALID: {}", path.display()));
                }
                fs::remove_file(&path)
                    .map_err(|error| format!("COMPOSITE_JOURNAL_CLEANUP_FAILED: {error}"))?;
                continue;
            }
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("COMPOSITE_JOURNAL_METADATA_FAILED: {error}"))?;
            if !metadata.file_type().is_file() || metadata.len() > 4 * 1024 * 1024 {
                return Err(format!("COMPOSITE_JOURNAL_INVALID: {}", path.display()));
            }
            let journal: CompositeApplyJournal = serde_json::from_slice(
                &fs::read(&path)
                    .map_err(|error| format!("COMPOSITE_JOURNAL_READ_FAILED: {error}"))?,
            )
            .map_err(|error| format!("COMPOSITE_JOURNAL_INVALID: {error}"))?;
            self.validate_composite_apply_journal(project_id, &journal)?;
            self.recover_composite_apply_journal(&path, &journal)?;
        }
        sync_directory(&directory)?;
        Ok(())
    }

    fn validate_composite_apply_journal(
        &self,
        project_id: &str,
        journal: &CompositeApplyJournal,
    ) -> Result<(), String> {
        if !matches!(journal.schema_version, 1 | COMPOSITE_APPLY_JOURNAL_SCHEMA)
            || journal.project_id != project_id
            || journal.composite_id.trim().is_empty()
            || journal.confirmations.is_empty()
            || journal.snapshot.id.trim().is_empty()
            || !is_safe_storage_id(&journal.snapshot.id)
        {
            return Err(format!(
                "COMPOSITE_JOURNAL_INCOMPATIBLE: {}",
                journal.composite_id
            ));
        }
        let mut draft_ids = journal
            .confirmations
            .iter()
            .map(|confirmation| confirmation.draft_id.as_str())
            .collect::<Vec<_>>();
        draft_ids.sort();
        if draft_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("COMPOSITE_JOURNAL_DUPLICATE_DRAFT: duplicate Draft ids".to_string());
        }
        Ok(())
    }

    fn persist_composite_apply_journal(
        &self,
        journal: &CompositeApplyJournal,
    ) -> Result<PathBuf, String> {
        self.validate_composite_apply_journal(&journal.project_id, journal)?;
        let directory = self
            .project_dir(&journal.project_id)?
            .join(COMPOSITE_APPLY_JOURNAL_DIRECTORY);
        fs::create_dir_all(&directory)
            .map_err(|error| format!("COMPOSITE_JOURNAL_DIRECTORY_FAILED: {error}"))?;
        if let Some(parent) = directory.parent() {
            sync_directory(parent)?;
        }
        let path = directory.join(format!(
            "{}.json",
            hash_bytes(journal.composite_id.as_bytes())
        ));
        if path.exists() {
            return Err(format!(
                "COMPOSITE_JOURNAL_ALREADY_PENDING: {}",
                journal.composite_id
            ));
        }
        let nonce = REPLACE_NONCE.fetch_add(1, Ordering::Relaxed);
        let pending = directory.join(format!(".pending-{}-{nonce}", std::process::id()));
        let content = format!(
            "{}\n",
            serde_json::to_string_pretty(journal)
                .map_err(|error| format!("COMPOSITE_JOURNAL_RENDER_FAILED: {error}"))?
        );
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&pending)
            .map_err(|error| format!("COMPOSITE_JOURNAL_WRITE_FAILED: {error}"))?;
        file.write_all(content.as_bytes())
            .map_err(|error| format!("COMPOSITE_JOURNAL_WRITE_FAILED: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("COMPOSITE_JOURNAL_SYNC_FAILED: {error}"))?;
        drop(file);
        fs::rename(&pending, &path)
            .map_err(|error| format!("COMPOSITE_JOURNAL_COMMIT_FAILED: {error}"))?;
        sync_directory_tree(&directory)?;
        if let Some(parent) = directory.parent() {
            sync_directory(parent)?;
        }
        Ok(path)
    }

    fn recover_composite_apply_journal(
        &self,
        path: &Path,
        journal: &CompositeApplyJournal,
    ) -> Result<(), String> {
        self.validate_composite_apply_journal(&journal.project_id, journal)?;
        let connection = self.project_connection(&journal.project_id)?;
        let mut statuses = Vec::with_capacity(journal.confirmations.len());
        for confirmation in &journal.confirmations {
            statuses.push(
                connection
                    .query_row(
                        "SELECT status,revision FROM drafts WHERE id=?1",
                        [&confirmation.draft_id],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                    )
                    .map_err(|error| format!("COMPOSITE_RECOVERY_STATE_FAILED: {error}"))?,
            );
        }
        let revisions_match = statuses
            .iter()
            .zip(&journal.confirmations)
            .all(|((_, revision), confirmation)| *revision == confirmation.expected_revision);
        if !revisions_match {
            return Err(
                "COMPOSITE_RECOVERY_REVISION_MISMATCH: Draft changed after crash".to_string(),
            );
        }
        if statuses.iter().all(|(status, _)| status == "applied") {
            if !journal.governance_required
                || self.governance_receipts_complete(&journal.project_id, journal)?
            {
                return self.remove_composite_apply_journal(path);
            }
            self.ensure_apply_recovery_has_no_external_edits(journal)?;
            self.restore_snapshot_files(&journal.project_id, &journal.snapshot)?;
            return self.rollback_governance_journal(path, journal, "applied");
        }
        if statuses.iter().all(|(status, _)| status == "open") {
            return self.remove_composite_apply_journal(path);
        }
        if !statuses.iter().all(|(status, _)| status == "applying") {
            return Err(
                "COMPOSITE_RECOVERY_STATE_MIXED: Draft states do not share one atomic outcome"
                    .to_string(),
            );
        }

        self.ensure_apply_recovery_has_no_external_edits(journal)?;
        self.restore_snapshot_files(&journal.project_id, &journal.snapshot)?;
        let mut connection = self.project_connection(&journal.project_id)?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("COMPOSITE_RECOVERY_TRANSACTION_FAILED: {error}"))?;
        for confirmation in &journal.confirmations {
            let updated = transaction
                .execute(
                    "UPDATE drafts SET status='open',updated_at=?2 WHERE id=?1 AND status='applying' AND revision=?3",
                    params![confirmation.draft_id, now_millis(), confirmation.expected_revision],
                )
                .map_err(|error| format!("COMPOSITE_RECOVERY_STATUS_FAILED: {error}"))?;
            if updated != 1 {
                return Err(format!(
                    "COMPOSITE_RECOVERY_STATUS_CONFLICT: {}",
                    confirmation.draft_id
                ));
            }
        }
        transaction
            .commit()
            .map_err(|error| format!("COMPOSITE_RECOVERY_COMMIT_FAILED: {error}"))?;
        self.remove_composite_apply_journal(path)
    }

    fn ensure_apply_recovery_has_no_external_edits(
        &self,
        journal: &CompositeApplyJournal,
    ) -> Result<(), String> {
        let project = self.get_project(&journal.project_id)?;
        let root = PathBuf::from(project.root);
        let mut expected = journal
            .applied_files
            .iter()
            .map(|file| (file.path.clone(), file.sha256.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        if expected.is_empty() {
            let connection = self.project_connection(&journal.project_id)?;
            for confirmation in &journal.confirmations {
                let mut statement = connection
                    .prepare("SELECT path,content,deleted FROM draft_changes WHERE draft_id=?1")
                    .map_err(|error| format!("APPLY_RECOVERY_HASH_READ_FAILED: {error}"))?;
                let rows = statement
                    .query_map([&confirmation.draft_id], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<Vec<u8>>>(1)?,
                            row.get::<_, i64>(2)? != 0,
                        ))
                    })
                    .map_err(|error| format!("APPLY_RECOVERY_HASH_READ_FAILED: {error}"))?;
                for row in rows {
                    let (path, content, deleted) =
                        row.map_err(|error| format!("APPLY_RECOVERY_HASH_READ_FAILED: {error}"))?;
                    expected.insert(
                        path,
                        if deleted {
                            None
                        } else {
                            Some(hash_bytes(&content.unwrap_or_default()))
                        },
                    );
                }
            }
        }
        for snapshot_file in &journal.snapshot.files {
            let applied_hash = expected.get(&snapshot_file.path).ok_or_else(|| {
                format!(
                    "APPLY_RECOVERY_HASH_MISSING: {} has no applied target hash",
                    snapshot_file.path
                )
            })?;
            let target = safe_project_target(&root, &snapshot_file.path)?;
            let current_hash = match fs::read(&target) {
                Ok(bytes) => Some(hash_bytes(&bytes)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => {
                    return Err(format!(
                        "APPLY_RECOVERY_CURRENT_READ_FAILED: {}: {error}",
                        target.display()
                    ));
                }
            };
            let snapshot_hash = snapshot_file.sha256.clone();
            if &current_hash != applied_hash && current_hash != snapshot_hash {
                return Err(format!(
                    "APPLY_RECOVERY_EXTERNAL_EDIT_CONFLICT: {} changed outside the pending Apply",
                    snapshot_file.path
                ));
            }
        }
        Ok(())
    }

    fn governance_receipts_complete(
        &self,
        project_id: &str,
        journal: &CompositeApplyJournal,
    ) -> Result<bool, String> {
        let connection = self.project_connection(project_id)?;
        for confirmation in &journal.confirmations {
            let mut statement = connection
                .prepare(
                    "SELECT evidence FROM task_receipts WHERE draft_id=?1 AND status='applied'",
                )
                .map_err(|error| format!("GOVERNANCE_RECOVERY_RECEIPT_READ_FAILED: {error}"))?;
            let rows = statement
                .query_map([&confirmation.draft_id], |row| row.get::<_, String>(0))
                .map_err(|error| format!("GOVERNANCE_RECOVERY_RECEIPT_READ_FAILED: {error}"))?;
            let mut matched = false;
            for row in rows {
                let evidence = row
                    .map_err(|error| format!("GOVERNANCE_RECOVERY_RECEIPT_READ_FAILED: {error}"))?;
                let evidence: serde_json::Value = serde_json::from_str(&evidence)
                    .map_err(|error| format!("GOVERNANCE_RECOVERY_RECEIPT_INVALID: {error}"))?;
                if evidence
                    .get("snapshotId")
                    .and_then(serde_json::Value::as_str)
                    == Some(journal.snapshot.id.as_str())
                    && evidence
                        .pointer("/provenance/issuer")
                        .and_then(serde_json::Value::as_str)
                        == Some("mir3-kernel")
                {
                    matched = true;
                    break;
                }
            }
            if !matched {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn rollback_governance_journal(
        &self,
        path: &Path,
        journal: &CompositeApplyJournal,
        current_status: &str,
    ) -> Result<(), String> {
        let mut connection = self.project_connection(&journal.project_id)?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("GOVERNANCE_RECOVERY_TRANSACTION_FAILED: {error}"))?;
        for confirmation in &journal.confirmations {
            let mut receipt_ids = transaction
                .prepare("SELECT id,evidence FROM task_receipts WHERE draft_id=?1")
                .map_err(|error| format!("GOVERNANCE_RECOVERY_RECEIPT_READ_FAILED: {error}"))?
                .query_map([&confirmation.draft_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| format!("GOVERNANCE_RECOVERY_RECEIPT_READ_FAILED: {error}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("GOVERNANCE_RECOVERY_RECEIPT_READ_FAILED: {error}"))?;
            receipt_ids.retain(|(_, evidence)| {
                serde_json::from_str::<serde_json::Value>(evidence)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("snapshotId")
                            .and_then(serde_json::Value::as_str)
                            .map(|value| value == journal.snapshot.id)
                    })
                    .unwrap_or(false)
            });
            for (receipt_id, _) in receipt_ids {
                let memory_id = format!("memory-{}", &hash_bytes(receipt_id.as_bytes())[..20]);
                transaction
                    .execute("DELETE FROM domain_memories WHERE id=?1", [&memory_id])
                    .map_err(|error| format!("GOVERNANCE_RECOVERY_MEMORY_FAILED: {error}"))?;
                transaction
                    .execute("DELETE FROM task_receipts WHERE id=?1", [&receipt_id])
                    .map_err(|error| format!("GOVERNANCE_RECOVERY_RECEIPT_FAILED: {error}"))?;
            }
            let updated = transaction
                .execute(
                    "UPDATE drafts SET status='open',updated_at=?2 WHERE id=?1 AND status=?3 AND revision=?4",
                    params![
                        confirmation.draft_id,
                        now_millis(),
                        current_status,
                        confirmation.expected_revision
                    ],
                )
                .map_err(|error| format!("GOVERNANCE_RECOVERY_DRAFT_FAILED: {error}"))?;
            if updated != 1 {
                return Err(format!(
                    "GOVERNANCE_RECOVERY_DRAFT_CONFLICT: {}",
                    confirmation.draft_id
                ));
            }
        }
        transaction
            .commit()
            .map_err(|error| format!("GOVERNANCE_RECOVERY_COMMIT_FAILED: {error}"))?;
        self.remove_composite_apply_journal(path)
    }

    pub(crate) fn complete_governance_apply_journal(
        &self,
        project_id: &str,
        transaction_id: &str,
    ) -> Result<(), String> {
        let path = self
            .project_dir(project_id)?
            .join(COMPOSITE_APPLY_JOURNAL_DIRECTORY)
            .join(format!("{}.json", hash_bytes(transaction_id.as_bytes())));
        if !path.exists() {
            return Err(format!("GOVERNANCE_JOURNAL_MISSING: {transaction_id}"));
        }
        let journal: CompositeApplyJournal = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("GOVERNANCE_JOURNAL_READ_FAILED: {error}"))?,
        )
        .map_err(|error| format!("GOVERNANCE_JOURNAL_INVALID: {error}"))?;
        if !journal.governance_required
            || !self.governance_receipts_complete(project_id, &journal)?
        {
            return Err(
                "GOVERNANCE_JOURNAL_INCOMPLETE: applied receipts are incomplete".to_string(),
            );
        }
        self.remove_composite_apply_journal(&path)
    }

    fn remove_composite_apply_journal(&self, path: &Path) -> Result<(), String> {
        if path.exists() {
            fs::remove_file(path)
                .map_err(|error| format!("COMPOSITE_JOURNAL_CLEANUP_FAILED: {error}"))?;
            if let Some(directory) = path.parent() {
                sync_directory(directory)?;
            }
        }
        Ok(())
    }

    pub fn discard_draft(&self, project_id: &str, draft_id: &str) -> Result<Draft, String> {
        let _mutation = self.reserve_draft_mutation(project_id, draft_id)?;
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
                write_file_synced(&target, bytes, "SNAPSHOT_WRITE_FAILED")?;
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
        write_file_synced(
            &directory.join("manifest.json"),
            format!("{manifest}\n").as_bytes(),
            "SNAPSHOT_WRITE_FAILED",
        )?;
        sync_directory_tree(&directory)?;
        if let Some(parent) = directory.parent() {
            sync_directory(parent)?;
        }
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
        self.ensure_writable()?;
        let snapshot = self
            .list_snapshots(project_id)?
            .into_iter()
            .find(|snapshot| snapshot.id == snapshot_id)
            .ok_or_else(|| format!("SNAPSHOT_NOT_FOUND: {snapshot_id}"))?;
        self.restore_snapshot_files(project_id, &snapshot)?;
        Ok(snapshot)
    }

    /// Journal 携带完整快照清单，因此即使列表查询不可用也能幂等恢复文件。
    fn restore_snapshot_files(&self, project_id: &str, snapshot: &Snapshot) -> Result<(), String> {
        let project = self.get_project(project_id)?;
        let root = PathBuf::from(&project.root);
        let directory = self
            .project_dir(project_id)?
            .join("snapshots")
            .join(&snapshot.id)
            .join("files");
        for file in &snapshot.files {
            let target = safe_project_target(&root, &file.path)?;
            if file.existed {
                let source = directory.join(&file.path);
                let bytes = fs::read(&source)
                    .map_err(|e| format!("SNAPSHOT_READ_FAILED: {}: {e}", source.display()))?;
                let actual_hash = hash_bytes(&bytes);
                if file.sha256.as_deref() != Some(actual_hash.as_str()) {
                    return Err(format!("SNAPSHOT_HASH_MISMATCH: {}", source.display()));
                }
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("SNAPSHOT_RESTORE_FAILED: {e}"))?;
                }
                replace_file_safely(&target, &bytes)
                    .map_err(|e| format!("SNAPSHOT_RESTORE_FAILED: {e}"))?;
            } else if target.exists() {
                fs::remove_file(&target).map_err(|e| format!("SNAPSHOT_RESTORE_FAILED: {e}"))?;
                if let Some(parent) = target.parent() {
                    sync_directory(parent)?;
                }
            }
        }
        Ok(())
    }
}

fn write_file_synced(path: &Path, bytes: &[u8], prefix: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("{prefix}: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("{prefix}: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("{prefix}: {error}"))?;
    drop(file);
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

fn is_safe_storage_id(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn sync_directory_tree(root: &Path) -> Result<(), String> {
    for entry in
        fs::read_dir(root).map_err(|error| format!("COMPOSITE_DIRECTORY_SYNC_FAILED: {error}"))?
    {
        let entry = entry.map_err(|error| format!("COMPOSITE_DIRECTORY_SYNC_FAILED: {error}"))?;
        if entry
            .file_type()
            .map_err(|error| format!("COMPOSITE_DIRECTORY_SYNC_FAILED: {error}"))?
            .is_dir()
        {
            sync_directory_tree(&entry.path())?;
        }
    }
    sync_directory(root)
}

fn sync_directory_ancestors(directory: &Path, root: &Path) -> Result<(), String> {
    let mut current = directory;
    loop {
        sync_directory(current)?;
        if current == root {
            return Ok(());
        }
        current = current.parent().ok_or_else(|| {
            "COMPOSITE_DIRECTORY_SYNC_FAILED: target parent escaped project root".to_string()
        })?;
        if !current.starts_with(root) && current != root {
            return Err(
                "COMPOSITE_DIRECTORY_SYNC_FAILED: target parent escaped project root".to_string(),
            );
        }
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("COMPOSITE_DIRECTORY_SYNC_FAILED: {error}"))
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn row_to_draft(row: &rusqlite::Row<'_>) -> rusqlite::Result<Draft> {
    let status: String = row.get(3)?;
    Ok(Draft {
        id: row.get(0)?,
        intent: row.get(1)?,
        revision: row.get(2)?,
        status: match status.as_str() {
            "applying" => DraftStatus::Applying,
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
        fs::rename(&temporary, target).inspect_err(|_| {
            let _ = fs::remove_file(&temporary);
        })?;
        return sync_directory_io(parent);
    }

    if let Err(error) = fs::rename(target, &backup) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    match fs::rename(&temporary, target) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            sync_directory_io(parent)
        }
        Err(error) => {
            let _ = fs::rename(&backup, target);
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

#[cfg(unix)]
fn sync_directory_io(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
fn sync_directory_io(_path: &Path) -> std::io::Result<()> {
    Ok(())
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
        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let imported = store.import_project(&project).unwrap();
        let draft = store.open_draft(&imported.id, "修改入口").unwrap();
        store
            .bind_draft_domain(&imported.id, &draft.id, "quest", "1.3.1", None)
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
        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let imported = store.import_project(&project).unwrap();
        store.scan_project(&imported.id, || false).unwrap();

        let quest = store.open_draft(&imported.id, "更新任务").unwrap();
        store
            .bind_draft_domain(&imported.id, &quest.id, "quest", "1.3.1", Some("release-1"))
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
            .bind_draft_domain(&imported.id, &shop.id, "shop", "1.3.1", Some("release-1"))
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

        let wrong_binding = store
            .apply_composite_drafts(
                &imported.id,
                "release-wrong",
                &[
                    CompositeDraftConfirmation {
                        draft_id: quest.id.clone(),
                        expected_revision: quest_preview.draft.revision,
                        expected_diff_hash: quest_preview.diff_hash.clone(),
                    },
                    CompositeDraftConfirmation {
                        draft_id: shop.id.clone(),
                        expected_revision: shop_preview.draft.revision,
                        expected_diff_hash: shop_preview.diff_hash.clone(),
                    },
                ],
            )
            .unwrap_err();
        assert!(wrong_binding.starts_with("COMPOSITE_BINDING_MISMATCH:"));
        assert_eq!(
            fs::read_to_string(project.join(quest_path)).unwrap(),
            "quest=1\n"
        );
        assert_eq!(
            fs::read_to_string(project.join(shop_path)).unwrap(),
            "shop=1\n"
        );

        // 即使 SQLite 静默忽略状态更新，也必须把已经写入的两个文件整体恢复。
        store
            .project_connection(&imported.id)
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER ignore_composite_apply
                 BEFORE UPDATE OF status ON drafts
                 WHEN NEW.status='applied'
                 BEGIN SELECT RAISE(IGNORE); END;",
            )
            .unwrap();
        let status_conflict = store
            .apply_composite_drafts(
                &imported.id,
                "release-1",
                &[
                    CompositeDraftConfirmation {
                        draft_id: quest.id.clone(),
                        expected_revision: quest_preview.draft.revision,
                        expected_diff_hash: quest_preview.diff_hash.clone(),
                    },
                    CompositeDraftConfirmation {
                        draft_id: shop.id.clone(),
                        expected_revision: shop_preview.draft.revision,
                        expected_diff_hash: shop_preview.diff_hash.clone(),
                    },
                ],
            )
            .unwrap_err();
        assert!(status_conflict.starts_with("COMPOSITE_STATUS_CONFLICT:"));
        assert_eq!(
            fs::read_to_string(project.join(quest_path)).unwrap(),
            "quest=1\n"
        );
        assert_eq!(
            fs::read_to_string(project.join(shop_path)).unwrap(),
            "shop=1\n"
        );
        store
            .project_connection(&imported.id)
            .unwrap()
            .execute_batch("DROP TRIGGER ignore_composite_apply;")
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

    #[test]
    fn composite_drafts_reject_single_apply_and_partial_confirmation_sets() {
        let base = std::env::temp_dir().join(format!(
            "mir3-composite-gate-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let project = base.join("组合门禁项目");
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200/Envir")).unwrap();
        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let composite_id = "review-all-three";
        let cases = [
            ("quest", "quest.txt"),
            ("shop", "shop.txt"),
            ("item", "cfg_item.txt"),
        ];
        for (_, file_name) in cases {
            fs::write(
                project.join("引擎/Mir200/Envir").join(file_name),
                "value=1\n",
            )
            .unwrap();
        }
        let imported = store.import_project(&project).unwrap();
        store.scan_project(&imported.id, || false).unwrap();
        let mut confirmations = Vec::new();
        for (system_id, file_name) in cases {
            let draft = store.open_draft(&imported.id, system_id).unwrap();
            store
                .bind_draft_domain(
                    &imported.id,
                    &draft.id,
                    system_id,
                    "1.3.1",
                    Some(composite_id),
                )
                .unwrap();
            let path = format!("引擎/Mir200/Envir/{file_name}");
            let preview = store
                .patch_draft(
                    &imported.id,
                    &draft.id,
                    0,
                    &[DraftChangeInput {
                        path,
                        content: Some("value=2\n".to_string()),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )
                .unwrap();
            confirmations.push(CompositeDraftConfirmation {
                draft_id: draft.id,
                expected_revision: preview.draft.revision,
                expected_diff_hash: preview.diff_hash,
            });
        }

        let single = store
            .apply_validated_domain_draft(
                &imported.id,
                &confirmations[0].draft_id,
                confirmations[0].expected_revision,
                &confirmations[0].expected_diff_hash,
            )
            .unwrap_err();
        assert!(single.starts_with("COMPOSITE_DRAFT_APPLY_REQUIRED:"));

        let partial = store
            .apply_composite_drafts(&imported.id, composite_id, &confirmations[..2])
            .unwrap_err();
        assert!(partial.starts_with("COMPOSITE_DRAFT_SET_MISMATCH:"));
        for confirmation in &confirmations {
            assert_eq!(
                store
                    .get_draft(&imported.id, &confirmation.draft_id)
                    .unwrap()
                    .status,
                DraftStatus::Open
            );
        }
        for (_, file_name) in cases {
            assert_eq!(
                fs::read_to_string(project.join("引擎/Mir200/Envir").join(file_name)).unwrap(),
                "value=1\n"
            );
        }
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn composite_preflight_scales_across_three_eight_and_all_domains() {
        let base = std::env::temp_dir().join(format!(
            "mir3-composite-matrix-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let project = base.join("全领域项目");
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200/Envir")).unwrap();
        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let manifests = store.list_domain_systems().unwrap();
        assert_eq!(manifests.len(), 33);
        let mut relative_by_system = std::collections::BTreeMap::new();
        for manifest in &manifests {
            let selector = manifest
                .file_projection
                .keywords
                .iter()
                .find(|selector| {
                    !selector.is_empty()
                        && selector
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                })
                .unwrap();
            let relative = format!("引擎/Mir200/Envir/{selector}/{}.txt", manifest.system_id);
            let path = project.join(&relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("{}=0\n", manifest.system_id)).unwrap();
            relative_by_system.insert(manifest.system_id.clone(), relative);
        }
        let imported = store.import_project(&project).unwrap();
        store.scan_project(&imported.id, || false).unwrap();

        for count in [3_usize, 8, 33] {
            let composite_id = format!("g4-{count}");
            let mut confirmations = Vec::new();
            let mut paths = Vec::new();
            for manifest in manifests.iter().take(count) {
                let relative = relative_by_system[&manifest.system_id].clone();
                let draft = store
                    .open_draft(&imported.id, &format!("G4 {}", manifest.system_id))
                    .unwrap();
                store
                    .bind_draft_domain(
                        &imported.id,
                        &draft.id,
                        &manifest.system_id,
                        &manifest.version,
                        Some(&composite_id),
                    )
                    .unwrap();
                let preview = store
                    .patch_draft(
                        &imported.id,
                        &draft.id,
                        0,
                        &[DraftChangeInput {
                            path: relative.clone(),
                            content: Some(format!("{}={count}\n", manifest.system_id)),
                            deleted: false,
                            expected_sha256: None,
                        }],
                    )
                    .unwrap();
                confirmations.push(CompositeDraftConfirmation {
                    draft_id: draft.id,
                    expected_revision: preview.draft.revision,
                    expected_diff_hash: preview.diff_hash,
                });
                paths.push((relative, manifest.system_id.clone()));
            }

            let mut stale = confirmations.clone();
            stale[count / 2].expected_revision += 1;
            let error = store
                .apply_composite_drafts(&imported.id, &composite_id, &stale)
                .unwrap_err();
            assert!(error.starts_with("COMPOSITE_CONFIRMATION_STALE:"));
            for (relative, system_id) in &paths {
                assert_eq!(
                    fs::read_to_string(project.join(relative)).unwrap(),
                    format!("{system_id}=0\n")
                );
            }

            let applied = store
                .apply_composite_drafts(&imported.id, &composite_id, &confirmations)
                .unwrap();
            assert_eq!(applied.draft_ids.len(), count);
            for (relative, system_id) in &paths {
                assert_eq!(
                    fs::read_to_string(project.join(relative)).unwrap(),
                    format!("{system_id}={count}\n")
                );
            }
            store
                .restore_snapshot(&imported.id, &applied.snapshot.id)
                .unwrap();
            for (relative, system_id) in &paths {
                assert_eq!(
                    fs::read_to_string(project.join(relative)).unwrap(),
                    format!("{system_id}=0\n")
                );
            }
        }
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn concurrent_writers_cannot_both_commit_the_same_expected_revision() {
        let base = std::env::temp_dir().join(format!(
            "mir3-draft-concurrency-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let project = base.join("并发项目");
        let relative = "引擎/Mir200/Envir/Quest/quest.txt";
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200/Envir/Quest")).unwrap();
        fs::write(project.join(relative), "quest=0\n").unwrap();
        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let imported = store.import_project(&project).unwrap();
        store.scan_project(&imported.id, || false).unwrap();
        let draft = store.open_draft(&imported.id, "并发修改").unwrap();
        store
            .bind_draft_domain(&imported.id, &draft.id, "quest", "1.3.1", None)
            .unwrap();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = Vec::new();
        for value in ["quest=1\n", "quest=2\n"] {
            let worker_store = store.clone();
            let worker_project = imported.id.clone();
            let worker_draft = draft.id.clone();
            let worker_barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                worker_barrier.wait();
                worker_store.patch_draft(
                    &worker_project,
                    &worker_draft,
                    0,
                    &[DraftChangeInput {
                        path: relative.to_string(),
                        content: Some(value.to_string()),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )
            }));
        }
        barrier.wait();
        let outcomes = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
        let rejected = outcomes
            .iter()
            .find_map(|outcome| outcome.as_ref().err())
            .unwrap();
        assert!(
            rejected.starts_with("DRAFT_REVISION_CONFLICT:")
                || rejected.starts_with("DRAFT_MUTATION_RESERVED:"),
            "unexpected concurrent rejection: {rejected}"
        );
        assert_eq!(
            store.get_draft(&imported.id, &draft.id).unwrap().revision,
            1
        );
        assert_eq!(
            fs::read_to_string(project.join(relative)).unwrap(),
            "quest=0\n"
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn composite_apply_recovers_files_and_draft_states_after_injected_process_crash() {
        let base = std::env::temp_dir().join(format!(
            "mir3-composite-crash-recovery-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let project = base.join("组合崩溃恢复项目");
        let quest_path = "引擎/Mir200/Envir/QuestDiary/quest.txt";
        let shop_path = "引擎/Mir200/Envir/Shop/shop.txt";
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200/Envir/QuestDiary")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200/Envir/Shop")).unwrap();
        fs::write(project.join(quest_path), "quest=0\n").unwrap();
        fs::write(project.join(shop_path), "shop=0\n").unwrap();
        let data_root = base.join("data");
        let store = DomainStore::new_trusted_fixture(&data_root).unwrap();
        let imported = store.import_project(&project).unwrap();
        store.scan_project(&imported.id, || false).unwrap();
        let composite_id = "crash-safe-release";
        let mut confirmations = Vec::new();
        for (system_id, path, content) in [
            ("quest", quest_path, "quest=1\n"),
            ("shop", shop_path, "shop=1\n"),
        ] {
            let draft = store.open_draft(&imported.id, system_id).unwrap();
            store
                .bind_draft_domain(
                    &imported.id,
                    &draft.id,
                    system_id,
                    "1.3.1",
                    Some(composite_id),
                )
                .unwrap();
            let preview = store
                .patch_draft(
                    &imported.id,
                    &draft.id,
                    0,
                    &[DraftChangeInput {
                        path: path.to_string(),
                        content: Some(content.to_string()),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )
                .unwrap();
            confirmations.push(CompositeDraftConfirmation {
                draft_id: draft.id,
                expected_revision: preview.draft.revision,
                expected_diff_hash: preview.diff_hash,
            });
        }

        store
            .composite_apply_crash_after_writes
            .store(1, std::sync::atomic::Ordering::SeqCst);
        let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store
                .apply_composite_drafts(&imported.id, composite_id, &confirmations)
                .unwrap();
        }));
        assert!(crashed.is_err());
        assert!(confirmations.iter().all(|confirmation| {
            store
                .get_draft(&imported.id, &confirmation.draft_id)
                .unwrap()
                .status
                == DraftStatus::Applying
        }));
        assert!(
            fs::read_to_string(project.join(quest_path)).unwrap() == "quest=1\n"
                || fs::read_to_string(project.join(shop_path)).unwrap() == "shop=1\n"
        );
        drop(store);

        let recovered = DomainStore::new_trusted_fixture(&data_root).unwrap();
        assert!(recovered.read_only_reason().is_none());
        assert_eq!(
            fs::read_to_string(project.join(quest_path)).unwrap(),
            "quest=0\n"
        );
        assert_eq!(
            fs::read_to_string(project.join(shop_path)).unwrap(),
            "shop=0\n"
        );
        for confirmation in &confirmations {
            assert_eq!(
                recovered
                    .get_draft(&imported.id, &confirmation.draft_id)
                    .unwrap()
                    .status,
                DraftStatus::Open
            );
        }
        let journals = recovered
            .project_dir(&imported.id)
            .unwrap()
            .join(COMPOSITE_APPLY_JOURNAL_DIRECTORY);
        assert_eq!(
            fs::read_dir(journals)
                .unwrap()
                .filter_map(Result::ok)
                .filter(
                    |entry| entry.path().extension().and_then(|value| value.to_str())
                        == Some("json")
                )
                .count(),
            0
        );

        recovered
            .composite_apply_crash_after_commit
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let committed_crash = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            recovered
                .apply_composite_drafts(&imported.id, composite_id, &confirmations)
                .unwrap();
        }));
        assert!(committed_crash.is_err());
        assert!(confirmations.iter().all(|confirmation| {
            recovered
                .get_draft(&imported.id, &confirmation.draft_id)
                .unwrap()
                .status
                == DraftStatus::Applied
        }));
        drop(recovered);

        let committed = DomainStore::new_trusted_fixture(&data_root).unwrap();
        assert!(committed.read_only_reason().is_none());
        assert_eq!(
            fs::read_to_string(project.join(quest_path)).unwrap(),
            "quest=1\n"
        );
        assert_eq!(
            fs::read_to_string(project.join(shop_path)).unwrap(),
            "shop=1\n"
        );
        for confirmation in &confirmations {
            assert_eq!(
                committed
                    .get_draft(&imported.id, &confirmation.draft_id)
                    .unwrap()
                    .status,
                DraftStatus::Applied
            );
        }
        assert!(!committed
            .has_composite_apply_journals(&imported.id)
            .unwrap());
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn composite_apply_lock_rejects_binding_added_after_complete_set_check() {
        let base = std::env::temp_dir().join(format!(
            "mir3-composite-membership-race-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let project = base.join("组合成员竞态项目");
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200/Envir")).unwrap();
        let data_root = base.join("data");
        let store = DomainStore::new_trusted_fixture(&data_root).unwrap();
        let imported = store.import_project(&project).unwrap();
        let composite_id = "locked-membership";
        let mut confirmations = Vec::new();
        for (system_id, file_name) in [("quest", "quest.txt"), ("shop", "shop.txt")] {
            let path = format!("引擎/Mir200/Envir/{file_name}");
            fs::write(project.join(&path), "value=0\n").unwrap();
            let draft = store.open_draft(&imported.id, system_id).unwrap();
            store
                .bind_draft_domain(
                    &imported.id,
                    &draft.id,
                    system_id,
                    "1.3.1",
                    Some(composite_id),
                )
                .unwrap();
            let preview = store
                .patch_draft(
                    &imported.id,
                    &draft.id,
                    0,
                    &[DraftChangeInput {
                        path,
                        content: Some("value=1\n".to_string()),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )
                .unwrap();
            confirmations.push(CompositeDraftConfirmation {
                draft_id: draft.id,
                expected_revision: preview.draft.revision,
                expected_diff_hash: preview.diff_hash,
            });
        }
        let late = store.open_draft(&imported.id, "late member").unwrap();
        store
            .bind_draft_domain(&imported.id, &late.id, "item", "1.3.1", None)
            .unwrap();
        let second_store = DomainStore::new_trusted_fixture(&data_root).unwrap();
        let entered = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release = std::sync::Arc::new(std::sync::Barrier::new(2));
        *store.composite_apply_test_barrier.lock().unwrap() =
            Some((entered.clone(), release.clone()));
        let worker_store = store.clone();
        let worker_project = imported.id.clone();
        let worker_confirmations = confirmations.clone();
        let worker = std::thread::spawn(move || {
            worker_store.apply_composite_drafts(
                &worker_project,
                composite_id,
                &worker_confirmations,
            )
        });
        entered.wait();
        let rejected = second_store
            .associate_draft_composite(&imported.id, &late.id, "item", "1.3.1", composite_id)
            .unwrap_err();
        assert!(rejected.starts_with("DRAFT_MUTATION_RESERVED:"));
        release.wait();
        worker.join().unwrap().unwrap();
        let late_binding = store
            .project_connection(&imported.id)
            .unwrap()
            .query_row(
                "SELECT composite_id FROM draft_domains WHERE draft_id=?1",
                [&late.id],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap();
        assert!(late_binding.is_none());
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn concurrent_composite_apply_has_one_winner_without_rolling_back_its_files() {
        let base = std::env::temp_dir().join(format!(
            "mir3-composite-concurrency-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let project = base.join("组合并发项目");
        let quest_path = "引擎/Mir200/Envir/QuestDiary/quest.txt";
        let shop_path = "引擎/Mir200/Envir/Shop/shop.txt";
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200/Envir/QuestDiary")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200/Envir/Shop")).unwrap();
        fs::write(project.join(quest_path), "quest=0\n").unwrap();
        fs::write(project.join(shop_path), "shop=0\n").unwrap();
        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let imported = store.import_project(&project).unwrap();
        store.scan_project(&imported.id, || false).unwrap();
        let composite_id = "concurrent-release";

        let mut confirmations = Vec::new();
        for (system_id, path, content) in [
            ("quest", quest_path, "quest=1\n"),
            ("shop", shop_path, "shop=1\n"),
        ] {
            let draft = store
                .open_draft(&imported.id, &format!("并发更新 {system_id}"))
                .unwrap();
            store
                .bind_draft_domain(
                    &imported.id,
                    &draft.id,
                    system_id,
                    "1.3.1",
                    Some(composite_id),
                )
                .unwrap();
            let preview = store
                .patch_draft(
                    &imported.id,
                    &draft.id,
                    0,
                    &[DraftChangeInput {
                        path: path.to_string(),
                        content: Some(content.to_string()),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )
                .unwrap();
            confirmations.push(CompositeDraftConfirmation {
                draft_id: draft.id,
                expected_revision: preview.draft.revision,
                expected_diff_hash: preview.diff_hash,
            });
        }

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let worker_store = store.clone();
            let worker_project = imported.id.clone();
            let worker_confirmations = confirmations.clone();
            let worker_barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                worker_barrier.wait();
                worker_store.apply_composite_drafts(
                    &worker_project,
                    composite_id,
                    &worker_confirmations,
                )
            }));
        }
        barrier.wait();
        let outcomes = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
        let rejected = outcomes
            .iter()
            .find_map(|outcome| outcome.as_ref().err())
            .unwrap();
        assert!(
            rejected.starts_with("COMPOSITE_RESERVATION_CONFLICT:")
                || rejected.starts_with("DRAFT_MUTATION_RESERVED:"),
            "unexpected concurrent rejection: {rejected}"
        );
        assert_eq!(
            fs::read_to_string(project.join(quest_path)).unwrap(),
            "quest=1\n"
        );
        assert_eq!(
            fs::read_to_string(project.join(shop_path)).unwrap(),
            "shop=1\n"
        );
        assert_eq!(store.list_snapshots(&imported.id).unwrap().len(), 1);

        let applied = outcomes
            .into_iter()
            .find_map(Result::ok)
            .expect("one composite apply must succeed");
        store
            .restore_snapshot(&imported.id, &applied.snapshot.id)
            .unwrap();
        assert_eq!(
            fs::read_to_string(project.join(quest_path)).unwrap(),
            "quest=0\n"
        );
        assert_eq!(
            fs::read_to_string(project.join(shop_path)).unwrap(),
            "shop=0\n"
        );
        fs::remove_dir_all(base).ok();
    }
}
