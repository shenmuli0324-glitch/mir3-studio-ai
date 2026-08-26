//! 任务回执、用户能力、系统会话和作用域凭证的项目外治理数据。

use crate::{now_millis, DomainManifest, DomainStore, DraftPreview, DraftStatus, Snapshot};
use rusqlite::{params, OptionalExtension};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};

const TASK_SCOPE_MAX_TTL_MILLIS: i64 = 60 * 60 * 1_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskReceipt {
    pub id: String,
    pub task_id: String,
    pub system_id: String,
    pub summary: String,
    pub status: String,
    pub draft_id: Option<String>,
    pub plugin_versions: serde_json::Value,
    pub evidence: serde_json::Value,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserCapability {
    pub id: String,
    pub version: String,
    pub system_id: String,
    pub scope: String,
    pub name: String,
    pub description: String,
    pub parameter_schema: serde_json::Value,
    pub steps: serde_json::Value,
    pub read_systems: Vec<String>,
    pub write_systems: Vec<String>,
    pub status: String,
    pub source_task_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityCompileRequest {
    pub receipt_id: String,
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalCapabilityCompileRequest {
    pub receipt_ids: Vec<String>,
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityPromotionRequest {
    pub capability_id: String,
    pub version: String,
    pub target_scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityResolution {
    pub capability: UserCapability,
    pub resolved_scope: String,
    pub source_project_id: String,
    pub shadowed_scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRollbackRequest {
    pub capability_id: String,
    pub scope: String,
    pub from_version: String,
    pub to_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositeDraftBinding {
    pub draft_id: String,
    pub system_id: String,
    pub plugin_version: String,
    pub revision: i64,
}

#[derive(Debug, Clone)]
struct CompositeDraftCheckpoint {
    drafts: Vec<DraftCheckpoint>,
}

#[derive(Debug, Clone)]
struct DraftCheckpoint {
    draft_id: String,
    revision: i64,
    status: String,
    updated_at: i64,
    changes: Vec<DraftChangeCheckpoint>,
    evidence: Vec<DraftEvidenceCheckpoint>,
}

#[derive(Debug, Clone)]
struct DraftChangeCheckpoint {
    path: String,
    base_sha256: Option<String>,
    content: Option<Vec<u8>>,
    deleted: i64,
}

#[derive(Debug, Clone)]
struct DraftEvidenceCheckpoint {
    sequence: i64,
    system_id: String,
    plugin_version: String,
    operation_id: String,
    parameters: String,
    parameter_schema_hash: String,
    revision_before: i64,
    revision_after: i64,
    replay_change_hash: String,
    replay_evidence_hash: String,
    created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceMigrationReport {
    pub id: String,
    pub system_id: String,
    pub from_version: String,
    pub to_version: String,
    pub compatible: bool,
    pub migrated_capabilities: Vec<String>,
    pub migrated_memories: Vec<String>,
    pub deprecated_capabilities: Vec<String>,
    pub conflicts: Vec<String>,
    pub status: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct GovernanceSnapshot {
    system_id: String,
    project_rows: Vec<ProjectGovernanceSnapshot>,
    shared_capabilities: Vec<SharedCapabilityRow>,
    shared_memories: Vec<SharedMemoryRow>,
}

#[derive(Debug, Clone)]
struct ProjectGovernanceSnapshot {
    project_id: String,
    capabilities: Vec<UserCapability>,
    memories: Vec<DomainMemory>,
}

#[derive(Debug, Clone)]
struct SharedCapabilityRow {
    source_project_id: String,
    capability: UserCapability,
}

#[derive(Debug, Clone)]
struct SharedMemoryRow {
    source_project_id: String,
    memory: DomainMemory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftOperationEvidence {
    pub draft_id: String,
    pub sequence: i64,
    pub system_id: String,
    pub plugin_version: String,
    pub operation_id: String,
    pub parameters: serde_json::Value,
    pub parameter_schema_hash: String,
    pub revision_before: i64,
    pub revision_after: i64,
    pub replay_change_hash: String,
    pub replay_evidence_hash: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainMemory {
    pub id: String,
    pub system_id: String,
    pub scope: String,
    pub kind: String,
    pub summary: String,
    pub body: serde_json::Value,
    pub status: String,
    pub source_task_id: String,
    pub plugin_version: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSessionBinding {
    pub task_id: String,
    pub system_id: String,
    pub session_id: String,
    pub plugin_version: String,
    pub draft_id: Option<String>,
    pub status: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskScopeLease {
    pub token: String,
    pub task_id: String,
    pub read_systems: Vec<String>,
    pub write_systems: Vec<String>,
    pub draft_ids: Vec<String>,
    pub plugin_versions: serde_json::Value,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyDraftCloneRequest {
    pub legacy_draft_id: String,
    pub system_id: String,
    pub plugin_version: String,
    pub expected_sources: BTreeMap<String, String>,
    pub intent: String,
}

impl DomainStore {
    pub fn bind_draft_domain(
        &self,
        project_id: &str,
        draft_id: &str,
        system_id: &str,
        plugin_version: &str,
        composite_id: Option<&str>,
    ) -> Result<(), String> {
        let _mutation = self.reserve_draft_mutation(project_id, draft_id)?;
        if system_id != "__studio_gui__" {
            self.runtime_manifest_at_version(system_id, Some(plugin_version))?;
        }
        self.project_connection(project_id)?
            .execute(
                "INSERT INTO draft_domains(draft_id,system_id,composite_id,plugin_version,legacy)
                 VALUES(?1,?2,?3,?4,0)
                 ON CONFLICT(draft_id) DO UPDATE SET system_id=excluded.system_id,composite_id=excluded.composite_id,plugin_version=excluded.plugin_version,legacy=0",
                params![draft_id, system_id, composite_id, plugin_version],
            )
            .map_err(|error| format!("DRAFT_DOMAIN_BIND_FAILED: {error}"))?;
        Ok(())
    }

    /// 已有 Draft 只能在领域、插件版本和打开状态都一致时加入组合任务。
    pub fn associate_draft_composite(
        &self,
        project_id: &str,
        draft_id: &str,
        system_id: &str,
        plugin_version: &str,
        composite_id: &str,
    ) -> Result<(), String> {
        let _mutation = self.reserve_draft_mutation(project_id, draft_id)?;
        if composite_id.trim().is_empty() {
            return Err("COMPOSITE_DRAFT_ID_REQUIRED: composite id is required".to_string());
        }
        let draft = self.get_draft(project_id, draft_id)?;
        if draft.status != crate::DraftStatus::Open {
            return Err("COMPOSITE_DRAFT_NOT_OPEN: only open drafts can be associated".to_string());
        }
        let binding = self
            .project_connection(project_id)?
            .query_row(
                "SELECT system_id,plugin_version,legacy,composite_id FROM draft_domains WHERE draft_id=?1",
                [draft_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("COMPOSITE_DRAFT_BINDING_READ_FAILED: {error}"))?
            .ok_or_else(|| "COMPOSITE_DRAFT_DOMAIN_REQUIRED: draft is not scoped".to_string())?;
        if binding.2 != 0
            || binding.0.as_deref() != Some(system_id)
            || binding.1.as_deref() != Some(plugin_version)
        {
            return Err(
                "COMPOSITE_DRAFT_SCOPE_MISMATCH: draft domain or plugin version differs"
                    .to_string(),
            );
        }
        if binding
            .3
            .as_deref()
            .is_some_and(|value| value != composite_id)
        {
            return Err(
                "COMPOSITE_DRAFT_ALREADY_ASSOCIATED: draft belongs to another composite task"
                    .to_string(),
            );
        }
        self.project_connection(project_id)?
            .execute(
                "UPDATE draft_domains SET composite_id=?2 WHERE draft_id=?1",
                params![draft_id, composite_id],
            )
            .map_err(|error| format!("COMPOSITE_DRAFT_ASSOCIATE_FAILED: {error}"))?;
        Ok(())
    }

    /// 组合能力执行只读取已绑定且仍开放的领域 Draft，避免调用方自行猜测系统映射。
    pub fn list_composite_draft_bindings(
        &self,
        project_id: &str,
        composite_id: &str,
    ) -> Result<Vec<CompositeDraftBinding>, String> {
        if composite_id.trim().is_empty() {
            return Err("COMPOSITE_DRAFT_ID_REQUIRED: composite id is required".to_string());
        }
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT d.id,dd.system_id,dd.plugin_version,d.revision
                 FROM draft_domains dd JOIN drafts d ON d.id=dd.draft_id
                 WHERE dd.composite_id=?1 AND dd.legacy=0 AND d.status='open'
                 ORDER BY dd.system_id,d.id",
            )
            .map_err(|error| format!("COMPOSITE_DRAFT_BINDING_READ_FAILED: {error}"))?;
        let bindings = statement
            .query_map([composite_id], |row| {
                Ok(CompositeDraftBinding {
                    draft_id: row.get(0)?,
                    system_id: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    plugin_version: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    revision: row.get(3)?,
                })
            })
            .map_err(|error| format!("COMPOSITE_DRAFT_BINDING_READ_FAILED: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("COMPOSITE_DRAFT_BINDING_READ_FAILED: {error}"))?;
        if bindings.is_empty()
            || bindings
                .iter()
                .any(|binding| binding.system_id.is_empty() || binding.plugin_version.is_empty())
        {
            return Err(
                "COMPOSITE_DRAFT_BINDING_INVALID: no complete open domain bindings".to_string(),
            );
        }
        Ok(bindings)
    }

    /// 组合能力的多步执行失败时，在一个 SQLite 事务中恢复全部 Draft 内容、revision 与证据。
    pub fn with_composite_draft_transaction<T, F>(
        &self,
        project_id: &str,
        composite_id: &str,
        operation: F,
    ) -> Result<T, String>
    where
        F: FnOnce(&[CompositeDraftBinding]) -> Result<T, String>,
    {
        let bindings = self.list_composite_draft_bindings(project_id, composite_id)?;
        let draft_ids = bindings
            .iter()
            .map(|binding| binding.draft_id.clone())
            .collect::<Vec<_>>();
        let _reservation = self.reserve_draft_mutations(project_id, &draft_ids)?;
        let checkpoint = self.capture_composite_draft_checkpoint(project_id, &bindings)?;
        match operation(&bindings) {
            Ok(value) => Ok(value),
            Err(operation_error) => {
                self.restore_composite_draft_checkpoint(project_id, &checkpoint)
                    .map_err(|rollback_error| {
                        format!(
                            "COMPOSITE_CAPABILITY_ROLLBACK_FAILED: {operation_error} | {rollback_error}"
                        )
                    })?;
                Err(operation_error)
            }
        }
    }

    fn capture_composite_draft_checkpoint(
        &self,
        project_id: &str,
        bindings: &[CompositeDraftBinding],
    ) -> Result<CompositeDraftCheckpoint, String> {
        let connection = self.project_connection(project_id)?;
        let mut drafts = Vec::with_capacity(bindings.len());
        for binding in bindings {
            let (revision, status, updated_at) = connection
                .query_row(
                    "SELECT revision,status,updated_at FROM drafts WHERE id=?1",
                    [&binding.draft_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(|error| format!("COMPOSITE_CAPABILITY_CHECKPOINT_FAILED: {error}"))?;
            let mut changes_statement = connection
                .prepare(
                    "SELECT path,base_sha256,content,deleted FROM draft_changes WHERE draft_id=?1 ORDER BY path",
                )
                .map_err(|error| format!("COMPOSITE_CAPABILITY_CHECKPOINT_FAILED: {error}"))?;
            let changes = changes_statement
                .query_map([&binding.draft_id], |row| {
                    Ok(DraftChangeCheckpoint {
                        path: row.get(0)?,
                        base_sha256: row.get(1)?,
                        content: row.get(2)?,
                        deleted: row.get(3)?,
                    })
                })
                .map_err(|error| format!("COMPOSITE_CAPABILITY_CHECKPOINT_FAILED: {error}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("COMPOSITE_CAPABILITY_CHECKPOINT_FAILED: {error}"))?;
            let mut evidence_statement = connection
                .prepare(
                    "SELECT sequence,system_id,plugin_version,operation_id,parameters,parameter_schema_hash,revision_before,revision_after,replay_change_hash,replay_evidence_hash,created_at
                     FROM draft_operation_evidence WHERE draft_id=?1 ORDER BY sequence",
                )
                .map_err(|error| format!("COMPOSITE_CAPABILITY_CHECKPOINT_FAILED: {error}"))?;
            let evidence = evidence_statement
                .query_map([&binding.draft_id], |row| {
                    Ok(DraftEvidenceCheckpoint {
                        sequence: row.get(0)?,
                        system_id: row.get(1)?,
                        plugin_version: row.get(2)?,
                        operation_id: row.get(3)?,
                        parameters: row.get(4)?,
                        parameter_schema_hash: row.get(5)?,
                        revision_before: row.get(6)?,
                        revision_after: row.get(7)?,
                        replay_change_hash: row.get(8)?,
                        replay_evidence_hash: row.get(9)?,
                        created_at: row.get(10)?,
                    })
                })
                .map_err(|error| format!("COMPOSITE_CAPABILITY_CHECKPOINT_FAILED: {error}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("COMPOSITE_CAPABILITY_CHECKPOINT_FAILED: {error}"))?;
            drafts.push(DraftCheckpoint {
                draft_id: binding.draft_id.clone(),
                revision,
                status,
                updated_at,
                changes,
                evidence,
            });
        }
        Ok(CompositeDraftCheckpoint { drafts })
    }

    fn restore_composite_draft_checkpoint(
        &self,
        project_id: &str,
        checkpoint: &CompositeDraftCheckpoint,
    ) -> Result<(), String> {
        let mut connection = self.project_connection(project_id)?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("COMPOSITE_CAPABILITY_ROLLBACK_FAILED: {error}"))?;
        for draft in &checkpoint.drafts {
            transaction
                .execute(
                    "DELETE FROM draft_operation_evidence WHERE draft_id=?1",
                    [&draft.draft_id],
                )
                .map_err(|error| format!("COMPOSITE_CAPABILITY_ROLLBACK_FAILED: {error}"))?;
            transaction
                .execute(
                    "DELETE FROM draft_changes WHERE draft_id=?1",
                    [&draft.draft_id],
                )
                .map_err(|error| format!("COMPOSITE_CAPABILITY_ROLLBACK_FAILED: {error}"))?;
            for change in &draft.changes {
                transaction
                    .execute(
                        "INSERT INTO draft_changes(draft_id,path,base_sha256,content,deleted) VALUES(?1,?2,?3,?4,?5)",
                        params![draft.draft_id,change.path,change.base_sha256,change.content,change.deleted],
                    )
                    .map_err(|error| format!("COMPOSITE_CAPABILITY_ROLLBACK_FAILED: {error}"))?;
            }
            for evidence in &draft.evidence {
                transaction
                    .execute(
                        "INSERT INTO draft_operation_evidence(draft_id,sequence,system_id,plugin_version,operation_id,parameters,parameter_schema_hash,revision_before,revision_after,replay_change_hash,replay_evidence_hash,created_at)
                         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                        params![draft.draft_id,evidence.sequence,evidence.system_id,evidence.plugin_version,evidence.operation_id,evidence.parameters,evidence.parameter_schema_hash,evidence.revision_before,evidence.revision_after,evidence.replay_change_hash,evidence.replay_evidence_hash,evidence.created_at],
                    )
                    .map_err(|error| format!("COMPOSITE_CAPABILITY_ROLLBACK_FAILED: {error}"))?;
            }
            transaction
                .execute(
                    "UPDATE drafts SET revision=?2,status=?3,updated_at=?4 WHERE id=?1",
                    params![
                        draft.draft_id,
                        draft.revision,
                        draft.status,
                        draft.updated_at
                    ],
                )
                .map_err(|error| format!("COMPOSITE_CAPABILITY_ROLLBACK_FAILED: {error}"))?;
        }
        transaction
            .commit()
            .map_err(|error| format!("COMPOSITE_CAPABILITY_ROLLBACK_FAILED: {error}"))
    }

    /// 旧 Draft 只有在逐文件复核当前真实源哈希后，才能克隆成新的领域 Draft。
    pub fn clone_legacy_draft(
        &self,
        project_id: &str,
        request: &LegacyDraftCloneRequest,
    ) -> Result<DraftPreview, String> {
        self.ensure_known_system(&request.system_id)?;
        let connection = self.project_connection(project_id)?;
        let legacy = connection
            .query_row(
                "SELECT legacy FROM draft_domains WHERE draft_id=?1",
                [&request.legacy_draft_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| format!("DRAFT_LEGACY_READ_FAILED: {error}"))?
            .ok_or_else(|| "DRAFT_LEGACY_REQUIRED: draft has no legacy marker".to_string())?;
        if legacy == 0 {
            return Err("DRAFT_LEGACY_REQUIRED: draft is already scoped".to_string());
        }
        let mut statement = connection
            .prepare(
                "SELECT path,base_sha256,content,deleted FROM draft_changes
                 WHERE draft_id=?1 ORDER BY path",
            )
            .map_err(|error| format!("DRAFT_LEGACY_READ_FAILED: {error}"))?;
        let changes = statement
            .query_map([&request.legacy_draft_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|error| format!("DRAFT_LEGACY_READ_FAILED: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("DRAFT_LEGACY_READ_FAILED: {error}"))?;
        if changes.is_empty() || request.expected_sources.len() != changes.len() {
            return Err(
                "DRAFT_LEGACY_SOURCE_REVIEW_REQUIRED: expectedSources must cover every changed file"
                    .to_string(),
            );
        }
        let project = self.get_project(project_id)?;
        let root = fs::canonicalize(&project.root)
            .map_err(|error| format!("PROJECT_PATH_INVALID: {error}"))?;
        for (path, base_sha256, _, _) in &changes {
            validate_legacy_relative_path(path)?;
            let target = root.join(path);
            let bytes = fs::read(&target)
                .map_err(|error| format!("DRAFT_LEGACY_SOURCE_READ_FAILED: {path}: {error}"))?;
            let current = hash_bytes(&bytes);
            let reviewed = request
                .expected_sources
                .get(path)
                .ok_or_else(|| format!("DRAFT_LEGACY_SOURCE_REVIEW_REQUIRED: missing {path}"))?;
            if reviewed != &current || base_sha256.as_deref() != Some(current.as_str()) {
                return Err(format!("DRAFT_LEGACY_SOURCE_CONFLICT: {path}"));
            }
        }
        drop(statement);
        drop(connection);
        let draft = self.open_draft(project_id, &request.intent)?;
        let _mutation = self.reserve_draft_mutation(project_id, &draft.id)?;
        let clone_result = (|| -> Result<DraftPreview, String> {
            self.bind_draft_domain(
                project_id,
                &draft.id,
                &request.system_id,
                &request.plugin_version,
                None,
            )?;
            #[cfg(test)]
            self.wait_governance_copy_test_gate()?;
            for (path, _, _, _) in &changes {
                self.assert_draft_path_writable(project_id, &draft.id, path)?;
            }
            let mut connection = self.project_connection(project_id)?;
            let transaction = connection
                .transaction()
                .map_err(|error| format!("DRAFT_LEGACY_CLONE_FAILED: {error}"))?;
            for (path, base_sha256, content, deleted) in changes {
                transaction
                    .execute(
                        "INSERT INTO draft_changes(draft_id,path,base_sha256,content,deleted)
                         VALUES(?1,?2,?3,?4,?5)",
                        params![draft.id, path, base_sha256, content, deleted],
                    )
                    .map_err(|error| format!("DRAFT_LEGACY_CLONE_FAILED: {error}"))?;
            }
            let updated = transaction
                .execute(
                    "UPDATE drafts SET revision=1,updated_at=?2 WHERE id=?1 AND revision=0 AND status='open'",
                    params![draft.id, now_millis()],
                )
                .map_err(|error| format!("DRAFT_LEGACY_CLONE_FAILED: {error}"))?;
            if updated != 1 {
                return Err(
                    "DRAFT_LEGACY_CLONE_CONFLICT: new Draft changed concurrently".to_string(),
                );
            }
            transaction
                .commit()
                .map_err(|error| format!("DRAFT_LEGACY_CLONE_FAILED: {error}"))?;
            self.preview_draft(project_id, &draft.id)
        })();
        if clone_result.is_err() {
            self.discard_draft(project_id, &draft.id).ok();
        }
        clone_result
    }

    pub fn save_task_receipt(
        &self,
        project_id: &str,
        receipt: &TaskReceipt,
    ) -> Result<TaskReceipt, String> {
        self.ensure_known_system(&receipt.system_id)?;
        let candidate = matches!(receipt.status.as_str(), "applied" | "completed" | "success")
            .then(|| memory_candidate_for_receipt(receipt));
        let mut connection = self.project_connection(project_id)?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("TASK_RECEIPT_TRANSACTION_FAILED: {error}"))?;
        transaction
            .execute(
                "INSERT INTO task_receipts(id,task_id,system_id,summary,status,draft_id,plugin_versions,evidence,created_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
                 ON CONFLICT(id) DO UPDATE SET summary=excluded.summary,status=excluded.status,draft_id=excluded.draft_id,plugin_versions=excluded.plugin_versions,evidence=excluded.evidence",
                params![
                    receipt.id,
                    receipt.task_id,
                    receipt.system_id,
                    receipt.summary,
                    receipt.status,
                    receipt.draft_id,
                    receipt.plugin_versions.to_string(),
                    receipt.evidence.to_string(),
                    receipt.created_at,
                ],
            )
            .map_err(|error| format!("TASK_RECEIPT_WRITE_FAILED: {error}"))?;
        if let Some(candidate) = candidate {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO domain_memories(id,system_id,scope,kind,summary,body,status,source_task_id,plugin_version,created_at,updated_at)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                    params![candidate.id,candidate.system_id,candidate.scope,candidate.kind,candidate.summary,candidate.body.to_string(),candidate.status,candidate.source_task_id,candidate.plugin_version,candidate.created_at,candidate.updated_at],
                )
                .map_err(|error| format!("TASK_MEMORY_WRITE_FAILED: {error}"))?;
        }
        transaction
            .commit()
            .map_err(|error| format!("TASK_RECEIPT_TRANSACTION_FAILED: {error}"))?;
        Ok(receipt.clone())
    }

    pub fn list_task_receipts(
        &self,
        project_id: &str,
        system_id: Option<&str>,
    ) -> Result<Vec<TaskReceipt>, String> {
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT id,task_id,system_id,summary,status,draft_id,plugin_versions,evidence,created_at
                 FROM task_receipts WHERE (?1 IS NULL OR system_id=?1) ORDER BY created_at DESC",
            )
            .map_err(|error| format!("TASK_RECEIPT_LIST_FAILED: {error}"))?;
        let rows = statement
            .query_map([system_id], |row| {
                let plugin_versions: String = row.get(6)?;
                let evidence: String = row.get(7)?;
                Ok(TaskReceipt {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    system_id: row.get(2)?,
                    summary: row.get(3)?,
                    status: row.get(4)?,
                    draft_id: row.get(5)?,
                    plugin_versions: serde_json::from_str(&plugin_versions)
                        .unwrap_or(serde_json::Value::Null),
                    evidence: serde_json::from_str(&evidence).unwrap_or(serde_json::Value::Null),
                    created_at: row.get(8)?,
                })
            })
            .map_err(|error| format!("TASK_RECEIPT_LIST_FAILED: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("TASK_RECEIPT_LIST_FAILED: {error}"))
    }

    pub fn record_applied_draft_receipt(
        &self,
        project_id: &str,
        draft_id: &str,
        diff_hash: &str,
        snapshot: &Snapshot,
    ) -> Result<Option<TaskReceipt>, String> {
        let draft = self.get_draft(project_id, draft_id)?;
        let connection = self.project_connection(project_id)?;
        let binding = connection
            .query_row(
                "SELECT system_id,plugin_version FROM draft_domains WHERE draft_id=?1",
                [draft_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("TASK_RECEIPT_DOMAIN_READ_FAILED: {error}"))?;
        let Some((system_id, plugin_version)) =
            binding.and_then(|(system_id, plugin_version)| system_id.zip(plugin_version))
        else {
            return Ok(None);
        };
        let task_id = connection
            .query_row(
                "SELECT task_id FROM system_sessions WHERE draft_id=?1 ORDER BY updated_at DESC LIMIT 1",
                [draft_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("TASK_RECEIPT_SESSION_READ_FAILED: {error}"))?
            .unwrap_or_else(|| format!("draft:{draft_id}"));
        let created_at = now_millis();
        let id = format!(
            "receipt-{}",
            &hash(&format!("{project_id}:{task_id}:{draft_id}:{created_at}"))[..20]
        );
        let receipt = TaskReceipt {
            id,
            task_id,
            system_id,
            summary: draft.intent,
            status: "applied".to_string(),
            draft_id: Some(draft_id.to_string()),
            plugin_versions: serde_json::json!({"domain":plugin_version}),
            evidence: serde_json::json!({
                "snapshotId": snapshot.id,
                "diffHash": diff_hash,
                "revision": draft.revision,
                "files": snapshot.files,
            }),
            created_at,
        };
        self.save_task_receipt(project_id, &receipt).map(Some)
    }

    /// MCP 安全编译器在每次成功操作后记录不可由前端构造的执行证据。
    pub fn record_draft_operation_evidence(
        &self,
        project_id: &str,
        draft_id: &str,
        operation_id: &str,
        parameters: &serde_json::Value,
        revision_before: i64,
        revision_after: i64,
    ) -> Result<DraftOperationEvidence, String> {
        let _mutation = self.reserve_draft_mutation(project_id, draft_id)?;
        let draft = self.get_draft(project_id, draft_id)?;
        if draft.status != DraftStatus::Open || draft.revision != revision_after {
            return Err("CAPABILITY_EVIDENCE_DRAFT_STATE_INVALID: operation evidence must follow a successful open Draft mutation".to_string());
        }
        if revision_after <= revision_before
            || parameters
                .get("operation")
                .and_then(serde_json::Value::as_str)
                != Some(operation_id)
        {
            return Err("CAPABILITY_EVIDENCE_REVISION_INVALID: operation parameters or revision chain is invalid".to_string());
        }
        let manifest = self.draft_domain_manifest(project_id, draft_id)?;
        let operation = manifest
            .operations
            .iter()
            .find(|operation| operation.id == operation_id)
            .ok_or_else(|| {
                format!(
                    "CAPABILITY_OPERATION_NOT_REGISTERED: {}:{operation_id}",
                    manifest.system_id
                )
            })?;
        let schema_hash = hash_json(&operation.parameter_schema)?;
        let mut connection = self.project_connection(project_id)?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("CAPABILITY_EVIDENCE_WRITE_FAILED: {error}"))?;
        let previous = transaction
            .query_row(
                "SELECT revision_after FROM draft_operation_evidence WHERE draft_id=?1 ORDER BY sequence DESC LIMIT 1",
                [draft_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| format!("CAPABILITY_EVIDENCE_READ_FAILED: {error}"))?;
        if previous.is_some_and(|revision| revision != revision_before) {
            return Err(
                "CAPABILITY_EVIDENCE_CHAIN_INVALID: operation revisions are not contiguous"
                    .to_string(),
            );
        }
        let sequence = transaction
            .query_row(
                "SELECT COALESCE(MAX(sequence),0)+1 FROM draft_operation_evidence WHERE draft_id=?1",
                [draft_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| format!("CAPABILITY_EVIDENCE_READ_FAILED: {error}"))?;
        let evidence = DraftOperationEvidence {
            draft_id: draft_id.to_string(),
            sequence,
            system_id: manifest.system_id,
            plugin_version: manifest.version,
            operation_id: operation_id.to_string(),
            parameters: parameters.clone(),
            parameter_schema_hash: schema_hash,
            revision_before,
            revision_after,
            replay_change_hash: String::new(),
            replay_evidence_hash: String::new(),
            created_at: now_millis(),
        };
        transaction
            .execute(
                "INSERT INTO draft_operation_evidence(draft_id,sequence,system_id,plugin_version,operation_id,parameters,parameter_schema_hash,revision_before,revision_after,replay_change_hash,replay_evidence_hash,created_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![evidence.draft_id,evidence.sequence,evidence.system_id,evidence.plugin_version,evidence.operation_id,evidence.parameters.to_string(),evidence.parameter_schema_hash,evidence.revision_before,evidence.revision_after,evidence.replay_change_hash,evidence.replay_evidence_hash,evidence.created_at],
            )
            .map_err(|error| format!("CAPABILITY_EVIDENCE_WRITE_FAILED: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("CAPABILITY_EVIDENCE_WRITE_FAILED: {error}"))?;
        Ok(evidence)
    }

    pub fn list_draft_operation_evidence(
        &self,
        project_id: &str,
        draft_id: &str,
    ) -> Result<Vec<DraftOperationEvidence>, String> {
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT draft_id,sequence,system_id,plugin_version,operation_id,parameters,parameter_schema_hash,revision_before,revision_after,replay_change_hash,replay_evidence_hash,created_at
                 FROM draft_operation_evidence WHERE draft_id=?1 ORDER BY sequence",
            )
            .map_err(|error| format!("CAPABILITY_EVIDENCE_READ_FAILED: {error}"))?;
        let rows = statement
            .query_map([draft_id], |row| {
                let parameters: String = row.get(5)?;
                Ok(DraftOperationEvidence {
                    draft_id: row.get(0)?,
                    sequence: row.get(1)?,
                    system_id: row.get(2)?,
                    plugin_version: row.get(3)?,
                    operation_id: row.get(4)?,
                    parameters: serde_json::from_str(&parameters).unwrap_or_default(),
                    parameter_schema_hash: row.get(6)?,
                    revision_before: row.get(7)?,
                    revision_after: row.get(8)?,
                    replay_change_hash: row.get(9)?,
                    replay_evidence_hash: row.get(10)?,
                    created_at: row.get(11)?,
                })
            })
            .map_err(|error| format!("CAPABILITY_EVIDENCE_READ_FAILED: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("CAPABILITY_EVIDENCE_READ_FAILED: {error}"))
    }

    /// 安全执行器完成隔离重放后封存该次操作的完整前缀证明。
    pub fn seal_draft_operation_replay(
        &self,
        project_id: &str,
        draft_id: &str,
        sequence: i64,
        replay_change_hash: &str,
    ) -> Result<DraftOperationEvidence, String> {
        let _mutation = self.reserve_draft_mutation(project_id, draft_id)?;
        let evidence = self.list_draft_operation_evidence(project_id, draft_id)?;
        let current = evidence
            .last()
            .filter(|item| item.sequence == sequence)
            .ok_or_else(|| {
                "CAPABILITY_REPLAY_SEQUENCE_INVALID: only the latest operation can be sealed"
                    .to_string()
            })?;
        if !current.replay_change_hash.is_empty() || replay_change_hash.is_empty() {
            return Err(
                "CAPABILITY_REPLAY_ALREADY_SEALED: replay proof is invalid or already sealed"
                    .to_string(),
            );
        }
        let replay_evidence_hash = operation_evidence_prefix_hash(&evidence)?;
        let changed = self
            .project_connection(project_id)?
            .execute(
                "UPDATE draft_operation_evidence SET replay_change_hash=?3,replay_evidence_hash=?4
                 WHERE draft_id=?1 AND sequence=?2 AND replay_change_hash='' AND replay_evidence_hash=''",
                params![draft_id,sequence,replay_change_hash,replay_evidence_hash],
            )
            .map_err(|error| format!("CAPABILITY_REPLAY_SEAL_FAILED: {error}"))?;
        if changed != 1 {
            return Err("CAPABILITY_REPLAY_SEAL_CONFLICT: replay evidence changed".to_string());
        }
        self.list_draft_operation_evidence(project_id, draft_id)?
            .into_iter()
            .find(|item| item.sequence == sequence)
            .ok_or_else(|| "CAPABILITY_REPLAY_SEAL_FAILED: sealed evidence disappeared".to_string())
    }

    /// 仅从成功回执、固定版本契约和服务端操作证据编译项目能力。
    pub fn compile_user_capability(
        &self,
        project_id: &str,
        request: &CapabilityCompileRequest,
    ) -> Result<UserCapability, String> {
        let receipt = self.get_task_receipt(project_id, &request.receipt_id)?;
        if !matches!(receipt.status.as_str(), "applied" | "completed" | "success") {
            return Err("CAPABILITY_RECEIPT_NOT_SUCCESSFUL: only a successful applied task can become a capability".to_string());
        }
        let draft_id = receipt.draft_id.as_deref().ok_or_else(|| {
            "CAPABILITY_RECEIPT_DRAFT_REQUIRED: receipt has no applied Draft".to_string()
        })?;
        let draft = self.get_draft(project_id, draft_id)?;
        if draft.status != DraftStatus::Applied {
            return Err(
                "CAPABILITY_RECEIPT_DRAFT_NOT_APPLIED: source Draft must be applied".to_string(),
            );
        }
        let manifest = self.draft_domain_manifest(project_id, draft_id)?;
        if manifest.system_id != receipt.system_id
            || receipt_plugin_version(&receipt) != Some(manifest.version.as_str())
        {
            return Err("CAPABILITY_RECEIPT_VERSION_MISMATCH: receipt does not match the Draft pinned domain pack".to_string());
        }
        let evidence = self.list_draft_operation_evidence(project_id, draft_id)?;
        verify_operation_evidence(&manifest, &draft, &evidence)?;
        let receipt_diff_hash = receipt
            .evidence
            .get("diffHash")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                "CAPABILITY_RECEIPT_DIFF_REQUIRED: successful receipt has no diff hash".to_string()
            })?;
        let preview = self.preview_draft(project_id, draft_id)?;
        if preview.diff_hash != receipt_diff_hash {
            return Err(
                "CAPABILITY_RECEIPT_DIFF_MISMATCH: receipt diff no longer matches its Draft"
                    .to_string(),
            );
        }
        let replay_hash =
            self.replay_capability_source(project_id, &receipt, &manifest, &evidence)?;
        let parameter_schema = promoted_parameter_schema(&manifest, &evidence)?;
        let mut read_systems = Vec::new();
        let mut write_systems = Vec::new();
        let mut steps = Vec::with_capacity(evidence.len());
        for item in &evidence {
            let operation = manifest
                .operations
                .iter()
                .find(|operation| operation.id == item.operation_id)
                .ok_or_else(|| {
                    format!("CAPABILITY_OPERATION_NOT_REGISTERED: {}", item.operation_id)
                })?;
            append_unique(&mut read_systems, &operation.read_systems);
            append_unique(&mut write_systems, &operation.write_systems);
            steps.push(serde_json::json!({
                "type":"domain-operation",
                "operation":item.operation_id,
                "pluginVersion":item.plugin_version,
                "parameterSchemaHash":item.parameter_schema_hash,
                "evidenceHash":hash_json(item)?,
                "replayHash":replay_hash,
                "sourceReceiptId":receipt.id,
                "preconditions":operation.preconditions,
            }));
        }
        let now = now_millis();
        let capability = UserCapability {
            id: request.id.clone(),
            version: "0.1.0".to_string(),
            system_id: manifest.system_id,
            scope: "project".to_string(),
            name: request.name.trim().to_string(),
            description: request.description.trim().to_string(),
            parameter_schema,
            steps: serde_json::Value::Array(steps),
            read_systems,
            write_systems,
            status: "draft".to_string(),
            source_task_id: receipt.task_id,
            created_at: now,
            updated_at: now,
        };
        self.save_user_capability(project_id, &capability)
    }

    /// 组合任务只能从同一次原子提交产生的多个成功回执编译为全局工作流。
    pub fn compile_global_workflow_capability(
        &self,
        project_id: &str,
        request: &GlobalCapabilityCompileRequest,
    ) -> Result<UserCapability, String> {
        let receipt_ids = request
            .receipt_ids
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<BTreeSet<_>>();
        if receipt_ids.len() < 2 {
            return Err(
                "GLOBAL_CAPABILITY_RECEIPTS_REQUIRED: at least two distinct receipts are required"
                    .to_string(),
            );
        }
        let mut snapshot_id: Option<String> = None;
        let mut composite_id: Option<String> = None;
        let mut read_systems = Vec::new();
        let mut write_systems = Vec::new();
        let mut steps = Vec::new();
        let mut schemas = serde_json::Map::new();
        let mut task_ids = Vec::new();

        for (receipt_index, receipt_id) in receipt_ids.iter().enumerate() {
            let receipt = self.get_task_receipt(project_id, receipt_id)?;
            if receipt.status != "applied" {
                return Err(format!(
                    "GLOBAL_CAPABILITY_RECEIPT_NOT_APPLIED: {}",
                    receipt.id
                ));
            }
            let current_snapshot = receipt
                .evidence
                .get("snapshotId")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "GLOBAL_CAPABILITY_SNAPSHOT_REQUIRED: receipt {} has no snapshot",
                        receipt.id
                    )
                })?;
            if snapshot_id
                .as_deref()
                .is_some_and(|expected| expected != current_snapshot)
            {
                return Err(
                    "GLOBAL_CAPABILITY_NOT_ATOMIC: receipts do not share one snapshot".to_string(),
                );
            }
            snapshot_id = Some(current_snapshot.to_string());
            let draft_id = receipt.draft_id.as_deref().ok_or_else(|| {
                "GLOBAL_CAPABILITY_DRAFT_REQUIRED: receipt has no Draft".to_string()
            })?;
            let draft = self.get_draft(project_id, draft_id)?;
            if draft.status != DraftStatus::Applied {
                return Err(format!("GLOBAL_CAPABILITY_DRAFT_NOT_APPLIED: {draft_id}"));
            }
            let (draft_system, draft_composite, draft_version) = self
                .project_connection(project_id)?
                .query_row(
                    "SELECT system_id,composite_id,plugin_version FROM draft_domains WHERE draft_id=?1 AND legacy=0",
                    [draft_id],
                    |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<String>>(2)?)),
                )
                .optional()
                .map_err(|error| format!("GLOBAL_CAPABILITY_DRAFT_READ_FAILED: {error}"))?
                .and_then(|(system, composite, version)| Some((system?, composite?, version?)))
                .ok_or_else(|| "GLOBAL_CAPABILITY_COMPOSITE_REQUIRED: Draft is not composite-scoped".to_string())?;
            if composite_id
                .as_deref()
                .is_some_and(|expected| expected != draft_composite)
            {
                return Err(
                    "GLOBAL_CAPABILITY_NOT_ATOMIC: Drafts belong to different composites"
                        .to_string(),
                );
            }
            composite_id = Some(draft_composite);
            if receipt.system_id != draft_system
                || receipt_plugin_version(&receipt) != Some(draft_version.as_str())
            {
                return Err(format!(
                    "GLOBAL_CAPABILITY_RECEIPT_SCOPE_MISMATCH: {}",
                    receipt.id
                ));
            }
            let manifest = self.runtime_manifest_at_version(&draft_system, Some(&draft_version))?;
            let evidence = self.list_draft_operation_evidence(project_id, draft_id)?;
            verify_operation_evidence(&manifest, &draft, &evidence)?;
            let preview = self.preview_draft(project_id, draft_id)?;
            if receipt
                .evidence
                .get("diffHash")
                .and_then(serde_json::Value::as_str)
                != Some(preview.diff_hash.as_str())
            {
                return Err(format!(
                    "GLOBAL_CAPABILITY_RECEIPT_DIFF_MISMATCH: {}",
                    receipt.id
                ));
            }
            let replay_hash =
                self.replay_capability_source(project_id, &receipt, &manifest, &evidence)?;
            schemas.insert(
                format!("system{receipt_index}"),
                promoted_parameter_schema(&manifest, &evidence)?,
            );
            for (operation_index, item) in evidence.into_iter().enumerate() {
                let operation = manifest
                    .operations
                    .iter()
                    .find(|candidate| candidate.id == item.operation_id)
                    .ok_or_else(|| {
                        format!("CAPABILITY_OPERATION_NOT_REGISTERED: {}", item.operation_id)
                    })?;
                append_unique(&mut read_systems, &operation.read_systems);
                append_unique(&mut write_systems, &operation.write_systems);
                steps.push(serde_json::json!({
                    "type":"domain-operation",
                    "systemId":draft_system,
                    "operation":item.operation_id,
                    "pluginVersion":draft_version,
                    "parameterSchemaHash":item.parameter_schema_hash,
                    "evidenceHash":hash_json(&item)?,
                    "replayHash":replay_hash,
                    "sourceReceiptId":receipt.id,
                    "parameterKey":format!("system{receipt_index}"),
                    "operationIndex":operation_index,
                    "preconditions":operation.preconditions,
                }));
            }
            task_ids.push(receipt.task_id);
        }
        if write_systems.len() < 2 {
            return Err(
                "GLOBAL_CAPABILITY_MULTI_SYSTEM_REQUIRED: workflow must write at least two systems"
                    .to_string(),
            );
        }
        let now = now_millis();
        let capability = UserCapability {
            id: request.id.clone(),
            version: "0.1.0".to_string(),
            system_id: "__global__".to_string(),
            scope: "project".to_string(),
            name: request.name.trim().to_string(),
            description: request.description.trim().to_string(),
            parameter_schema: serde_json::json!({
                "type":"object",
                "additionalProperties":false,
                "properties":schemas,
            }),
            steps: serde_json::Value::Array(steps),
            read_systems,
            write_systems,
            status: "draft".to_string(),
            source_task_id: format!("global:{}", &hash(&task_ids.join(":"))[..20]),
            created_at: now,
            updated_at: now,
        };
        self.save_user_capability(project_id, &capability)
    }

    fn get_task_receipt(&self, project_id: &str, receipt_id: &str) -> Result<TaskReceipt, String> {
        self.project_connection(project_id)?
            .query_row(
                "SELECT id,task_id,system_id,summary,status,draft_id,plugin_versions,evidence,created_at FROM task_receipts WHERE id=?1",
                [receipt_id],
                row_to_receipt,
            )
            .optional()
            .map_err(|error| format!("TASK_RECEIPT_READ_FAILED: {error}"))?
            .ok_or_else(|| format!("TASK_RECEIPT_NOT_FOUND: {receipt_id}"))
    }

    fn replay_capability_source(
        &self,
        project_id: &str,
        receipt: &TaskReceipt,
        manifest: &DomainManifest,
        evidence: &[DraftOperationEvidence],
    ) -> Result<String, String> {
        let source_draft_id = receipt
            .draft_id
            .as_deref()
            .ok_or_else(|| "CAPABILITY_RECEIPT_DRAFT_REQUIRED: receipt has no Draft".to_string())?;
        let source_hash = self.draft_change_evidence_hash(project_id, source_draft_id)?;
        verify_replay_proofs(evidence, &source_hash)?;
        let replay = self.open_draft(project_id, "capability isolated replay")?;
        let _mutation = self.reserve_draft_mutation(project_id, &replay.id)?;
        self.bind_draft_domain(
            project_id,
            &replay.id,
            &manifest.system_id,
            &manifest.version,
            None,
        )?;
        #[cfg(test)]
        self.wait_governance_copy_test_gate()?;
        let copy_result = (|| -> Result<(), String> {
            let source = self.project_connection(project_id)?;
            let mut statement = source
                .prepare(
                    "SELECT path,base_sha256,content,deleted FROM draft_changes WHERE draft_id=?1 ORDER BY path",
                )
                .map_err(|error| format!("CAPABILITY_REPLAY_READ_FAILED: {error}"))?;
            let changes = statement
                .query_map([source_draft_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<Vec<u8>>>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })
                .map_err(|error| format!("CAPABILITY_REPLAY_READ_FAILED: {error}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("CAPABILITY_REPLAY_READ_FAILED: {error}"))?;
            drop(statement);
            drop(source);
            if changes.is_empty() {
                return Err("CAPABILITY_REPLAY_EMPTY: source Draft has no changes".to_string());
            }
            let mut connection = self.project_connection(project_id)?;
            let transaction = connection
                .transaction()
                .map_err(|error| format!("CAPABILITY_REPLAY_WRITE_FAILED: {error}"))?;
            for (path, base_sha256, content, deleted) in changes {
                transaction
                    .execute(
                        "INSERT INTO draft_changes(draft_id,path,base_sha256,content,deleted) VALUES(?1,?2,?3,?4,?5)",
                        params![replay.id,path,base_sha256,content,deleted],
                    )
                    .map_err(|error| format!("CAPABILITY_REPLAY_WRITE_FAILED: {error}"))?;
            }
            let updated = transaction
                .execute(
                    "UPDATE drafts SET revision=?2,updated_at=?3 WHERE id=?1 AND revision=0 AND status='open'",
                    params![
                        replay.id,
                        evidence.last().map_or(0, |item| item.revision_after),
                        now_millis()
                    ],
                )
                .map_err(|error| format!("CAPABILITY_REPLAY_WRITE_FAILED: {error}"))?;
            if updated != 1 {
                return Err(
                    "CAPABILITY_REPLAY_WRITE_CONFLICT: new Draft changed concurrently".to_string(),
                );
            }
            transaction
                .commit()
                .map_err(|error| format!("CAPABILITY_REPLAY_WRITE_FAILED: {error}"))
        })();
        if let Err(error) = copy_result {
            self.discard_draft(project_id, &replay.id).ok();
            return Err(error);
        }
        let replay_result = (|| -> Result<String, String> {
            let replay_change_hash = self.draft_change_evidence_hash(project_id, &replay.id)?;
            if replay_change_hash != source_hash {
                return Err(
                    "CAPABILITY_REPLAY_DIFF_MISMATCH: isolated Draft differs from source"
                        .to_string(),
                );
            }
            let validation = self.validate_domain_draft(project_id, &replay.id)?;
            if !validation.valid {
                return Err(format!(
                    "CAPABILITY_REPLAY_VALIDATION_FAILED: {}",
                    validation.diagnostics.join(" | ")
                ));
            }
            hash_json(&serde_json::json!({
                "receiptId":receipt.id,
                "sourceDraftId":source_draft_id,
                "systemId":manifest.system_id,
                "pluginVersion":manifest.version,
                "changeHash":source_hash,
                "operationEvidence":evidence,
            }))
        })();
        self.discard_draft(project_id, &replay.id).ok();
        replay_result
    }

    pub fn draft_change_evidence_hash(
        &self,
        project_id: &str,
        draft_id: &str,
    ) -> Result<String, String> {
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT path,base_sha256,content,deleted FROM draft_changes WHERE draft_id=?1 ORDER BY path",
            )
            .map_err(|error| format!("CAPABILITY_DIFF_READ_FAILED: {error}"))?;
        let rows = statement
            .query_map([draft_id], |row| {
                Ok(serde_json::json!({
                    "path":row.get::<_, String>(0)?,
                    "baseSha256":row.get::<_, Option<String>>(1)?,
                    "contentSha256":row.get::<_, Option<Vec<u8>>>(2)?.as_deref().map(hash_bytes),
                    "deleted":row.get::<_, i64>(3)? != 0,
                }))
            })
            .map_err(|error| format!("CAPABILITY_DIFF_READ_FAILED: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("CAPABILITY_DIFF_READ_FAILED: {error}"))?;
        hash_json(&rows)
    }

    pub fn save_user_capability(
        &self,
        project_id: &str,
        capability: &UserCapability,
    ) -> Result<UserCapability, String> {
        validate_capability(capability)?;
        if capability.status != "draft" {
            return Err(
                "CAPABILITY_CREATION_STATUS_INVALID: new capability versions start as draft"
                    .to_string(),
            );
        }
        if capability.system_id != "__global__" {
            self.ensure_known_system(&capability.system_id)?;
        }
        for step in capability.steps.as_array().into_iter().flatten() {
            let operation = step
                .get("operation")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "CAPABILITY_STEP_INVALID: operation is required".to_string())?;
            let plugin_version = step
                .get("pluginVersion")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    "CAPABILITY_STEP_VERSION_REQUIRED: compiled steps must pin a domain pack"
                        .to_string()
                })?;
            let step_system = step
                .get("systemId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&capability.system_id);
            if capability.system_id == "__global__"
                && !capability
                    .write_systems
                    .iter()
                    .any(|value| value == step_system)
            {
                return Err(format!(
                    "CAPABILITY_STEP_SCOPE_DENIED: {step_system} is outside writeSystems"
                ));
            }
            let manifest = self.runtime_manifest_at_version(step_system, Some(plugin_version))?;
            let registered = manifest
                .operations
                .iter()
                .find(|registered| registered.id == operation)
                .ok_or_else(|| {
                    format!(
                        "CAPABILITY_OPERATION_NOT_REGISTERED: {}:{operation}",
                        step_system
                    )
                })?;
            let expected_schema_hash = step
                .get("parameterSchemaHash")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    "CAPABILITY_STEP_SCHEMA_HASH_REQUIRED: compiled step has no schema evidence"
                        .to_string()
                })?;
            if hash_json(&registered.parameter_schema)? != expected_schema_hash {
                return Err(format!("CAPABILITY_OPERATION_SCHEMA_MISMATCH: {operation}"));
            }
        }
        for system_id in capability
            .read_systems
            .iter()
            .chain(capability.write_systems.iter())
        {
            self.ensure_known_system(system_id)?;
        }
        self.project_connection(project_id)?
            .execute(
                "INSERT INTO user_capabilities(id,version,system_id,scope,name,description,parameter_schema,steps,read_systems,write_systems,status,source_task_id,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
                 ON CONFLICT(id,version) DO UPDATE SET name=excluded.name,description=excluded.description,parameter_schema=excluded.parameter_schema,steps=excluded.steps,read_systems=excluded.read_systems,write_systems=excluded.write_systems,status=excluded.status,updated_at=excluded.updated_at",
                params![
                    capability.id,
                    capability.version,
                    capability.system_id,
                    capability.scope,
                    capability.name,
                    capability.description,
                    capability.parameter_schema.to_string(),
                    capability.steps.to_string(),
                    serde_json::to_string(&capability.read_systems).unwrap_or_else(|_| "[]".to_string()),
                    serde_json::to_string(&capability.write_systems).unwrap_or_else(|_| "[]".to_string()),
                    capability.status,
                    capability.source_task_id,
                    capability.created_at,
                    capability.updated_at,
                ],
            )
            .map_err(|error| format!("CAPABILITY_WRITE_FAILED: {error}"))?;
        Ok(capability.clone())
    }

    pub fn list_user_capabilities(
        &self,
        project_id: &str,
        system_id: Option<&str>,
    ) -> Result<Vec<UserCapability>, String> {
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT id,version,system_id,scope,name,description,parameter_schema,steps,read_systems,write_systems,status,source_task_id,created_at,updated_at
                 FROM user_capabilities WHERE (?1 IS NULL OR system_id=?1) ORDER BY updated_at DESC,id,version DESC",
            )
            .map_err(|error| format!("CAPABILITY_LIST_FAILED: {error}"))?;
        let rows = statement
            .query_map([system_id], row_to_capability)
            .map_err(|error| format!("CAPABILITY_LIST_FAILED: {error}"))?;
        let mut capabilities = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("CAPABILITY_LIST_FAILED: {error}"))?;
        capabilities.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| compare_semver(&right.version, &left.version))
        });
        Ok(capabilities)
    }

    /// 将已验证的项目能力提升到跨项目共享层；共享记录不复制原始会话或自由代码。
    pub fn promote_user_capability(
        &self,
        project_id: &str,
        request: &CapabilityPromotionRequest,
    ) -> Result<CapabilityResolution, String> {
        if !matches!(request.target_scope.as_str(), "personal" | "team") {
            return Err(
                "CAPABILITY_PROMOTION_SCOPE_INVALID: target must be personal or team".to_string(),
            );
        }
        let mut capability = self.get_project_capability(
            project_id,
            &request.capability_id,
            Some(&request.version),
        )?;
        if capability.scope != "project" || capability.status != "active" {
            return Err(
                "CAPABILITY_PROMOTION_SOURCE_INVALID: only an active project capability can be promoted"
                    .to_string(),
            );
        }
        self.verify_capability_runtime_contract(&capability)?;
        capability.scope = request.target_scope.clone();
        capability.updated_at = now_millis();
        let existing = self.shared_capability(
            &request.target_scope,
            &request.capability_id,
            Some(&request.version),
        )?;
        if let Some((_, existing)) = existing {
            if capability_material_hash(&existing)? != capability_material_hash(&capability)? {
                return Err(format!(
                    "CAPABILITY_PROMOTION_CONFLICT: {} already contains another {}@{}",
                    request.target_scope, request.capability_id, request.version
                ));
            }
        }
        let mut connection = self.registry()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("CAPABILITY_PROMOTION_FAILED: {error}"))?;
        transaction
            .execute(
                "UPDATE shared_user_capabilities SET status='disabled',updated_at=?3 WHERE scope=?1 AND id=?2 AND version<>?4 AND status='active'",
                params![request.target_scope,request.capability_id,capability.updated_at,request.version],
            )
            .map_err(|error| format!("CAPABILITY_PROMOTION_FAILED: {error}"))?;
        transaction
            .execute(
                "INSERT INTO shared_user_capabilities(scope,id,version,source_project_id,system_id,name,description,parameter_schema,steps,read_systems,write_systems,status,source_task_id,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'active',?12,?13,?14)
                 ON CONFLICT(scope,id,version) DO UPDATE SET status='active',updated_at=excluded.updated_at",
                params![
                    capability.scope,
                    capability.id,
                    capability.version,
                    project_id,
                    capability.system_id,
                    capability.name,
                    capability.description,
                    capability.parameter_schema.to_string(),
                    capability.steps.to_string(),
                    json_array(&capability.read_systems),
                    json_array(&capability.write_systems),
                    capability.source_task_id,
                    capability.created_at,
                    capability.updated_at,
                ],
            )
            .map_err(|error| format!("CAPABILITY_PROMOTION_FAILED: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("CAPABILITY_PROMOTION_FAILED: {error}"))?;
        Ok(CapabilityResolution {
            capability,
            resolved_scope: request.target_scope.clone(),
            source_project_id: project_id.to_string(),
            shadowed_scopes: Vec::new(),
        })
    }

    /// 按 project > personal > team 解析同名能力，每层内部使用最高活动 SemVer。
    pub fn resolve_user_capabilities(
        &self,
        project_id: &str,
        system_id: Option<&str>,
    ) -> Result<Vec<CapabilityResolution>, String> {
        let mut candidates: BTreeMap<String, Vec<CapabilityResolution>> = BTreeMap::new();
        for capability in self.list_user_capabilities(project_id, system_id)? {
            if capability.scope == "project" && capability.status == "active" {
                candidates
                    .entry(capability.id.clone())
                    .or_default()
                    .push(CapabilityResolution {
                        capability,
                        resolved_scope: "project".to_string(),
                        source_project_id: project_id.to_string(),
                        shadowed_scopes: Vec::new(),
                    });
            }
        }
        for (source_project_id, capability) in self.list_shared_capabilities(system_id)? {
            if capability.status == "active" {
                candidates
                    .entry(capability.id.clone())
                    .or_default()
                    .push(CapabilityResolution {
                        resolved_scope: capability.scope.clone(),
                        source_project_id,
                        capability,
                        shadowed_scopes: Vec::new(),
                    });
            }
        }
        let mut resolved = Vec::new();
        for (_, mut versions) in candidates {
            versions.sort_by(|left, right| {
                capability_scope_priority(&right.resolved_scope)
                    .cmp(&capability_scope_priority(&left.resolved_scope))
                    .then_with(|| {
                        compare_semver(&right.capability.version, &left.capability.version)
                    })
            });
            let winner_scope = versions[0].resolved_scope.clone();
            let mut same_scope = versions
                .iter()
                .filter(|candidate| candidate.resolved_scope == winner_scope)
                .cloned()
                .collect::<Vec<_>>();
            same_scope.sort_by(|left, right| {
                compare_semver(&right.capability.version, &left.capability.version)
            });
            let mut winner = same_scope.remove(0);
            winner.shadowed_scopes = versions
                .iter()
                .filter(|candidate| candidate.resolved_scope != winner.resolved_scope)
                .map(|candidate| candidate.resolved_scope.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            resolved.push(winner);
        }
        Ok(resolved)
    }

    /// 返回项目、个人和团队各层的完整版本记录，供 Studio 展示禁用与回退目标。
    pub fn list_user_capability_versions(
        &self,
        project_id: &str,
        system_id: Option<&str>,
    ) -> Result<Vec<CapabilityResolution>, String> {
        let mut versions = self
            .list_user_capabilities(project_id, system_id)?
            .into_iter()
            .map(|capability| CapabilityResolution {
                capability,
                resolved_scope: "project".to_string(),
                source_project_id: project_id.to_string(),
                shadowed_scopes: Vec::new(),
            })
            .collect::<Vec<_>>();
        versions.extend(self.list_shared_capabilities(system_id)?.into_iter().map(
            |(source_project_id, capability)| CapabilityResolution {
                resolved_scope: capability.scope.clone(),
                source_project_id,
                capability,
                shadowed_scopes: Vec::new(),
            },
        ));
        versions.sort_by(|left, right| {
            left.capability
                .id
                .cmp(&right.capability.id)
                .then_with(|| left.resolved_scope.cmp(&right.resolved_scope))
                .then_with(|| compare_semver(&right.capability.version, &left.capability.version))
        });
        Ok(versions)
    }

    /// 在同一作用域内禁用当前活动版本并恢复一个已禁用旧版本，两个状态必须原子切换。
    pub fn rollback_user_capability(
        &self,
        project_id: &str,
        request: &CapabilityRollbackRequest,
    ) -> Result<UserCapability, String> {
        if request.from_version == request.to_version {
            return Err("CAPABILITY_ROLLBACK_VERSION_INVALID: versions must differ".to_string());
        }
        if request.scope == "project" {
            let current = self.get_project_capability(
                project_id,
                &request.capability_id,
                Some(&request.from_version),
            )?;
            let target = self.get_project_capability(
                project_id,
                &request.capability_id,
                Some(&request.to_version),
            )?;
            if current.status != "active" || target.status != "disabled" {
                return Err(
                    "CAPABILITY_ROLLBACK_STATE_INVALID: source must be active and target disabled"
                        .to_string(),
                );
            }
            if target.system_id == "__global__" {
                self.verify_global_capability_activation(project_id, &target)?;
            } else {
                self.verify_capability_activation(project_id, &target)?;
            }
            let mut connection = self.project_connection(project_id)?;
            let transaction = connection
                .transaction()
                .map_err(|error| format!("CAPABILITY_ROLLBACK_FAILED: {error}"))?;
            let now = now_millis();
            let disabled = transaction
                .execute(
                    "UPDATE user_capabilities SET status='disabled',updated_at=?3 WHERE id=?1 AND version=?2 AND status='active'",
                    params![request.capability_id, request.from_version, now],
                )
                .map_err(|error| format!("CAPABILITY_ROLLBACK_FAILED: {error}"))?;
            transaction
                .execute(
                    "UPDATE user_capabilities SET status='disabled',updated_at=?2 WHERE id=?1 AND version<>?3 AND status='active'",
                    params![request.capability_id,now,request.to_version],
                )
                .map_err(|error| format!("CAPABILITY_ROLLBACK_FAILED: {error}"))?;
            let restored = transaction
                .execute(
                    "UPDATE user_capabilities SET status='active',updated_at=?3 WHERE id=?1 AND version=?2 AND status='disabled'",
                    params![request.capability_id, request.to_version, now],
                )
                .map_err(|error| format!("CAPABILITY_ROLLBACK_FAILED: {error}"))?;
            if disabled != 1 || restored != 1 {
                return Err("CAPABILITY_ROLLBACK_CONFLICT: capability versions changed".to_string());
            }
            transaction
                .commit()
                .map_err(|error| format!("CAPABILITY_ROLLBACK_FAILED: {error}"))?;
            return self.get_project_capability(
                project_id,
                &request.capability_id,
                Some(&request.to_version),
            );
        }
        if !matches!(request.scope.as_str(), "personal" | "team") {
            return Err("CAPABILITY_ROLLBACK_SCOPE_INVALID: invalid scope".to_string());
        }
        let (_, current) = self
            .shared_capability(
                &request.scope,
                &request.capability_id,
                Some(&request.from_version),
            )?
            .ok_or_else(|| {
                format!(
                    "CAPABILITY_NOT_FOUND: {}:{}@{}",
                    request.scope, request.capability_id, request.from_version
                )
            })?;
        let (_, target) = self
            .shared_capability(
                &request.scope,
                &request.capability_id,
                Some(&request.to_version),
            )?
            .ok_or_else(|| {
                format!(
                    "CAPABILITY_NOT_FOUND: {}:{}@{}",
                    request.scope, request.capability_id, request.to_version
                )
            })?;
        if current.status != "active" || target.status != "disabled" {
            return Err(
                "CAPABILITY_ROLLBACK_STATE_INVALID: source must be active and target disabled"
                    .to_string(),
            );
        }
        self.verify_capability_runtime_contract(&target)?;
        let mut connection = self.registry()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("CAPABILITY_ROLLBACK_FAILED: {error}"))?;
        let now = now_millis();
        let disabled = transaction
            .execute(
                "UPDATE shared_user_capabilities SET status='disabled',updated_at=?4 WHERE scope=?1 AND id=?2 AND version=?3 AND status='active'",
                params![request.scope,request.capability_id,request.from_version,now],
            )
            .map_err(|error| format!("CAPABILITY_ROLLBACK_FAILED: {error}"))?;
        transaction
            .execute(
                "UPDATE shared_user_capabilities SET status='disabled',updated_at=?3 WHERE scope=?1 AND id=?2 AND version<>?4 AND status='active'",
                params![request.scope,request.capability_id,now,request.to_version],
            )
            .map_err(|error| format!("CAPABILITY_ROLLBACK_FAILED: {error}"))?;
        let restored = transaction
            .execute(
                "UPDATE shared_user_capabilities SET status='active',updated_at=?4 WHERE scope=?1 AND id=?2 AND version=?3 AND status='disabled'",
                params![request.scope,request.capability_id,request.to_version,now],
            )
            .map_err(|error| format!("CAPABILITY_ROLLBACK_FAILED: {error}"))?;
        if disabled != 1 || restored != 1 {
            return Err("CAPABILITY_ROLLBACK_CONFLICT: capability versions changed".to_string());
        }
        transaction
            .commit()
            .map_err(|error| format!("CAPABILITY_ROLLBACK_FAILED: {error}"))?;
        self.shared_capability(
            &request.scope,
            &request.capability_id,
            Some(&request.to_version),
        )?
        .map(|(_, capability)| capability)
        .ok_or_else(|| "CAPABILITY_ROLLBACK_FAILED: restored version disappeared".to_string())
    }

    pub fn set_shared_capability_status(
        &self,
        scope: &str,
        capability_id: &str,
        version: &str,
        status: &str,
    ) -> Result<UserCapability, String> {
        if !matches!(scope, "personal" | "team")
            || !matches!(status, "active" | "disabled" | "deprecated")
        {
            return Err("CAPABILITY_SHARED_STATUS_INVALID: invalid scope or status".to_string());
        }
        let (_, capability) = self
            .shared_capability(scope, capability_id, Some(version))?
            .ok_or_else(|| format!("CAPABILITY_NOT_FOUND: {scope}:{capability_id}@{version}"))?;
        if status == "active" {
            self.verify_capability_runtime_contract(&capability)?;
        }
        let mut connection = self.registry()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("CAPABILITY_SHARED_STATUS_FAILED: {error}"))?;
        let now = now_millis();
        if status == "active" {
            transaction
                .execute(
                    "UPDATE shared_user_capabilities SET status='disabled',updated_at=?3 WHERE scope=?1 AND id=?2 AND version<>?4 AND status='active'",
                    params![scope,capability_id,now,version],
                )
                .map_err(|error| format!("CAPABILITY_SHARED_STATUS_FAILED: {error}"))?;
        }
        let changed = transaction
            .execute(
                "UPDATE shared_user_capabilities SET status=?4,updated_at=?5 WHERE scope=?1 AND id=?2 AND version=?3",
                params![scope,capability_id,version,status,now],
            )
            .map_err(|error| format!("CAPABILITY_SHARED_STATUS_FAILED: {error}"))?;
        if changed != 1 {
            return Err(format!(
                "CAPABILITY_NOT_FOUND: {scope}:{capability_id}@{version}"
            ));
        }
        transaction
            .commit()
            .map_err(|error| format!("CAPABILITY_SHARED_STATUS_FAILED: {error}"))?;
        self.shared_capability(scope, capability_id, Some(version))?
            .map(|(_, capability)| capability)
            .ok_or_else(|| format!("CAPABILITY_NOT_FOUND: {scope}:{capability_id}@{version}"))
    }

    pub fn get_user_capability(
        &self,
        project_id: &str,
        capability_id: &str,
        version: Option<&str>,
    ) -> Result<UserCapability, String> {
        self.resolve_user_capability(project_id, capability_id, version)
            .map(|resolution| resolution.capability)
    }

    /// 对单个能力使用与列表、描述和调用相同的活动作用域优先级。
    pub fn resolve_user_capability(
        &self,
        project_id: &str,
        capability_id: &str,
        version: Option<&str>,
    ) -> Result<CapabilityResolution, String> {
        let mut candidates = self
            .list_user_capabilities(project_id, None)?
            .into_iter()
            .filter(|capability| {
                capability.scope == "project"
                    && capability.status == "active"
                    && capability.id == capability_id
                    && version.is_none_or(|expected| capability.version == expected)
            })
            .map(|capability| CapabilityResolution {
                capability,
                resolved_scope: "project".to_string(),
                source_project_id: project_id.to_string(),
                shadowed_scopes: Vec::new(),
            })
            .collect::<Vec<_>>();
        candidates.extend(
            self.list_shared_capabilities(None)?
                .into_iter()
                .filter(|(_, capability)| {
                    capability.status == "active"
                        && capability.id == capability_id
                        && version.is_none_or(|expected| capability.version == expected)
                })
                .map(|(source_project_id, capability)| CapabilityResolution {
                    resolved_scope: capability.scope.clone(),
                    source_project_id,
                    capability,
                    shadowed_scopes: Vec::new(),
                }),
        );
        select_capability_resolution(candidates).ok_or_else(|| match version {
            Some(version) => format!("CAPABILITY_NOT_FOUND: {capability_id}@{version}"),
            None => format!("CAPABILITY_NOT_FOUND: {capability_id}"),
        })
    }

    fn get_project_capability(
        &self,
        project_id: &str,
        capability_id: &str,
        version: Option<&str>,
    ) -> Result<UserCapability, String> {
        let connection = self.project_connection(project_id)?;
        if let Some(version) = version {
            return connection
                .query_row(
                    "SELECT id,version,system_id,scope,name,description,parameter_schema,steps,read_systems,write_systems,status,source_task_id,created_at,updated_at FROM user_capabilities WHERE id=?1 AND version=?2",
                    params![capability_id, version],
                    row_to_capability,
                )
                .optional()
                .map_err(|error| format!("CAPABILITY_READ_FAILED: {error}"))?
                .ok_or_else(|| format!("CAPABILITY_NOT_FOUND: {capability_id}@{version}"));
        }
        let mut statement = connection
            .prepare(
                "SELECT id,version,system_id,scope,name,description,parameter_schema,steps,read_systems,write_systems,status,source_task_id,created_at,updated_at FROM user_capabilities WHERE id=?1",
            )
            .map_err(|error| format!("CAPABILITY_READ_FAILED: {error}"))?;
        let capabilities = statement
            .query_map([capability_id], row_to_capability)
            .map_err(|error| format!("CAPABILITY_READ_FAILED: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("CAPABILITY_READ_FAILED: {error}"))?;
        capabilities
            .into_iter()
            .max_by(|left, right| compare_semver(&left.version, &right.version))
            .ok_or_else(|| format!("CAPABILITY_NOT_FOUND: {capability_id}"))
    }

    fn list_shared_capabilities(
        &self,
        system_id: Option<&str>,
    ) -> Result<Vec<(String, UserCapability)>, String> {
        let connection = self.registry()?;
        let mut statement = connection
            .prepare(
                "SELECT source_project_id,id,version,system_id,scope,name,description,parameter_schema,steps,read_systems,write_systems,status,source_task_id,created_at,updated_at
                 FROM shared_user_capabilities WHERE (?1 IS NULL OR system_id=?1)",
            )
            .map_err(|error| format!("CAPABILITY_SHARED_LIST_FAILED: {error}"))?;
        let rows = statement
            .query_map([system_id], |row| {
                let parameter_schema: String = row.get(7)?;
                let steps: String = row.get(8)?;
                let read_systems: String = row.get(9)?;
                let write_systems: String = row.get(10)?;
                Ok((
                    row.get(0)?,
                    UserCapability {
                        id: row.get(1)?,
                        version: row.get(2)?,
                        system_id: row.get(3)?,
                        scope: row.get(4)?,
                        name: row.get(5)?,
                        description: row.get(6)?,
                        parameter_schema: serde_json::from_str(&parameter_schema)
                            .unwrap_or_default(),
                        steps: serde_json::from_str(&steps).unwrap_or_default(),
                        read_systems: parse_array(&read_systems),
                        write_systems: parse_array(&write_systems),
                        status: row.get(11)?,
                        source_task_id: row.get(12)?,
                        created_at: row.get(13)?,
                        updated_at: row.get(14)?,
                    },
                ))
            })
            .map_err(|error| format!("CAPABILITY_SHARED_LIST_FAILED: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("CAPABILITY_SHARED_LIST_FAILED: {error}"))
    }

    fn shared_capability(
        &self,
        scope: &str,
        capability_id: &str,
        version: Option<&str>,
    ) -> Result<Option<(String, UserCapability)>, String> {
        let mut candidates = self
            .list_shared_capabilities(None)?
            .into_iter()
            .filter(|(_, capability)| {
                capability.scope == scope
                    && capability.id == capability_id
                    && version.is_none_or(|expected| capability.version == expected)
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|(_, left), (_, right)| compare_semver(&right.version, &left.version));
        Ok(candidates.into_iter().next())
    }

    fn list_shared_memories(
        &self,
        system_id: Option<&str>,
    ) -> Result<Vec<SharedMemoryRow>, String> {
        let connection = self.registry()?;
        let mut statement = connection
            .prepare(
                "SELECT source_project_id,id,system_id,scope,kind,summary,body,status,source_task_id,plugin_version,created_at,updated_at
                 FROM shared_domain_memories WHERE (?1 IS NULL OR system_id=?1)",
            )
            .map_err(|error| format!("MEMORY_SHARED_LIST_FAILED: {error}"))?;
        let rows = statement
            .query_map([system_id], |row| {
                let body: String = row.get(6)?;
                Ok(SharedMemoryRow {
                    source_project_id: row.get(0)?,
                    memory: DomainMemory {
                        id: row.get(1)?,
                        system_id: row.get(2)?,
                        scope: row.get(3)?,
                        kind: row.get(4)?,
                        summary: row.get(5)?,
                        body: serde_json::from_str(&body).unwrap_or_default(),
                        status: row.get(7)?,
                        source_task_id: row.get(8)?,
                        plugin_version: row.get(9)?,
                        created_at: row.get(10)?,
                        updated_at: row.get(11)?,
                    },
                })
            })
            .map_err(|error| format!("MEMORY_SHARED_LIST_FAILED: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("MEMORY_SHARED_LIST_FAILED: {error}"))
    }

    fn verify_capability_runtime_contract(
        &self,
        capability: &UserCapability,
    ) -> Result<(), String> {
        validate_capability(capability)?;
        for step in capability.steps.as_array().into_iter().flatten() {
            let system_id = step
                .get("systemId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&capability.system_id);
            let version = step
                .get("pluginVersion")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "CAPABILITY_STEP_VERSION_REQUIRED: missing version".to_string())?;
            let operation_id = step
                .get("operation")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "CAPABILITY_STEP_INVALID: missing operation".to_string())?;
            let manifest = self.runtime_manifest_at_version(system_id, Some(version))?;
            let operation = manifest
                .operations
                .iter()
                .find(|operation| operation.id == operation_id)
                .ok_or_else(|| format!("CAPABILITY_OPERATION_NOT_REGISTERED: {operation_id}"))?;
            let current_schema_hash = hash_json(&operation.parameter_schema)?;
            if step
                .get("parameterSchemaHash")
                .and_then(serde_json::Value::as_str)
                != Some(current_schema_hash.as_str())
            {
                return Err(format!(
                    "CAPABILITY_OPERATION_SCHEMA_MISMATCH: {operation_id}"
                ));
            }
        }
        Ok(())
    }

    pub fn set_user_capability_status(
        &self,
        project_id: &str,
        capability_id: &str,
        version: &str,
        status: &str,
    ) -> Result<UserCapability, String> {
        if !matches!(status, "draft" | "active" | "disabled" | "deprecated") {
            return Err("CAPABILITY_STATUS_INVALID: unsupported status".to_string());
        }
        if status == "active" {
            let capability =
                self.get_project_capability(project_id, capability_id, Some(version))?;
            if capability.system_id == "__global__" {
                self.verify_global_capability_activation(project_id, &capability)?;
            } else {
                self.verify_capability_activation(project_id, &capability)?;
            }
        }
        let mut connection = self.project_connection(project_id)?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("CAPABILITY_STATUS_FAILED: {error}"))?;
        let now = now_millis();
        if status == "active" {
            transaction
                .execute(
                    "UPDATE user_capabilities SET status='disabled',updated_at=?2 WHERE id=?1 AND version<>?3 AND status='active'",
                    params![capability_id,now,version],
                )
                .map_err(|error| format!("CAPABILITY_STATUS_FAILED: {error}"))?;
        }
        let changed = transaction
            .execute(
                "UPDATE user_capabilities SET status=?3,updated_at=?4 WHERE id=?1 AND version=?2",
                params![capability_id, version, status, now],
            )
            .map_err(|error| format!("CAPABILITY_STATUS_FAILED: {error}"))?;
        if changed == 0 {
            return Err(format!("CAPABILITY_NOT_FOUND: {capability_id}@{version}"));
        }
        transaction
            .commit()
            .map_err(|error| format!("CAPABILITY_STATUS_FAILED: {error}"))?;
        self.get_project_capability(project_id, capability_id, Some(version))
    }

    pub fn validate_user_capability_for_draft(
        &self,
        project_id: &str,
        draft_id: &str,
        capability_id: &str,
    ) -> Result<UserCapability, String> {
        self.validate_user_capability_version_for_draft(project_id, draft_id, capability_id, None)
    }

    pub fn validate_user_capability_version_for_draft(
        &self,
        project_id: &str,
        draft_id: &str,
        capability_id: &str,
        version: Option<&str>,
    ) -> Result<UserCapability, String> {
        let capability = self.get_user_capability(project_id, capability_id, version)?;
        if capability.status != "active" {
            return Err(format!("CAPABILITY_NOT_ACTIVE: {capability_id}"));
        }
        validate_capability(&capability)?;
        let (draft_system, draft_version) = self
            .project_connection(project_id)?
            .query_row(
                "SELECT system_id,plugin_version FROM draft_domains WHERE draft_id=?1 AND legacy=0",
                [draft_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("DRAFT_DOMAIN_READ_FAILED: {error}"))?
            .and_then(|(system_id, plugin_version)| system_id.zip(plugin_version))
            .ok_or_else(|| {
                "DRAFT_DOMAIN_REQUIRED: capability invocation requires a scoped draft".to_string()
            })?;
        if capability.system_id != draft_system
            || !capability
                .write_systems
                .iter()
                .any(|system| system == &draft_system)
        {
            return Err(format!("CAPABILITY_DRAFT_SCOPE_DENIED: {capability_id}"));
        }
        for step in capability.steps.as_array().into_iter().flatten() {
            if step
                .get("pluginVersion")
                .and_then(serde_json::Value::as_str)
                != Some(draft_version.as_str())
            {
                return Err(format!(
                    "CAPABILITY_DOMAIN_VERSION_INCOMPATIBLE: {capability_id}@{} requires another domain pack version",
                    capability.version
                ));
            }
            let operation_id = step
                .get("operation")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "CAPABILITY_STEP_INVALID: operation is required".to_string())?;
            let manifest = self.runtime_manifest_at_version(&draft_system, Some(&draft_version))?;
            let operation = manifest
                .operations
                .iter()
                .find(|operation| operation.id == operation_id)
                .ok_or_else(|| format!("CAPABILITY_OPERATION_NOT_REGISTERED: {operation_id}"))?;
            let schema_hash = hash_json(&operation.parameter_schema)?;
            if step
                .get("parameterSchemaHash")
                .and_then(serde_json::Value::as_str)
                != Some(schema_hash.as_str())
            {
                return Err(format!(
                    "CAPABILITY_OPERATION_SCHEMA_MISMATCH: {operation_id}"
                ));
            }
        }
        Ok(capability)
    }

    /// 全局工作流调用必须绑定一个覆盖全部写系统的开放组合 Draft。
    pub fn validate_global_capability_for_composite(
        &self,
        project_id: &str,
        composite_id: &str,
        capability_id: &str,
        version: Option<&str>,
    ) -> Result<UserCapability, String> {
        let capability = self.get_user_capability(project_id, capability_id, version)?;
        if capability.system_id != "__global__" || capability.status != "active" {
            return Err(format!("GLOBAL_CAPABILITY_NOT_ACTIVE: {capability_id}"));
        }
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT dd.system_id,dd.plugin_version FROM draft_domains dd
                 JOIN drafts d ON d.id=dd.draft_id
                 WHERE dd.composite_id=?1 AND dd.legacy=0 AND d.status='open'",
            )
            .map_err(|error| format!("GLOBAL_CAPABILITY_COMPOSITE_READ_FAILED: {error}"))?;
        let bindings = statement
            .query_map([composite_id], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            })
            .map_err(|error| format!("GLOBAL_CAPABILITY_COMPOSITE_READ_FAILED: {error}"))?
            .filter_map(|row| {
                row.ok()
                    .and_then(|(system, version)| Some((system?, version?)))
            })
            .collect::<BTreeMap<_, _>>();
        let bound_systems = bindings.keys().cloned().collect::<BTreeSet<_>>();
        let expected_systems = capability
            .write_systems
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if bound_systems != expected_systems {
            return Err(
                "GLOBAL_CAPABILITY_COMPOSITE_SCOPE_MISMATCH: open Draft systems must exactly match writeSystems"
                    .to_string(),
            );
        }
        for step in capability.steps.as_array().into_iter().flatten() {
            let system_id = step
                .get("systemId")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    "GLOBAL_CAPABILITY_STEP_SYSTEM_REQUIRED: systemId missing".to_string()
                })?;
            let plugin_version = step
                .get("pluginVersion")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "CAPABILITY_STEP_VERSION_REQUIRED: version missing".to_string())?;
            if bindings.get(system_id).map(String::as_str) != Some(plugin_version) {
                return Err(format!(
                    "GLOBAL_CAPABILITY_DOMAIN_VERSION_INCOMPATIBLE: {system_id}@{plugin_version}"
                ));
            }
            let operation_id = step
                .get("operation")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "CAPABILITY_STEP_INVALID: operation missing".to_string())?;
            let manifest = self.runtime_manifest_at_version(system_id, Some(plugin_version))?;
            let operation = manifest
                .operations
                .iter()
                .find(|operation| operation.id == operation_id)
                .ok_or_else(|| format!("CAPABILITY_OPERATION_NOT_REGISTERED: {operation_id}"))?;
            let schema_hash = hash_json(&operation.parameter_schema)?;
            if step
                .get("parameterSchemaHash")
                .and_then(serde_json::Value::as_str)
                != Some(schema_hash.as_str())
            {
                return Err(format!(
                    "CAPABILITY_OPERATION_SCHEMA_MISMATCH: {operation_id}"
                ));
            }
        }
        Ok(capability)
    }

    fn verify_capability_activation(
        &self,
        project_id: &str,
        capability: &UserCapability,
    ) -> Result<(), String> {
        if !matches!(capability.status.as_str(), "draft" | "disabled") {
            return Err(
                "CAPABILITY_ACTIVATION_STATE_INVALID: only a compiled or disabled version can be activated"
                    .to_string(),
            );
        }
        let steps = capability
            .steps
            .as_array()
            .ok_or_else(|| "CAPABILITY_STEPS_INVALID: steps must be an array".to_string())?;
        let first = steps
            .first()
            .ok_or_else(|| "CAPABILITY_STEPS_INVALID: no compiled steps".to_string())?;
        let receipt_id = first
            .get("sourceReceiptId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                "CAPABILITY_ACTIVATION_EVIDENCE_REQUIRED: source receipt is missing".to_string()
            })?;
        let expected_replay_hash = first
            .get("replayHash")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                "CAPABILITY_ACTIVATION_EVIDENCE_REQUIRED: replay hash is missing".to_string()
            })?;
        if steps.iter().any(|step| {
            step.get("sourceReceiptId")
                .and_then(serde_json::Value::as_str)
                != Some(receipt_id)
                || step.get("replayHash").and_then(serde_json::Value::as_str)
                    != Some(expected_replay_hash)
        }) {
            return Err("CAPABILITY_ACTIVATION_EVIDENCE_MISMATCH: compiled steps do not share one replay proof".to_string());
        }
        let receipt = self.get_task_receipt(project_id, receipt_id)?;
        if receipt.task_id != capability.source_task_id {
            return Err("CAPABILITY_ACTIVATION_RECEIPT_MISMATCH: source task differs".to_string());
        }
        let draft_id = receipt
            .draft_id
            .as_deref()
            .ok_or_else(|| "CAPABILITY_RECEIPT_DRAFT_REQUIRED: receipt has no Draft".to_string())?;
        let manifest = self.draft_domain_manifest(project_id, draft_id)?;
        let evidence = self.list_draft_operation_evidence(project_id, draft_id)?;
        let draft = self.get_draft(project_id, draft_id)?;
        verify_operation_evidence(&manifest, &draft, &evidence)?;
        if evidence.len() != steps.len() {
            return Err(
                "CAPABILITY_ACTIVATION_EVIDENCE_MISMATCH: operation count changed".to_string(),
            );
        }
        for (item, step) in evidence.iter().zip(steps) {
            let evidence_hash = hash_json(item)?;
            if step.get("operation").and_then(serde_json::Value::as_str)
                != Some(item.operation_id.as_str())
                || step.get("evidenceHash").and_then(serde_json::Value::as_str)
                    != Some(evidence_hash.as_str())
            {
                return Err(
                    "CAPABILITY_ACTIVATION_EVIDENCE_MISMATCH: operation evidence changed"
                        .to_string(),
                );
            }
        }
        let actual_replay_hash =
            self.replay_capability_source(project_id, &receipt, &manifest, &evidence)?;
        if actual_replay_hash != expected_replay_hash {
            return Err(
                "CAPABILITY_REPLAY_DIFF_MISMATCH: activation replay hash changed".to_string(),
            );
        }
        Ok(())
    }

    fn verify_global_capability_activation(
        &self,
        project_id: &str,
        capability: &UserCapability,
    ) -> Result<(), String> {
        if !matches!(capability.status.as_str(), "draft" | "disabled") {
            return Err(
                "CAPABILITY_ACTIVATION_STATE_INVALID: global workflow is not activatable"
                    .to_string(),
            );
        }
        self.verify_capability_runtime_contract(capability)?;
        let steps = capability
            .steps
            .as_array()
            .ok_or_else(|| "CAPABILITY_STEPS_INVALID: steps must be an array".to_string())?;
        let receipt_ids = steps
            .iter()
            .filter_map(|step| {
                step.get("sourceReceiptId")
                    .and_then(serde_json::Value::as_str)
            })
            .collect::<BTreeSet<_>>();
        if receipt_ids.len() < 2 {
            return Err(
                "GLOBAL_CAPABILITY_RECEIPTS_REQUIRED: workflow evidence is incomplete".to_string(),
            );
        }
        let mut snapshot_id: Option<String> = None;
        let mut systems = BTreeSet::new();
        for receipt_id in receipt_ids {
            let receipt = self.get_task_receipt(project_id, receipt_id)?;
            let current_snapshot = receipt
                .evidence
                .get("snapshotId")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    "GLOBAL_CAPABILITY_SNAPSHOT_REQUIRED: missing snapshot".to_string()
                })?;
            if snapshot_id
                .as_deref()
                .is_some_and(|expected| expected != current_snapshot)
            {
                return Err("GLOBAL_CAPABILITY_NOT_ATOMIC: snapshots differ".to_string());
            }
            snapshot_id = Some(current_snapshot.to_string());
            systems.insert(receipt.system_id.clone());
            let draft_id = receipt
                .draft_id
                .as_deref()
                .ok_or_else(|| "GLOBAL_CAPABILITY_DRAFT_REQUIRED: missing Draft".to_string())?;
            let manifest = self.draft_domain_manifest(project_id, draft_id)?;
            let evidence = self.list_draft_operation_evidence(project_id, draft_id)?;
            let draft = self.get_draft(project_id, draft_id)?;
            verify_operation_evidence(&manifest, &draft, &evidence)?;
            let replay_hash =
                self.replay_capability_source(project_id, &receipt, &manifest, &evidence)?;
            let compiled = steps
                .iter()
                .filter(|step| {
                    step.get("sourceReceiptId")
                        .and_then(serde_json::Value::as_str)
                        == Some(receipt_id)
                })
                .collect::<Vec<_>>();
            if compiled.len() != evidence.len()
                || evidence.iter().zip(compiled).any(|(item, step)| {
                    step.get("operation").and_then(serde_json::Value::as_str)
                        != Some(item.operation_id.as_str())
                        || step.get("replayHash").and_then(serde_json::Value::as_str)
                            != Some(replay_hash.as_str())
                })
            {
                return Err(
                    "GLOBAL_CAPABILITY_ACTIVATION_EVIDENCE_MISMATCH: workflow changed".to_string(),
                );
            }
        }
        if systems.len() < 2 {
            return Err(
                "GLOBAL_CAPABILITY_MULTI_SYSTEM_REQUIRED: workflow is not cross-system".to_string(),
            );
        }
        Ok(())
    }

    /// 升级前捕获所有受影响项目与共享层的能力、Memory；失败时可精确恢复。
    pub fn snapshot_domain_governance(
        &self,
        system_id: &str,
    ) -> Result<GovernanceSnapshot, String> {
        self.ensure_known_system(system_id)?;
        let mut project_rows = Vec::new();
        for project in self.list_projects()? {
            let capabilities = self
                .list_user_capabilities(&project.id, None)?
                .into_iter()
                .filter(|capability| capability_touches_system(capability, system_id))
                .collect();
            let memories = self.list_domain_memories(&project.id, system_id, false)?;
            project_rows.push(ProjectGovernanceSnapshot {
                project_id: project.id,
                capabilities,
                memories,
            });
        }
        let shared_capabilities = self
            .list_shared_capabilities(None)?
            .into_iter()
            .filter(|(_, capability)| capability_touches_system(capability, system_id))
            .map(|(source_project_id, capability)| SharedCapabilityRow {
                source_project_id,
                capability,
            })
            .collect();
        let shared_memories = self.list_shared_memories(Some(system_id))?;
        Ok(GovernanceSnapshot {
            system_id: system_id.to_string(),
            project_rows,
            shared_capabilities,
            shared_memories,
        })
    }

    /// 恢复以逐库事务执行；任一库失败都明确报错，绝不把部分恢复当成功。
    pub fn restore_domain_governance_snapshot(
        &self,
        snapshot: &GovernanceSnapshot,
    ) -> Result<(), String> {
        for project in &snapshot.project_rows {
            let current = self
                .list_user_capabilities(&project.project_id, None)?
                .into_iter()
                .filter(|capability| capability_touches_system(capability, &snapshot.system_id))
                .collect::<Vec<_>>();
            let mut connection = self.project_connection(&project.project_id)?;
            let transaction = connection
                .transaction()
                .map_err(|error| format!("GOVERNANCE_RESTORE_FAILED: {error}"))?;
            for capability in current {
                transaction
                    .execute(
                        "DELETE FROM user_capabilities WHERE id=?1 AND version=?2",
                        params![capability.id, capability.version],
                    )
                    .map_err(|error| format!("GOVERNANCE_RESTORE_FAILED: {error}"))?;
            }
            transaction
                .execute(
                    "DELETE FROM domain_memories WHERE system_id=?1",
                    [&snapshot.system_id],
                )
                .map_err(|error| format!("GOVERNANCE_RESTORE_FAILED: {error}"))?;
            for capability in &project.capabilities {
                insert_project_capability(&transaction, capability)?;
            }
            for memory in &project.memories {
                insert_project_memory(&transaction, memory)?;
            }
            transaction
                .commit()
                .map_err(|error| format!("GOVERNANCE_RESTORE_FAILED: {error}"))?;
        }
        let mut registry = self.registry()?;
        let transaction = registry
            .transaction()
            .map_err(|error| format!("GOVERNANCE_RESTORE_FAILED: {error}"))?;
        let current = self.list_shared_capabilities(None)?;
        for (_, capability) in current
            .into_iter()
            .filter(|(_, capability)| capability_touches_system(capability, &snapshot.system_id))
        {
            transaction
                .execute(
                    "DELETE FROM shared_user_capabilities WHERE scope=?1 AND id=?2 AND version=?3",
                    params![capability.scope, capability.id, capability.version],
                )
                .map_err(|error| format!("GOVERNANCE_RESTORE_FAILED: {error}"))?;
        }
        transaction
            .execute(
                "DELETE FROM shared_domain_memories WHERE system_id=?1",
                [&snapshot.system_id],
            )
            .map_err(|error| format!("GOVERNANCE_RESTORE_FAILED: {error}"))?;
        for row in &snapshot.shared_capabilities {
            insert_shared_capability(&transaction, &row.source_project_id, &row.capability)?;
        }
        for row in &snapshot.shared_memories {
            insert_shared_memory(&transaction, row)?;
        }
        transaction
            .commit()
            .map_err(|error| format!("GOVERNANCE_RESTORE_FAILED: {error}"))
    }

    /// 在候选已临时激活后执行通用迁移预演；冲突时不改任何能力或 Memory。
    pub fn migrate_domain_governance(
        &self,
        system_id: &str,
        from_version: &str,
        to_version: &str,
    ) -> Result<GovernanceMigrationReport, String> {
        let source = self.runtime_manifest_at_version(system_id, Some(from_version))?;
        let target = self.runtime_manifest_at_version(system_id, Some(to_version))?;
        let snapshot = self.snapshot_domain_governance(system_id)?;
        let created_at = now_millis();
        let mut report = GovernanceMigrationReport {
            id: format!(
                "migration-{}",
                &hash(&format!(
                    "{system_id}:{from_version}:{to_version}:{created_at}"
                ))[..20]
            ),
            system_id: system_id.to_string(),
            from_version: from_version.to_string(),
            to_version: to_version.to_string(),
            compatible: true,
            migrated_capabilities: Vec::new(),
            migrated_memories: Vec::new(),
            deprecated_capabilities: Vec::new(),
            conflicts: Vec::new(),
            status: "planned".to_string(),
            created_at,
        };
        if source.memory_schema_version != target.memory_schema_version
            && (snapshot
                .project_rows
                .iter()
                .any(|row| !row.memories.is_empty())
                || !snapshot.shared_memories.is_empty())
        {
            report.conflicts.push(format!(
                "memory schema {} -> {} has no registered migration primitive",
                source.memory_schema_version, target.memory_schema_version
            ));
        }
        for capability in snapshot
            .project_rows
            .iter()
            .flat_map(|row| row.capabilities.iter())
            .chain(
                snapshot
                    .shared_capabilities
                    .iter()
                    .map(|row| &row.capability),
            )
        {
            inspect_capability_migration(capability, system_id, &target, &mut report)?;
        }
        if !report.conflicts.is_empty() {
            report.compatible = false;
            report.status = "blocked".to_string();
            self.record_governance_migration(&report)?;
            return Ok(report);
        }

        let migration_result = (|| -> Result<(), String> {
            for project in &snapshot.project_rows {
                let mut connection = self.project_connection(&project.project_id)?;
                let transaction = connection
                    .transaction()
                    .map_err(|error| format!("GOVERNANCE_MIGRATION_FAILED: {error}"))?;
                for capability in &project.capabilities {
                    migrate_project_capability(
                        &transaction,
                        capability,
                        system_id,
                        to_version,
                        &mut report,
                    )?;
                }
                transaction
                    .execute(
                        "UPDATE domain_memories SET plugin_version=?2,updated_at=?3 WHERE system_id=?1 AND plugin_version=?4",
                        params![system_id,to_version,created_at,from_version],
                    )
                    .map_err(|error| format!("GOVERNANCE_MIGRATION_FAILED: {error}"))?;
                for memory in &project.memories {
                    if memory.plugin_version == from_version {
                        report
                            .migrated_memories
                            .push(format!("project:{}:{}", project.project_id, memory.id));
                    }
                }
                transaction
                    .commit()
                    .map_err(|error| format!("GOVERNANCE_MIGRATION_FAILED: {error}"))?;
            }
            let mut registry = self.registry()?;
            let transaction = registry
                .transaction()
                .map_err(|error| format!("GOVERNANCE_MIGRATION_FAILED: {error}"))?;
            for row in &snapshot.shared_capabilities {
                migrate_shared_capability(&transaction, row, system_id, to_version, &mut report)?;
            }
            transaction
                .execute(
                    "UPDATE shared_domain_memories SET plugin_version=?2,updated_at=?3 WHERE system_id=?1 AND plugin_version=?4",
                    params![system_id,to_version,created_at,from_version],
                )
                .map_err(|error| format!("GOVERNANCE_MIGRATION_FAILED: {error}"))?;
            for memory in &snapshot.shared_memories {
                if memory.memory.plugin_version == from_version {
                    report
                        .migrated_memories
                        .push(format!("{}:{}", memory.memory.scope, memory.memory.id));
                }
            }
            transaction
                .commit()
                .map_err(|error| format!("GOVERNANCE_MIGRATION_FAILED: {error}"))
        })();
        if let Err(error) = migration_result {
            self.restore_domain_governance_snapshot(&snapshot)?;
            return Err(error);
        }
        report.migrated_capabilities.sort();
        report.migrated_capabilities.dedup();
        report.migrated_memories.sort();
        report.migrated_memories.dedup();
        report.status = "applied".to_string();
        self.record_governance_migration(&report)?;
        Ok(report)
    }

    fn record_governance_migration(
        &self,
        report: &GovernanceMigrationReport,
    ) -> Result<(), String> {
        self.registry()?
            .execute(
                "INSERT INTO domain_governance_migrations(id,system_id,from_version,to_version,status,report,created_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![report.id,report.system_id,report.from_version,report.to_version,report.status,serde_json::to_string(report).map_err(|error| format!("GOVERNANCE_MIGRATION_SERIALIZE_FAILED: {error}"))?,report.created_at],
            )
            .map_err(|error| format!("GOVERNANCE_MIGRATION_RECORD_FAILED: {error}"))?;
        Ok(())
    }

    pub fn save_domain_memory(
        &self,
        project_id: &str,
        memory: &DomainMemory,
    ) -> Result<DomainMemory, String> {
        self.ensure_known_system(&memory.system_id)?;
        self.runtime_manifest_at_version(&memory.system_id, Some(&memory.plugin_version))
            .map_err(|error| {
                format!(
                    "MEMORY_DOMAIN_VERSION_INCOMPATIBLE: {}@{}: {error}",
                    memory.id, memory.plugin_version
                )
            })?;
        if !matches!(memory.scope.as_str(), "project" | "personal" | "team") {
            return Err("MEMORY_SCOPE_INVALID: expected project, personal, or team".to_string());
        }
        if !matches!(
            memory.status.as_str(),
            "candidate" | "active" | "disabled" | "contested" | "revoked"
        ) {
            return Err("MEMORY_STATUS_INVALID: unsupported status".to_string());
        }
        self.project_connection(project_id)?
            .execute(
                "INSERT INTO domain_memories(id,system_id,scope,kind,summary,body,status,source_task_id,plugin_version,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
                 ON CONFLICT(id) DO UPDATE SET scope=excluded.scope,kind=excluded.kind,summary=excluded.summary,body=excluded.body,status=excluded.status,plugin_version=excluded.plugin_version,updated_at=excluded.updated_at",
                params![memory.id,memory.system_id,memory.scope,memory.kind,memory.summary,memory.body.to_string(),memory.status,memory.source_task_id,memory.plugin_version,memory.created_at,memory.updated_at],
            )
            .map_err(|error| format!("MEMORY_WRITE_FAILED: {error}"))?;
        Ok(memory.clone())
    }

    pub fn list_domain_memories(
        &self,
        project_id: &str,
        system_id: &str,
        active_only: bool,
    ) -> Result<Vec<DomainMemory>, String> {
        self.ensure_known_system(system_id)?;
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT id,system_id,scope,kind,summary,body,status,source_task_id,plugin_version,created_at,updated_at
                 FROM domain_memories WHERE system_id=?1 AND (?2=0 OR status='active') ORDER BY updated_at DESC",
            )
            .map_err(|error| format!("MEMORY_LIST_FAILED: {error}"))?;
        let rows = statement
            .query_map(params![system_id, i64::from(active_only)], |row| {
                let body: String = row.get(5)?;
                Ok(DomainMemory {
                    id: row.get(0)?,
                    system_id: row.get(1)?,
                    scope: row.get(2)?,
                    kind: row.get(3)?,
                    summary: row.get(4)?,
                    body: serde_json::from_str(&body).unwrap_or_default(),
                    status: row.get(6)?,
                    source_task_id: row.get(7)?,
                    plugin_version: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })
            .map_err(|error| format!("MEMORY_LIST_FAILED: {error}"))?;
        let memories = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("MEMORY_LIST_FAILED: {error}"))?;
        if active_only {
            for memory in &memories {
                self.runtime_manifest_at_version(system_id, Some(&memory.plugin_version))
                    .map_err(|error| {
                        format!(
                            "MEMORY_DOMAIN_VERSION_INCOMPATIBLE: {}@{}: {error}",
                            memory.id, memory.plugin_version
                        )
                    })?;
            }
        }
        Ok(memories)
    }

    pub fn list_memory_candidates(
        &self,
        project_id: &str,
        system_id: Option<&str>,
    ) -> Result<Vec<DomainMemory>, String> {
        if let Some(system_id) = system_id {
            self.ensure_known_system(system_id)?;
        }
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT id,system_id,scope,kind,summary,body,status,source_task_id,plugin_version,created_at,updated_at
                 FROM domain_memories WHERE status='candidate' AND (?1 IS NULL OR system_id=?1)
                 ORDER BY updated_at DESC",
            )
            .map_err(|error| format!("MEMORY_LIST_FAILED: {error}"))?;
        let rows = statement
            .query_map([system_id], row_to_memory)
            .map_err(|error| format!("MEMORY_LIST_FAILED: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("MEMORY_LIST_FAILED: {error}"))
    }

    pub fn set_domain_memory_status(
        &self,
        project_id: &str,
        memory_id: &str,
        status: &str,
    ) -> Result<DomainMemory, String> {
        if !matches!(status, "active" | "contested" | "revoked") {
            return Err(
                "MEMORY_STATUS_INVALID: expected active, contested, or revoked".to_string(),
            );
        }
        let current = self.get_domain_memory(project_id, memory_id)?;
        let transition_allowed = match status {
            "active" => current.status == "candidate",
            "contested" => matches!(current.status.as_str(), "candidate" | "active"),
            "revoked" => current.status != "revoked",
            _ => false,
        };
        if !transition_allowed {
            return Err(format!(
                "MEMORY_STATUS_TRANSITION_DENIED: {} -> {status}",
                current.status
            ));
        }
        if status == "active" {
            self.runtime_manifest_at_version(&current.system_id, Some(&current.plugin_version))
                .map_err(|error| {
                    format!(
                        "MEMORY_DOMAIN_VERSION_INCOMPATIBLE: {}@{}: {error}",
                        current.id, current.plugin_version
                    )
                })?;
        }
        let changed = self
            .project_connection(project_id)?
            .execute(
                "UPDATE domain_memories SET status=?2,updated_at=?3 WHERE id=?1",
                params![memory_id, status, now_millis()],
            )
            .map_err(|error| format!("MEMORY_STATUS_FAILED: {error}"))?;
        if changed == 0 {
            return Err(format!("MEMORY_NOT_FOUND: {memory_id}"));
        }
        self.get_domain_memory(project_id, memory_id)
    }

    pub fn get_domain_memory(
        &self,
        project_id: &str,
        memory_id: &str,
    ) -> Result<DomainMemory, String> {
        self.project_connection(project_id)?
            .query_row(
                "SELECT id,system_id,scope,kind,summary,body,status,source_task_id,plugin_version,created_at,updated_at
                 FROM domain_memories WHERE id=?1",
                [memory_id],
                row_to_memory,
            )
            .optional()
            .map_err(|error| format!("MEMORY_READ_FAILED: {error}"))?
            .ok_or_else(|| format!("MEMORY_NOT_FOUND: {memory_id}"))
    }

    pub fn bind_system_session(
        &self,
        project_id: &str,
        binding: &SystemSessionBinding,
    ) -> Result<SystemSessionBinding, String> {
        self.ensure_known_system(&binding.system_id)?;
        self.project_connection(project_id)?
            .execute(
                "INSERT INTO system_sessions(task_id,system_id,session_id,plugin_version,draft_id,status,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(task_id) DO UPDATE SET system_id=excluded.system_id,session_id=excluded.session_id,plugin_version=excluded.plugin_version,draft_id=excluded.draft_id,status=excluded.status,updated_at=excluded.updated_at",
                params![binding.task_id,binding.system_id,binding.session_id,binding.plugin_version,binding.draft_id,binding.status,binding.updated_at],
            )
            .map_err(|error| format!("SYSTEM_SESSION_WRITE_FAILED: {error}"))?;
        Ok(binding.clone())
    }

    pub fn get_system_session(
        &self,
        project_id: &str,
        task_id: &str,
    ) -> Result<Option<SystemSessionBinding>, String> {
        self.project_connection(project_id)?
            .query_row(
                "SELECT task_id,system_id,session_id,plugin_version,draft_id,status,updated_at FROM system_sessions WHERE task_id=?1",
                [task_id],
                |row| Ok(SystemSessionBinding { task_id: row.get(0)?, system_id: row.get(1)?, session_id: row.get(2)?, plugin_version: row.get(3)?, draft_id: row.get(4)?, status: row.get(5)?, updated_at: row.get(6)? }),
            )
            .optional()
            .map_err(|error| format!("SYSTEM_SESSION_READ_FAILED: {error}"))
    }

    pub fn issue_task_scope(
        &self,
        project_id: &str,
        task_id: &str,
        read_systems: &[String],
        write_systems: &[String],
        draft_ids: &[String],
        plugin_versions: serde_json::Value,
        expires_at: i64,
    ) -> Result<TaskScopeLease, String> {
        if write_systems.is_empty() {
            return Err(
                "TASK_SCOPE_WRITE_SYSTEM_REQUIRED: at least one write system is required"
                    .to_string(),
            );
        }
        let issued_at = now_millis();
        if expires_at <= issued_at {
            return Err("TASK_SCOPE_EXPIRED: expiry must be in the future".to_string());
        }
        if expires_at - issued_at > TASK_SCOPE_MAX_TTL_MILLIS {
            return Err(format!(
                "TASK_SCOPE_TTL_EXCEEDED: maximum lease lifetime is {TASK_SCOPE_MAX_TTL_MILLIS} milliseconds"
            ));
        }
        for system_id in read_systems.iter().chain(write_systems.iter()) {
            self.ensure_known_system(system_id)?;
            let version = plugin_versions
                .get(system_id)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("TASK_SCOPE_PLUGIN_VERSION_REQUIRED: {system_id}"))?;
            self.runtime_manifest_at_version(system_id, Some(version))?;
        }
        if write_systems
            .iter()
            .any(|system_id| !read_systems.contains(system_id))
        {
            return Err(
                "TASK_SCOPE_WRITE_NOT_READABLE: write systems must also be readable".to_string(),
            );
        }
        for draft_id in draft_ids {
            let draft = self.get_draft(project_id, draft_id)?;
            if draft.status != crate::DraftStatus::Open {
                return Err(format!("TASK_SCOPE_DRAFT_NOT_OPEN: {draft_id}"));
            }
            self.validate_scope_draft_binding(
                project_id,
                draft_id,
                write_systems,
                &plugin_versions,
            )?;
        }
        let token = scope_token(project_id, task_id, issued_at)?;
        let token_hash = hash(&token);
        self.project_connection(project_id)?
            .execute(
                "INSERT INTO task_scope_leases(token_hash,task_id,read_systems,write_systems,draft_ids,plugin_versions,expires_at,revoked,created_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,0,?8)",
                params![token_hash, task_id, json_array(read_systems), json_array(write_systems), json_array(draft_ids), plugin_versions.to_string(), expires_at, issued_at],
            )
            .map_err(|error| format!("TASK_SCOPE_WRITE_FAILED: {error}"))?;
        Ok(TaskScopeLease {
            token,
            task_id: task_id.to_string(),
            read_systems: read_systems.to_vec(),
            write_systems: write_systems.to_vec(),
            draft_ids: draft_ids.to_vec(),
            plugin_versions,
            expires_at,
        })
    }

    pub fn authorize_task_scope(
        &self,
        project_id: &str,
        token: &str,
        read_system: Option<&str>,
        write_system: Option<&str>,
        draft_id: Option<&str>,
    ) -> Result<TaskScopeLease, String> {
        let token_hash = hash(token);
        let row = self
            .project_connection(project_id)?
            .query_row(
                "SELECT task_id,read_systems,write_systems,draft_ids,plugin_versions,expires_at,revoked
                 FROM task_scope_leases WHERE token_hash=?1",
                [&token_hash],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)? != 0,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("TASK_SCOPE_READ_FAILED: {error}"))?
            .ok_or_else(|| "TASK_SCOPE_NOT_FOUND: invalid scope token".to_string())?;
        let (task_id, read_json, write_json, draft_json, versions_json, expires_at, revoked) = row;
        if revoked || expires_at <= now_millis() {
            return Err("TASK_SCOPE_EXPIRED: scope is revoked or expired".to_string());
        }
        let plugin_versions: serde_json::Value = serde_json::from_str(&versions_json)
            .map_err(|error| format!("TASK_SCOPE_PLUGIN_VERSIONS_INVALID: {error}"))?;
        let read_systems = parse_array(&read_json);
        let write_systems = parse_array(&write_json);
        let draft_ids = parse_array(&draft_json);
        if read_system.is_some_and(|value| !read_systems.iter().any(|item| item == value)) {
            return Err("TASK_SCOPE_READ_DENIED: system is outside the lease".to_string());
        }
        if write_system.is_some_and(|value| !write_systems.iter().any(|item| item == value)) {
            return Err("TASK_SCOPE_WRITE_DENIED: system is outside the lease".to_string());
        }
        if draft_id.is_some_and(|value| !draft_ids.iter().any(|item| item == value)) {
            return Err("TASK_SCOPE_DRAFT_DENIED: draft is outside the lease".to_string());
        }
        if let Some(draft_id) = draft_id {
            self.validate_scope_draft_binding(
                project_id,
                draft_id,
                &write_systems,
                &plugin_versions,
            )?;
        }
        for system_id in [read_system, write_system].into_iter().flatten() {
            let pinned_version = plugin_versions
                .get(system_id)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("TASK_SCOPE_PLUGIN_VERSION_REQUIRED: {system_id}"))?;
            self.runtime_manifest_at_version(system_id, Some(pinned_version))?;
        }
        Ok(TaskScopeLease {
            token: token.to_string(),
            task_id,
            read_systems,
            write_systems,
            draft_ids,
            plugin_versions,
            expires_at,
        })
    }

    pub fn attach_draft_to_scope(
        &self,
        project_id: &str,
        token: &str,
        system_id: &str,
        draft_id: &str,
    ) -> Result<TaskScopeLease, String> {
        let mut lease =
            self.authorize_task_scope(project_id, token, Some(system_id), Some(system_id), None)?;
        let draft = self.get_draft(project_id, draft_id)?;
        if draft.status != crate::DraftStatus::Open {
            return Err(format!("TASK_SCOPE_DRAFT_NOT_OPEN: {draft_id}"));
        }
        self.validate_scope_draft_binding(
            project_id,
            draft_id,
            &lease.write_systems,
            &lease.plugin_versions,
        )?;
        if !lease.draft_ids.iter().any(|value| value == draft_id) {
            lease.draft_ids.push(draft_id.to_string());
            self.project_connection(project_id)?
                .execute(
                    "UPDATE task_scope_leases SET draft_ids=?2 WHERE token_hash=?1 AND revoked=0",
                    params![hash(token), json_array(&lease.draft_ids)],
                )
                .map_err(|error| format!("TASK_SCOPE_ATTACH_FAILED: {error}"))?;
        }
        Ok(lease)
    }

    fn validate_scope_draft_binding(
        &self,
        project_id: &str,
        draft_id: &str,
        write_systems: &[String],
        plugin_versions: &serde_json::Value,
    ) -> Result<(), String> {
        let binding = self
            .project_connection(project_id)?
            .query_row(
                "SELECT system_id,plugin_version,legacy FROM draft_domains WHERE draft_id=?1",
                [draft_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)? != 0,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("TASK_SCOPE_DRAFT_BINDING_READ_FAILED: {error}"))?
            .ok_or_else(|| format!("TASK_SCOPE_DRAFT_DOMAIN_REQUIRED: {draft_id}"))?;
        let (Some(system_id), Some(plugin_version), false) = binding else {
            return Err(format!("TASK_SCOPE_DRAFT_DOMAIN_REQUIRED: {draft_id}"));
        };
        if !write_systems
            .iter()
            .any(|candidate| candidate == &system_id)
        {
            return Err(format!(
                "TASK_SCOPE_DRAFT_SYSTEM_MISMATCH: {draft_id} is bound to {system_id}"
            ));
        }
        let pinned_version = plugin_versions
            .get(&system_id)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("TASK_SCOPE_PLUGIN_VERSION_REQUIRED: {system_id}"))?;
        if pinned_version != plugin_version {
            return Err(format!(
                "TASK_SCOPE_DRAFT_VERSION_MISMATCH: {draft_id} is bound to {plugin_version}, lease pins {pinned_version}"
            ));
        }
        self.runtime_manifest_at_version(&system_id, Some(&plugin_version))?;
        Ok(())
    }

    pub fn revoke_task_scope(&self, project_id: &str, token: &str) -> Result<(), String> {
        let changed = self
            .project_connection(project_id)?
            .execute(
                "UPDATE task_scope_leases SET revoked=1 WHERE token_hash=?1",
                [hash(token)],
            )
            .map_err(|error| format!("TASK_SCOPE_REVOKE_FAILED: {error}"))?;
        if changed == 0 {
            return Err("TASK_SCOPE_NOT_FOUND: invalid scope token".to_string());
        }
        Ok(())
    }

    fn ensure_known_system(&self, system_id: &str) -> Result<(), String> {
        if self
            .list_domain_systems()?
            .iter()
            .any(|manifest| manifest.system_id == system_id)
        {
            Ok(())
        } else {
            Err(format!("DOMAIN_SYSTEM_NOT_FOUND: {system_id}"))
        }
    }
}

fn memory_candidate_for_receipt(receipt: &TaskReceipt) -> DomainMemory {
    let plugin_version = receipt_plugin_version(receipt)
        .unwrap_or("unknown")
        .to_string();
    DomainMemory {
        id: format!("memory-{}", &hash(&receipt.id)[..20]),
        system_id: receipt.system_id.clone(),
        scope: "project".to_string(),
        kind: "task-rule-candidate".to_string(),
        summary: receipt.summary.clone(),
        body: serde_json::json!({
            "state": "PROPOSED",
            "rule": receipt.summary,
            "preference": null,
            "reason": "successful task receipt",
            "applicableScope": {"systemId": receipt.system_id},
            "evidence": receipt.evidence,
            "taskId": receipt.task_id,
            "packVersion": plugin_version,
        }),
        status: "candidate".to_string(),
        source_task_id: receipt.task_id.clone(),
        plugin_version,
        created_at: receipt.created_at,
        updated_at: receipt.created_at,
    }
}

fn receipt_plugin_version(receipt: &TaskReceipt) -> Option<&str> {
    receipt
        .plugin_versions
        .get(&receipt.system_id)
        .or_else(|| receipt.plugin_versions.get("domain"))
        .and_then(serde_json::Value::as_str)
}

fn row_to_receipt(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskReceipt> {
    let plugin_versions: String = row.get(6)?;
    let evidence: String = row.get(7)?;
    Ok(TaskReceipt {
        id: row.get(0)?,
        task_id: row.get(1)?,
        system_id: row.get(2)?,
        summary: row.get(3)?,
        status: row.get(4)?,
        draft_id: row.get(5)?,
        plugin_versions: serde_json::from_str(&plugin_versions).unwrap_or_default(),
        evidence: serde_json::from_str(&evidence).unwrap_or_default(),
        created_at: row.get(8)?,
    })
}

fn verify_operation_evidence(
    manifest: &DomainManifest,
    draft: &crate::Draft,
    evidence: &[DraftOperationEvidence],
) -> Result<(), String> {
    if evidence.is_empty()
        || evidence
            .first()
            .is_some_and(|item| item.revision_before != 0)
        || evidence.last().map(|item| item.revision_after) != Some(draft.revision)
    {
        return Err("CAPABILITY_EVIDENCE_INCOMPLETE: operation evidence must cover the complete Draft revision chain".to_string());
    }
    let mut expected_revision = 0;
    for item in evidence {
        if item.system_id != manifest.system_id
            || item.plugin_version != manifest.version
            || item.revision_before != expected_revision
            || item.revision_after <= item.revision_before
            || item
                .parameters
                .get("operation")
                .and_then(serde_json::Value::as_str)
                != Some(item.operation_id.as_str())
            || item
                .parameters
                .get("expectedRevision")
                .and_then(serde_json::Value::as_i64)
                != Some(item.revision_before)
        {
            return Err(
                "CAPABILITY_EVIDENCE_CHAIN_INVALID: operation evidence is inconsistent".to_string(),
            );
        }
        let operation = manifest
            .operations
            .iter()
            .find(|operation| operation.id == item.operation_id)
            .ok_or_else(|| format!("CAPABILITY_OPERATION_NOT_REGISTERED: {}", item.operation_id))?;
        if hash_json(&operation.parameter_schema)? != item.parameter_schema_hash {
            return Err(format!(
                "CAPABILITY_OPERATION_SCHEMA_MISMATCH: {}",
                item.operation_id
            ));
        }
        expected_revision = item.revision_after;
    }
    Ok(())
}

fn operation_evidence_prefix_hash(evidence: &[DraftOperationEvidence]) -> Result<String, String> {
    let core = evidence
        .iter()
        .map(|item| {
            serde_json::json!({
                "draftId":item.draft_id,
                "sequence":item.sequence,
                "systemId":item.system_id,
                "pluginVersion":item.plugin_version,
                "operationId":item.operation_id,
                "parameters":item.parameters,
                "parameterSchemaHash":item.parameter_schema_hash,
                "revisionBefore":item.revision_before,
                "revisionAfter":item.revision_after,
            })
        })
        .collect::<Vec<_>>();
    hash_json(&core)
}

fn verify_replay_proofs(
    evidence: &[DraftOperationEvidence],
    final_source_change_hash: &str,
) -> Result<(), String> {
    for (index, item) in evidence.iter().enumerate() {
        let expected = operation_evidence_prefix_hash(&evidence[..=index])?;
        if item.replay_change_hash.is_empty() || item.replay_evidence_hash != expected {
            return Err("CAPABILITY_REPLAY_EVIDENCE_MISMATCH: operation parameters were not safely replayed".to_string());
        }
    }
    if evidence
        .last()
        .is_none_or(|item| item.replay_change_hash != final_source_change_hash)
    {
        return Err(
            "CAPABILITY_REPLAY_DIFF_MISMATCH: replay output differs from the applied source Draft"
                .to_string(),
        );
    }
    Ok(())
}

fn promoted_parameter_schema(
    manifest: &DomainManifest,
    evidence: &[DraftOperationEvidence],
) -> Result<serde_json::Value, String> {
    let schemas = evidence
        .iter()
        .map(|item| {
            manifest
                .operations
                .iter()
                .find(|operation| operation.id == item.operation_id)
                .map(|operation| sanitize_operation_parameter_schema(&operation.parameter_schema))
                .ok_or_else(|| {
                    format!("CAPABILITY_OPERATION_NOT_REGISTERED: {}", item.operation_id)
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if schemas.len() == 1 {
        return Ok(schemas.into_iter().next().unwrap_or_default());
    }
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for (index, schema) in schemas.into_iter().enumerate() {
        let key = format!("step{index}");
        required.push(serde_json::Value::String(key.clone()));
        properties.insert(key, schema);
    }
    Ok(serde_json::json!({
        "type":"object",
        "properties":properties,
        "required":required,
        "additionalProperties":false,
    }))
}

fn sanitize_operation_parameter_schema(schema: &serde_json::Value) -> serde_json::Value {
    let mut schema = schema.clone();
    if let Some(properties) = schema
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
    {
        properties.remove("operation");
        properties.remove("expectedRevision");
    }
    if let Some(required) = schema
        .get_mut("required")
        .and_then(serde_json::Value::as_array_mut)
    {
        required.retain(|value| !matches!(value.as_str(), Some("operation" | "expectedRevision")));
    }
    schema
}

fn append_unique(target: &mut Vec<String>, values: &[String]) {
    for value in values {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn compare_semver(left: &str, right: &str) -> std::cmp::Ordering {
    match (Version::parse(left), Version::parse(right)) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
        (Err(_), Ok(_)) => std::cmp::Ordering::Less,
        (Err(_), Err(_)) => left.cmp(right),
    }
}

fn capability_scope_priority(scope: &str) -> u8 {
    match scope {
        "project" => 3,
        "personal" => 2,
        "team" => 1,
        _ => 0,
    }
}

fn select_capability_resolution(
    mut candidates: Vec<CapabilityResolution>,
) -> Option<CapabilityResolution> {
    candidates.sort_by(|left, right| {
        capability_scope_priority(&right.resolved_scope)
            .cmp(&capability_scope_priority(&left.resolved_scope))
            .then_with(|| compare_semver(&right.capability.version, &left.capability.version))
    });
    let mut winner = candidates.first()?.clone();
    winner.shadowed_scopes = candidates
        .iter()
        .filter(|candidate| candidate.resolved_scope != winner.resolved_scope)
        .map(|candidate| candidate.resolved_scope.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Some(winner)
}

fn capability_material_hash(capability: &UserCapability) -> Result<String, String> {
    hash_json(&serde_json::json!({
        "id":capability.id,
        "version":capability.version,
        "systemId":capability.system_id,
        "name":capability.name,
        "description":capability.description,
        "parameterSchema":capability.parameter_schema,
        "steps":capability.steps,
        "readSystems":capability.read_systems,
        "writeSystems":capability.write_systems,
        "sourceTaskId":capability.source_task_id,
    }))
}

fn capability_touches_system(capability: &UserCapability, system_id: &str) -> bool {
    capability.system_id == system_id
        || capability
            .steps
            .as_array()
            .into_iter()
            .flatten()
            .any(|step| {
                step.get("systemId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(&capability.system_id)
                    == system_id
            })
}

fn capability_migration_key(capability: &UserCapability) -> String {
    format!(
        "{}:{}@{}",
        capability.scope, capability.id, capability.version
    )
}

fn inspect_capability_migration(
    capability: &UserCapability,
    system_id: &str,
    target: &DomainManifest,
    report: &mut GovernanceMigrationReport,
) -> Result<(), String> {
    if capability.status != "active" {
        return Ok(());
    }
    for step in capability
        .steps
        .as_array()
        .into_iter()
        .flatten()
        .filter(|step| {
            step.get("systemId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&capability.system_id)
                == system_id
        })
    {
        let operation_id = step
            .get("operation")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "CAPABILITY_STEP_INVALID: operation missing".to_string())?;
        let Some(operation) = target
            .operations
            .iter()
            .find(|operation| operation.id == operation_id)
        else {
            report
                .deprecated_capabilities
                .push(capability_migration_key(capability));
            continue;
        };
        let expected_hash = hash_json(&operation.parameter_schema)?;
        if step
            .get("parameterSchemaHash")
            .and_then(serde_json::Value::as_str)
            != Some(expected_hash.as_str())
        {
            report.conflicts.push(format!(
                "{} operation {operation_id} changed parameter schema",
                capability_migration_key(capability)
            ));
        }
    }
    Ok(())
}

fn migrated_capability(
    capability: &UserCapability,
    system_id: &str,
    to_version: &str,
) -> Result<UserCapability, String> {
    let mut migrated = capability.clone();
    let mut version = Version::parse(&migrated.version)
        .map_err(|error| format!("CAPABILITY_VERSION_INVALID: {error}"))?;
    version.patch = version.patch.saturating_add(1);
    migrated.version = version.to_string();
    migrated.updated_at = now_millis();
    if let Some(steps) = migrated.steps.as_array_mut() {
        for step in steps {
            let step_system = step
                .get("systemId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&migrated.system_id);
            if step_system == system_id {
                step["pluginVersion"] = serde_json::json!(to_version);
            }
        }
    }
    Ok(migrated)
}

fn migrate_project_capability(
    transaction: &rusqlite::Transaction<'_>,
    capability: &UserCapability,
    system_id: &str,
    to_version: &str,
    report: &mut GovernanceMigrationReport,
) -> Result<(), String> {
    if capability.status != "active" {
        return Ok(());
    }
    let key = capability_migration_key(capability);
    if report.deprecated_capabilities.contains(&key) {
        transaction
            .execute(
                "UPDATE user_capabilities SET status='deprecated',updated_at=?3 WHERE id=?1 AND version=?2",
                params![capability.id,capability.version,now_millis()],
            )
            .map_err(|error| format!("GOVERNANCE_MIGRATION_FAILED: {error}"))?;
        return Ok(());
    }
    let migrated = migrated_capability(capability, system_id, to_version)?;
    transaction
        .execute(
            "UPDATE user_capabilities SET status='deprecated',updated_at=?3 WHERE id=?1 AND version=?2",
            params![capability.id,capability.version,now_millis()],
        )
        .map_err(|error| format!("GOVERNANCE_MIGRATION_FAILED: {error}"))?;
    insert_project_capability(transaction, &migrated)?;
    report.migrated_capabilities.push(format!(
        "project:{}@{}->{}",
        capability.id, capability.version, migrated.version
    ));
    Ok(())
}

fn migrate_shared_capability(
    transaction: &rusqlite::Transaction<'_>,
    row: &SharedCapabilityRow,
    system_id: &str,
    to_version: &str,
    report: &mut GovernanceMigrationReport,
) -> Result<(), String> {
    let capability = &row.capability;
    if capability.status != "active" {
        return Ok(());
    }
    let key = capability_migration_key(capability);
    if report.deprecated_capabilities.contains(&key) {
        transaction
            .execute(
                "UPDATE shared_user_capabilities SET status='deprecated',updated_at=?4 WHERE scope=?1 AND id=?2 AND version=?3",
                params![capability.scope,capability.id,capability.version,now_millis()],
            )
            .map_err(|error| format!("GOVERNANCE_MIGRATION_FAILED: {error}"))?;
        return Ok(());
    }
    let migrated = migrated_capability(capability, system_id, to_version)?;
    transaction
        .execute(
            "UPDATE shared_user_capabilities SET status='deprecated',updated_at=?4 WHERE scope=?1 AND id=?2 AND version=?3",
            params![capability.scope,capability.id,capability.version,now_millis()],
        )
        .map_err(|error| format!("GOVERNANCE_MIGRATION_FAILED: {error}"))?;
    insert_shared_capability(transaction, &row.source_project_id, &migrated)?;
    report.migrated_capabilities.push(format!(
        "{}:{}@{}->{}",
        capability.scope, capability.id, capability.version, migrated.version
    ));
    Ok(())
}

fn insert_project_capability(
    transaction: &rusqlite::Transaction<'_>,
    capability: &UserCapability,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO user_capabilities(id,version,system_id,scope,name,description,parameter_schema,steps,read_systems,write_systems,status,source_task_id,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![capability.id,capability.version,capability.system_id,capability.scope,capability.name,capability.description,capability.parameter_schema.to_string(),capability.steps.to_string(),json_array(&capability.read_systems),json_array(&capability.write_systems),capability.status,capability.source_task_id,capability.created_at,capability.updated_at],
        )
        .map_err(|error| format!("GOVERNANCE_WRITE_FAILED: {error}"))?;
    Ok(())
}

fn insert_project_memory(
    transaction: &rusqlite::Transaction<'_>,
    memory: &DomainMemory,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO domain_memories(id,system_id,scope,kind,summary,body,status,source_task_id,plugin_version,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![memory.id,memory.system_id,memory.scope,memory.kind,memory.summary,memory.body.to_string(),memory.status,memory.source_task_id,memory.plugin_version,memory.created_at,memory.updated_at],
        )
        .map_err(|error| format!("GOVERNANCE_WRITE_FAILED: {error}"))?;
    Ok(())
}

fn insert_shared_capability(
    transaction: &rusqlite::Transaction<'_>,
    source_project_id: &str,
    capability: &UserCapability,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO shared_user_capabilities(scope,id,version,source_project_id,system_id,name,description,parameter_schema,steps,read_systems,write_systems,status,source_task_id,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![capability.scope,capability.id,capability.version,source_project_id,capability.system_id,capability.name,capability.description,capability.parameter_schema.to_string(),capability.steps.to_string(),json_array(&capability.read_systems),json_array(&capability.write_systems),capability.status,capability.source_task_id,capability.created_at,capability.updated_at],
        )
        .map_err(|error| format!("GOVERNANCE_WRITE_FAILED: {error}"))?;
    Ok(())
}

fn insert_shared_memory(
    transaction: &rusqlite::Transaction<'_>,
    row: &SharedMemoryRow,
) -> Result<(), String> {
    let memory = &row.memory;
    transaction
        .execute(
            "INSERT INTO shared_domain_memories(scope,id,source_project_id,system_id,kind,summary,body,status,source_task_id,plugin_version,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![memory.scope,memory.id,row.source_project_id,memory.system_id,memory.kind,memory.summary,memory.body.to_string(),memory.status,memory.source_task_id,memory.plugin_version,memory.created_at,memory.updated_at],
        )
        .map_err(|error| format!("GOVERNANCE_WRITE_FAILED: {error}"))?;
    Ok(())
}

fn hash_json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| hash_bytes(&bytes))
        .map_err(|error| format!("CAPABILITY_EVIDENCE_SERIALIZE_FAILED: {error}"))
}

fn validate_capability(capability: &UserCapability) -> Result<(), String> {
    if capability.scope != "project" && capability.scope != "personal" && capability.scope != "team"
    {
        return Err("CAPABILITY_SCOPE_INVALID: expected project, personal, or team".to_string());
    }
    if capability.id.is_empty()
        || capability.id.len() > 64
        || !capability
            .id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("CAPABILITY_ID_INVALID: expected a portable identifier".to_string());
    }
    if Version::parse(&capability.version).is_err() {
        return Err("CAPABILITY_VERSION_INVALID: expected major.minor.patch".to_string());
    }
    if !capability.parameter_schema.is_object() {
        return Err("CAPABILITY_PARAMETER_SCHEMA_INVALID: expected an object schema".to_string());
    }
    let steps = capability
        .steps
        .as_array()
        .filter(|steps| !steps.is_empty() && steps.len() <= 256)
        .ok_or_else(|| "CAPABILITY_STEPS_INVALID: expected 1..256 structured steps".to_string())?;
    for step in steps {
        let kind = step.get("type").and_then(serde_json::Value::as_str);
        let operation = step
            .get("operation")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if kind != Some("domain-operation")
            || operation.is_empty()
            || ["shell", "exec", "command", "script", "absolute"]
                .iter()
                .any(|forbidden| operation.to_ascii_lowercase().contains(forbidden))
        {
            return Err(
                "CAPABILITY_STEP_FORBIDDEN: only registered domain operations are allowed"
                    .to_string(),
            );
        }
    }
    if capability.write_systems.is_empty() {
        return Err("CAPABILITY_WRITE_SYSTEM_REQUIRED: writeSystems cannot be empty".to_string());
    }
    if capability
        .write_systems
        .iter()
        .any(|system| !capability.read_systems.contains(system))
    {
        return Err(
            "CAPABILITY_WRITE_NOT_READABLE: write systems must also be readable".to_string(),
        );
    }
    if !matches!(
        capability.status.as_str(),
        "draft" | "active" | "disabled" | "deprecated"
    ) {
        return Err("CAPABILITY_STATUS_INVALID: unsupported status".to_string());
    }
    Ok(())
}

fn scope_token(project_id: &str, task_id: &str, issued_at: i64) -> Result<String, String> {
    let mut entropy = [0_u8; 32];
    getrandom::fill(&mut entropy).map_err(|error| format!("TASK_SCOPE_ENTROPY_FAILED: {error}"))?;
    Ok(hash(&format!(
        "{project_id}:{task_id}:{issued_at}:{}:{entropy:x?}",
        std::process::id()
    )))
}

fn hash(value: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(value.as_bytes());
    format!("{:x}", digest.finalize())
}

fn hash_bytes(value: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(value);
    format!("{:x}", digest.finalize())
}

fn validate_legacy_relative_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("DRAFT_LEGACY_PATH_INVALID: expected a normalized relative path".to_string());
    }
    Ok(())
}

fn json_array(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

fn parse_array(value: &str) -> Vec<String> {
    serde_json::from_str(value).unwrap_or_default()
}

fn row_to_capability(row: &rusqlite::Row<'_>) -> rusqlite::Result<UserCapability> {
    let parameter_schema: String = row.get(6)?;
    let steps: String = row.get(7)?;
    let read_systems: String = row.get(8)?;
    let write_systems: String = row.get(9)?;
    Ok(UserCapability {
        id: row.get(0)?,
        version: row.get(1)?,
        system_id: row.get(2)?,
        scope: row.get(3)?,
        name: row.get(4)?,
        description: row.get(5)?,
        parameter_schema: serde_json::from_str(&parameter_schema).unwrap_or_default(),
        steps: serde_json::from_str(&steps).unwrap_or_default(),
        read_systems: parse_array(&read_systems),
        write_systems: parse_array(&write_systems),
        status: row.get(10)?,
        source_task_id: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn row_to_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<DomainMemory> {
    let body: String = row.get(5)?;
    Ok(DomainMemory {
        id: row.get(0)?,
        system_id: row.get(1)?,
        scope: row.get(2)?,
        kind: row.get(3)?,
        summary: row.get(4)?,
        body: serde_json::from_str(&body).unwrap_or_default(),
        status: row.get(6)?,
        source_task_id: row.get(7)?,
        plugin_version: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    static CAPABILITY_TEST_NONCE: AtomicU64 = AtomicU64::new(0);

    fn governance_test_store(base: &Path) -> DomainStore {
        DomainStore::new_trusted_fixture(base.join("data")).unwrap()
    }

    #[test]
    fn capabilities_reject_shell_steps() {
        let capability = UserCapability {
            id: "batch-price".to_string(),
            version: "0.1.0".to_string(),
            system_id: "shop".to_string(),
            scope: "project".to_string(),
            name: "Batch price".to_string(),
            description: String::new(),
            parameter_schema: serde_json::json!({}),
            steps: serde_json::json!([{"type":"shell.exec"}]),
            read_systems: vec!["item".to_string()],
            write_systems: vec!["shop".to_string()],
            status: "draft".to_string(),
            source_task_id: "task-1".to_string(),
            created_at: 1,
            updated_at: 1,
        };
        assert!(validate_capability(&capability).is_err());
    }

    #[test]
    fn governance_records_are_versioned_and_scope_leases_fail_closed() {
        let base = std::env::temp_dir().join(format!("mir3-governance-{}", std::process::id()));
        let root = base.join("木立");
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200")).unwrap();
        let store = governance_test_store(&base);
        let project = store.import_project(&root).unwrap();
        let now = now_millis();
        let capability = UserCapability {
            id: "batch-price".to_string(),
            version: "0.1.0".to_string(),
            system_id: "shop".to_string(),
            scope: "project".to_string(),
            name: "Batch price".to_string(),
            description: "safe operation".to_string(),
            parameter_schema: serde_json::json!({"type":"object"}),
            steps: serde_json::json!([{"type":"domain-operation","operation":"batch-price-shop"}]),
            read_systems: vec!["shop".to_string(), "item".to_string()],
            write_systems: vec!["shop".to_string()],
            status: "draft".to_string(),
            source_task_id: "task-1".to_string(),
            created_at: now,
            updated_at: now,
        };
        assert!(store
            .save_user_capability(&project.id, &capability)
            .unwrap_err()
            .starts_with("CAPABILITY_STEP_VERSION_REQUIRED:"));

        let memory = DomainMemory {
            id: "memory-1".to_string(),
            system_id: "shop".to_string(),
            scope: "project".to_string(),
            kind: "rule".to_string(),
            summary: "价格规则".to_string(),
            body: serde_json::json!({"minimum":1}),
            status: "candidate".to_string(),
            source_task_id: "task-1".to_string(),
            plugin_version: "1.2.0".to_string(),
            created_at: now,
            updated_at: now,
        };
        store.save_domain_memory(&project.id, &memory).unwrap();
        let mut incompatible_memory = memory.clone();
        incompatible_memory.id = "memory-incompatible".to_string();
        incompatible_memory.plugin_version = "9.0.0".to_string();
        assert!(store
            .save_domain_memory(&project.id, &incompatible_memory)
            .unwrap_err()
            .starts_with("MEMORY_DOMAIN_VERSION_INCOMPATIBLE:"));
        assert_eq!(
            store
                .list_domain_memories(&project.id, "shop", false)
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .list_domain_memories(&project.id, "shop", true)
            .unwrap()
            .is_empty());

        let receipt = TaskReceipt {
            id: "receipt-task-1".to_string(),
            task_id: "task-1".to_string(),
            system_id: "shop".to_string(),
            summary: "批量价格规则".to_string(),
            status: "applied".to_string(),
            draft_id: None,
            plugin_versions: serde_json::json!({"domain":"1.2.0"}),
            evidence: serde_json::json!({"diffHash":"abc"}),
            created_at: now,
        };
        store.save_task_receipt(&project.id, &receipt).unwrap();
        let candidates = store
            .list_memory_candidates(&project.id, Some("shop"))
            .unwrap();
        assert_eq!(candidates.len(), 2);
        let proposed = candidates
            .iter()
            .find(|memory| {
                memory.source_task_id == "task-1" && memory.kind == "task-rule-candidate"
            })
            .unwrap();
        assert_eq!(proposed.status, "candidate");
        assert_eq!(proposed.body["state"], "PROPOSED");
        let proposed_id = proposed.id.clone();
        store
            .set_domain_memory_status(&project.id, &proposed_id, "active")
            .unwrap();
        store.save_task_receipt(&project.id, &receipt).unwrap();
        assert_eq!(
            store
                .get_domain_memory(&project.id, &proposed_id)
                .unwrap()
                .status,
            "active"
        );
        assert_eq!(
            store
                .list_domain_memories(&project.id, "shop", true)
                .unwrap()
                .len(),
            1
        );
        store
            .project_connection(&project.id)
            .unwrap()
            .execute(
                "UPDATE domain_memories SET plugin_version='9.0.0' WHERE id=?1",
                [&proposed_id],
            )
            .unwrap();
        assert!(store
            .list_domain_memories(&project.id, "shop", true)
            .unwrap_err()
            .starts_with("MEMORY_DOMAIN_VERSION_INCOMPATIBLE:"));
        store
            .project_connection(&project.id)
            .unwrap()
            .execute(
                "UPDATE domain_memories SET plugin_version='1.2.0' WHERE id=?1",
                [&proposed_id],
            )
            .unwrap();

        let lease = store
            .issue_task_scope(
                &project.id,
                "task-1",
                &["shop".to_string(), "item".to_string()],
                &["shop".to_string()],
                &[],
                serde_json::json!({"shop":"1.2.0","item":"1.2.0"}),
                now + 60_000,
            )
            .unwrap();
        assert!(store
            .authorize_task_scope(&project.id, &lease.token, Some("item"), None, None)
            .is_ok());
        assert!(store
            .authorize_task_scope(&project.id, &lease.token, Some("quest"), None, None)
            .is_err());
        store.revoke_task_scope(&project.id, &lease.token).unwrap();
        assert!(store
            .authorize_task_scope(&project.id, &lease.token, None, None, None)
            .is_err());
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn scope_lease_rejects_long_ttl_and_foreign_or_unscoped_drafts() {
        let base = std::env::temp_dir().join(format!(
            "mir3-scope-binding-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let root = base.join("木立");
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200")).unwrap();
        let store = governance_test_store(&base);
        let project = store.import_project(&root).unwrap();
        let now = now_millis();

        let too_long = store.issue_task_scope(
            &project.id,
            "long-task",
            &["shop".to_string()],
            &["shop".to_string()],
            &[],
            serde_json::json!({"shop":"1.2.0"}),
            now + TASK_SCOPE_MAX_TTL_MILLIS + 60_000,
        );
        assert!(too_long
            .unwrap_err()
            .starts_with("TASK_SCOPE_TTL_EXCEEDED:"));

        let unscoped = store.open_draft(&project.id, "unscoped").unwrap();
        let unscoped_lease = store.issue_task_scope(
            &project.id,
            "unscoped-task",
            &["shop".to_string()],
            &["shop".to_string()],
            std::slice::from_ref(&unscoped.id),
            serde_json::json!({"shop":"1.2.0"}),
            now + 60_000,
        );
        assert!(unscoped_lease
            .unwrap_err()
            .starts_with("TASK_SCOPE_DRAFT_DOMAIN_REQUIRED:"));

        let foreign = store.open_draft(&project.id, "foreign").unwrap();
        store
            .bind_draft_domain(&project.id, &foreign.id, "item", "1.2.0", None)
            .unwrap();
        let foreign_lease = store.issue_task_scope(
            &project.id,
            "foreign-task",
            &["shop".to_string()],
            &["shop".to_string()],
            std::slice::from_ref(&foreign.id),
            serde_json::json!({"shop":"1.2.0"}),
            now + 60_000,
        );
        assert!(foreign_lease
            .unwrap_err()
            .starts_with("TASK_SCOPE_DRAFT_SYSTEM_MISMATCH:"));

        fs::remove_dir_all(base).ok();
    }

    fn applied_shop_source(
        operation_ids: &[&str],
    ) -> (std::path::PathBuf, DomainStore, String, TaskReceipt) {
        let base = std::env::temp_dir().join(format!(
            "mir3-capability-replay-{}-{}",
            std::process::id(),
            CAPABILITY_TEST_NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("项目");
        let relative = "引擎/Mir200/Envir/shop.txt";
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Envir")).unwrap();
        fs::write(root.join(relative), "shopId=1\nprice=1\n").unwrap();
        let store = governance_test_store(&base);
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let draft = store.open_draft(&project.id, "shop operations").unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "shop", "1.2.0", None)
            .unwrap();
        for (index, operation_id) in operation_ids.iter().enumerate() {
            let revision_before = index as i64;
            let content = format!("shopId=1\nprice={}\n", index + 2);
            store
                .patch_draft(
                    &project.id,
                    &draft.id,
                    revision_before,
                    &[crate::DraftChangeInput {
                        path: relative.to_string(),
                        content: Some(content),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )
                .unwrap();
            let evidence = store
                .record_draft_operation_evidence(
                    &project.id,
                    &draft.id,
                    operation_id,
                    &serde_json::json!({
                        "operation":operation_id,
                        "resourceIds":["shop:1"],
                        "changes":{"price":index + 2},
                        "expectedRevision":revision_before,
                    }),
                    revision_before,
                    revision_before + 1,
                )
                .unwrap();
            let replay_change_hash = store
                .draft_change_evidence_hash(&project.id, &draft.id)
                .unwrap();
            store
                .seal_draft_operation_replay(
                    &project.id,
                    &draft.id,
                    evidence.sequence,
                    &replay_change_hash,
                )
                .unwrap();
        }
        let preview = store.preview_draft(&project.id, &draft.id).unwrap();
        let snapshot = store
            .apply_draft(
                &project.id,
                &draft.id,
                preview.draft.revision,
                &preview.diff_hash,
            )
            .unwrap();
        let receipt = store
            .record_applied_draft_receipt(&project.id, &draft.id, &preview.diff_hash, &snapshot)
            .unwrap()
            .unwrap();
        (base, store, project.id, receipt)
    }

    fn applied_cross_system_source() -> (std::path::PathBuf, DomainStore, String, Vec<TaskReceipt>)
    {
        let base = std::env::temp_dir().join(format!(
            "mir3-global-capability-{}-{}",
            std::process::id(),
            CAPABILITY_TEST_NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("项目");
        let shop_path = "引擎/Mir200/Envir/shop.txt";
        let item_path = "引擎/Mir200/Envir/cfg_item.txt";
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Envir")).unwrap();
        fs::write(root.join(shop_path), "shopId=1\nprice=1\n").unwrap();
        fs::write(root.join(item_path), "cfg_item=1\nitemId=1\nstackLimit=1\n").unwrap();
        let store = governance_test_store(&base);
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let composite_id = "global-pricing-and-items";
        let cases = [
            (
                "shop",
                "batch-price-shop",
                shop_path,
                "shopId=1\nprice=2\n",
                serde_json::json!({
                    "operation":"batch-price-shop","resourceIds":["shop:1"],
                    "changes":{"price":2},"expectedRevision":0
                }),
            ),
            (
                "item",
                "batch-edit-item",
                item_path,
                "cfg_item=1\nitemId=1\nstackLimit=2\n",
                serde_json::json!({
                    "operation":"batch-edit-item","resourceIds":["item:1"],
                    "changes":{"stackLimit":2},"expectedRevision":0
                }),
            ),
        ];
        let mut confirmations = Vec::new();
        let mut draft_ids = Vec::new();
        for (system_id, operation_id, path, content, parameters) in cases {
            let draft = store.open_draft(&project.id, operation_id).unwrap();
            store
                .bind_draft_domain(
                    &project.id,
                    &draft.id,
                    system_id,
                    "1.2.0",
                    Some(composite_id),
                )
                .unwrap();
            store
                .patch_draft(
                    &project.id,
                    &draft.id,
                    0,
                    &[crate::DraftChangeInput {
                        path: path.to_string(),
                        content: Some(content.to_string()),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )
                .unwrap();
            let evidence = store
                .record_draft_operation_evidence(
                    &project.id,
                    &draft.id,
                    operation_id,
                    &parameters,
                    0,
                    1,
                )
                .unwrap();
            let change_hash = store
                .draft_change_evidence_hash(&project.id, &draft.id)
                .unwrap();
            store
                .seal_draft_operation_replay(
                    &project.id,
                    &draft.id,
                    evidence.sequence,
                    &change_hash,
                )
                .unwrap();
            let preview = store.preview_draft(&project.id, &draft.id).unwrap();
            confirmations.push(crate::CompositeDraftConfirmation {
                draft_id: draft.id.clone(),
                expected_revision: 1,
                expected_diff_hash: preview.diff_hash,
            });
            draft_ids.push(draft.id);
        }
        let result = store
            .apply_composite_drafts(&project.id, composite_id, &confirmations)
            .unwrap();
        let receipts = confirmations
            .iter()
            .map(|confirmation| {
                store
                    .record_applied_draft_receipt(
                        &project.id,
                        &confirmation.draft_id,
                        &confirmation.expected_diff_hash,
                        &result.snapshot,
                    )
                    .unwrap()
                    .unwrap()
            })
            .collect();
        (base, store, project.id, receipts)
    }

    #[test]
    fn successful_multi_operation_receipt_compiles_replays_and_activates() {
        let (base, store, project_id, receipt) =
            applied_shop_source(&["batch-price-shop", "batch-price-shop"]);
        let capability = store
            .compile_user_capability(
                &project_id,
                &CapabilityCompileRequest {
                    receipt_id: receipt.id,
                    id: "reprice-shop".to_string(),
                    name: "Reprice shop".to_string(),
                    description: "Two safe pricing operations".to_string(),
                },
            )
            .unwrap();
        assert_eq!(capability.version, "0.1.0");
        assert_eq!(capability.steps.as_array().unwrap().len(), 2);
        assert!(capability.parameter_schema["properties"]["step0"].is_object());
        let active = store
            .set_user_capability_status(&project_id, &capability.id, &capability.version, "active")
            .unwrap();
        assert_eq!(active.status, "active");
        let disabled = store
            .set_user_capability_status(
                &project_id,
                &capability.id,
                &capability.version,
                "disabled",
            )
            .unwrap();
        assert_eq!(disabled.status, "disabled");
        let restored = store
            .set_user_capability_status(&project_id, &capability.id, &capability.version, "active")
            .unwrap();
        assert_eq!(restored.status, "active");
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn capability_rollback_atomically_restores_a_disabled_project_version() {
        let (base, store, project_id, receipt) = applied_shop_source(&["batch-price-shop"]);
        let current = store
            .compile_user_capability(
                &project_id,
                &CapabilityCompileRequest {
                    receipt_id: receipt.id,
                    id: "rollback-shop-pricing".to_string(),
                    name: "Rollback shop pricing".to_string(),
                    description: String::new(),
                },
            )
            .unwrap();
        let mut previous = current.clone();
        previous.version = "0.0.9".to_string();
        previous.status = "disabled".to_string();
        let mut oldest = current.clone();
        oldest.version = "0.0.8".to_string();
        oldest.status = "active".to_string();
        let mut connection = store.project_connection(&project_id).unwrap();
        let transaction = connection.transaction().unwrap();
        insert_project_capability(&transaction, &previous).unwrap();
        insert_project_capability(&transaction, &oldest).unwrap();
        transaction.commit().unwrap();
        store
            .set_user_capability_status(&project_id, &current.id, &current.version, "active")
            .unwrap();
        let active_after_activation = store
            .list_user_capabilities(&project_id, Some("shop"))
            .unwrap()
            .into_iter()
            .filter(|capability| capability.id == current.id && capability.status == "active")
            .collect::<Vec<_>>();
        assert_eq!(active_after_activation.len(), 1);
        assert_eq!(active_after_activation[0].version, current.version);
        let restored = store
            .rollback_user_capability(
                &project_id,
                &CapabilityRollbackRequest {
                    capability_id: current.id.clone(),
                    scope: "project".to_string(),
                    from_version: current.version.clone(),
                    to_version: previous.version.clone(),
                },
            )
            .unwrap();
        assert_eq!(restored.version, "0.0.9");
        assert_eq!(restored.status, "active");
        let active_after_rollback = store
            .list_user_capabilities(&project_id, Some("shop"))
            .unwrap()
            .into_iter()
            .filter(|capability| capability.id == current.id && capability.status == "active")
            .collect::<Vec<_>>();
        assert_eq!(active_after_rollback.len(), 1);
        assert_eq!(active_after_rollback[0].version, previous.version);
        assert_eq!(
            store
                .get_project_capability(&project_id, &current.id, Some(&current.version))
                .unwrap()
                .status,
            "disabled"
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn shared_capability_activation_and_rollback_keep_one_active_of_three_versions() {
        let (base, store, project_id, receipt) = applied_shop_source(&["batch-price-shop"]);
        let compiled = store
            .compile_user_capability(
                &project_id,
                &CapabilityCompileRequest {
                    receipt_id: receipt.id,
                    id: "shared-three-version-pricing".to_string(),
                    name: "Shared three version pricing".to_string(),
                    description: String::new(),
                },
            )
            .unwrap();
        let mut latest = compiled.clone();
        latest.scope = "personal".to_string();
        latest.status = "disabled".to_string();
        let mut middle = latest.clone();
        middle.version = "0.0.9".to_string();
        middle.status = "active".to_string();
        let mut oldest = latest.clone();
        oldest.version = "0.0.8".to_string();

        let mut connection = store.registry().unwrap();
        let transaction = connection.transaction().unwrap();
        insert_shared_capability(&transaction, &project_id, &latest).unwrap();
        insert_shared_capability(&transaction, &project_id, &middle).unwrap();
        insert_shared_capability(&transaction, &project_id, &oldest).unwrap();
        transaction.commit().unwrap();

        store
            .set_shared_capability_status("personal", &latest.id, &latest.version, "active")
            .unwrap();
        let active_after_activation = store
            .list_shared_capabilities(Some("shop"))
            .unwrap()
            .into_iter()
            .map(|(_, capability)| capability)
            .filter(|capability| capability.id == latest.id && capability.status == "active")
            .collect::<Vec<_>>();
        assert_eq!(active_after_activation.len(), 1);
        assert_eq!(active_after_activation[0].version, latest.version);

        let restored = store
            .rollback_user_capability(
                &project_id,
                &CapabilityRollbackRequest {
                    capability_id: latest.id.clone(),
                    scope: "personal".to_string(),
                    from_version: latest.version.clone(),
                    to_version: middle.version.clone(),
                },
            )
            .unwrap();
        assert_eq!(restored.version, middle.version);
        let active_after_rollback = store
            .list_shared_capabilities(Some("shop"))
            .unwrap()
            .into_iter()
            .map(|(_, capability)| capability)
            .filter(|capability| capability.id == latest.id && capability.status == "active")
            .collect::<Vec<_>>();
        assert_eq!(active_after_rollback.len(), 1);
        assert_eq!(active_after_rollback[0].version, middle.version);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn capability_promotion_reuses_across_projects_and_resolves_scope_priority() {
        let (base, store, project_id, receipt) = applied_shop_source(&["batch-price-shop"]);
        let capability = store
            .compile_user_capability(
                &project_id,
                &CapabilityCompileRequest {
                    receipt_id: receipt.id,
                    id: "shared-shop-pricing".to_string(),
                    name: "Shared shop pricing".to_string(),
                    description: "Reusable pricing workflow".to_string(),
                },
            )
            .unwrap();
        store
            .set_user_capability_status(&project_id, &capability.id, &capability.version, "active")
            .unwrap();
        for scope in ["team", "personal"] {
            store
                .promote_user_capability(
                    &project_id,
                    &CapabilityPromotionRequest {
                        capability_id: capability.id.clone(),
                        version: capability.version.clone(),
                        target_scope: scope.to_string(),
                    },
                )
                .unwrap();
        }

        let second_root = base.join("项目二");
        fs::create_dir_all(second_root.join("客户端/dev")).unwrap();
        fs::create_dir_all(second_root.join("引擎/Mir200")).unwrap();
        let second = store.import_project(&second_root).unwrap();
        let shared = store
            .resolve_user_capabilities(&second.id, Some("shop"))
            .unwrap();
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].resolved_scope, "personal");
        assert_eq!(shared[0].shadowed_scopes, vec!["team"]);

        let mut local = capability.clone();
        local.scope = "project".to_string();
        local.status = "disabled".to_string();
        let mut connection = store.project_connection(&second.id).unwrap();
        let transaction = connection.transaction().unwrap();
        insert_project_capability(&transaction, &local).unwrap();
        transaction.commit().unwrap();
        let disabled_local_resolution = store
            .resolve_user_capability(&second.id, &capability.id, Some(&capability.version))
            .unwrap();
        assert_eq!(disabled_local_resolution.resolved_scope, "personal");
        assert_eq!(
            store
                .get_user_capability(&second.id, &capability.id, Some(&capability.version))
                .unwrap()
                .scope,
            "personal"
        );
        store
            .project_connection(&second.id)
            .unwrap()
            .execute(
                "UPDATE user_capabilities SET status='active' WHERE id=?1 AND version=?2",
                params![local.id, local.version],
            )
            .unwrap();
        let resolved = store
            .resolve_user_capabilities(&second.id, Some("shop"))
            .unwrap();
        assert_eq!(resolved[0].resolved_scope, "project");
        assert_eq!(
            resolved[0].shadowed_scopes,
            vec!["personal".to_string(), "team".to_string()]
        );

        store
            .registry()
            .unwrap()
            .execute(
                "UPDATE shared_user_capabilities SET description='conflict' WHERE scope='team' AND id=?1 AND version=?2",
                params![capability.id,capability.version],
            )
            .unwrap();
        assert!(store
            .promote_user_capability(
                &project_id,
                &CapabilityPromotionRequest {
                    capability_id: capability.id.clone(),
                    version: capability.version.clone(),
                    target_scope: "team".to_string(),
                },
            )
            .unwrap_err()
            .starts_with("CAPABILITY_PROMOTION_CONFLICT:"));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn composite_receipts_compile_and_replay_a_scoped_global_workflow() {
        let (base, store, project_id, receipts) = applied_cross_system_source();
        let capability = store
            .compile_global_workflow_capability(
                &project_id,
                &GlobalCapabilityCompileRequest {
                    receipt_ids: receipts.iter().map(|receipt| receipt.id.clone()).collect(),
                    id: "global-shop-item-update".to_string(),
                    name: "Global shop and item update".to_string(),
                    description: "Atomic two-system workflow".to_string(),
                },
            )
            .unwrap();
        assert_eq!(capability.system_id, "__global__");
        assert_eq!(capability.write_systems.len(), 2);
        assert!(capability.steps.as_array().unwrap().iter().all(|step| {
            step.get("systemId")
                .and_then(serde_json::Value::as_str)
                .is_some()
                && step
                    .get("pluginVersion")
                    .and_then(serde_json::Value::as_str)
                    == Some("1.2.0")
                && step
                    .get("parameterKey")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|value| value.starts_with("system"))
                && step
                    .get("operationIndex")
                    .and_then(serde_json::Value::as_u64)
                    .is_some()
        }));
        let active = store
            .set_user_capability_status(&project_id, &capability.id, &capability.version, "active")
            .unwrap();
        assert_eq!(active.status, "active");
        let invocation_composite = "replay-global-workflow";
        for system_id in ["shop", "item"] {
            let draft = store
                .open_draft(&project_id, &format!("invoke {system_id}"))
                .unwrap();
            store
                .bind_draft_domain(
                    &project_id,
                    &draft.id,
                    system_id,
                    "1.2.0",
                    Some(invocation_composite),
                )
                .unwrap();
        }
        assert!(store
            .validate_global_capability_for_composite(
                &project_id,
                invocation_composite,
                &capability.id,
                Some(&capability.version),
            )
            .is_ok());

        let mut non_atomic = receipts
            .iter()
            .map(|receipt| receipt.id.clone())
            .collect::<Vec<_>>();
        let mut forged = receipts[1].clone();
        forged.id = "forged-non-atomic".to_string();
        forged.evidence["snapshotId"] = serde_json::json!("another-snapshot");
        store.save_task_receipt(&project_id, &forged).unwrap();
        non_atomic[1] = forged.id;
        assert!(store
            .compile_global_workflow_capability(
                &project_id,
                &GlobalCapabilityCompileRequest {
                    receipt_ids: non_atomic,
                    id: "rejected-global".to_string(),
                    name: "Rejected".to_string(),
                    description: String::new(),
                },
            )
            .unwrap_err()
            .starts_with("GLOBAL_CAPABILITY_NOT_ATOMIC:"));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn composite_transaction_restores_same_system_first_step_after_second_step_failure() {
        let (base, store, project_id, composite_id, bindings) = checkpoint_failure_fixture();
        let shop = bindings
            .iter()
            .find(|binding| binding.system_id == "shop")
            .unwrap();
        let result = store.with_composite_draft_transaction(
            &project_id,
            &composite_id,
            |_| -> Result<(), String> {
                store.patch_draft(
                    &project_id,
                    &shop.draft_id,
                    0,
                    &[crate::DraftChangeInput {
                        path: "引擎/Mir200/Envir/shop.txt".to_string(),
                        content: Some("shopId=1\nprice=2\n".to_string()),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )?;
                store.record_draft_operation_evidence(
                    &project_id,
                    &shop.draft_id,
                    "batch-price-shop",
                    &serde_json::json!({
                        "operation":"batch-price-shop",
                        "resourceIds":["shop:1"],
                        "changes":{"price":2},
                        "expectedRevision":0
                    }),
                    0,
                    1,
                )?;
                Err("CAPABILITY_RUNTIME_FAILURE: second shop step".to_string())
            },
        );
        assert!(result
            .unwrap_err()
            .starts_with("CAPABILITY_RUNTIME_FAILURE:"));
        assert_composite_checkpoint_restored(&store, &project_id, &bindings);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn composite_transaction_restores_cross_system_mid_run_failure() {
        let (base, store, project_id, composite_id, bindings) = checkpoint_failure_fixture();
        let shop = bindings
            .iter()
            .find(|binding| binding.system_id == "shop")
            .unwrap();
        let result = store.with_composite_draft_transaction(
            &project_id,
            &composite_id,
            |_| -> Result<(), String> {
                store.patch_draft(
                    &project_id,
                    &shop.draft_id,
                    0,
                    &[crate::DraftChangeInput {
                        path: "引擎/Mir200/Envir/shop.txt".to_string(),
                        content: Some("shopId=1\nprice=2\n".to_string()),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )?;
                store.record_draft_operation_evidence(
                    &project_id,
                    &shop.draft_id,
                    "batch-price-shop",
                    &serde_json::json!({
                        "operation":"batch-price-shop",
                        "resourceIds":["shop:1"],
                        "changes":{"price":2},
                        "expectedRevision":0
                    }),
                    0,
                    1,
                )?;
                Err("CAPABILITY_RUNTIME_FAILURE: item step".to_string())
            },
        );
        assert!(result
            .unwrap_err()
            .starts_with("CAPABILITY_RUNTIME_FAILURE:"));
        assert_composite_checkpoint_restored(&store, &project_id, &bindings);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn composite_transaction_reservation_preserves_a_concurrent_writer_after_rollback() {
        let (base, store, project_id, composite_id, bindings) = checkpoint_failure_fixture();
        let shop = bindings
            .iter()
            .find(|binding| binding.system_id == "shop")
            .unwrap()
            .clone();
        let (reserved_tx, reserved_rx) = std::sync::mpsc::channel();
        let (retry_tx, retry_rx) = std::sync::mpsc::channel();
        let (completed_tx, completed_rx) = std::sync::mpsc::channel();
        let writer_store = store.clone();
        let writer_project_id = project_id.clone();
        let writer_draft_id = shop.draft_id.clone();
        let mut writer = None;
        let result = store.with_composite_draft_transaction(
            &project_id,
            &composite_id,
            |_| -> Result<(), String> {
                writer = Some(std::thread::spawn(move || {
                    let first = writer_store.patch_draft(
                        &writer_project_id,
                        &writer_draft_id,
                        0,
                        &[crate::DraftChangeInput {
                            path: "引擎/Mir200/Envir/shop.txt".to_string(),
                            content: Some("shopId=1\nprice=9\n".to_string()),
                            deleted: false,
                            expected_sha256: None,
                        }],
                    );
                    reserved_tx.send(first.unwrap_err()).unwrap();
                    retry_rx.recv().unwrap();
                    let revision = writer_store
                        .patch_draft(
                            &writer_project_id,
                            &writer_draft_id,
                            0,
                            &[crate::DraftChangeInput {
                                path: "引擎/Mir200/Envir/shop.txt".to_string(),
                                content: Some("shopId=1\nprice=9\n".to_string()),
                                deleted: false,
                                expected_sha256: None,
                            }],
                        )
                        .map(|preview| preview.draft.revision);
                    completed_tx.send(revision).unwrap();
                }));
                assert!(reserved_rx
                    .recv()
                    .unwrap()
                    .starts_with("DRAFT_MUTATION_RESERVED:"));
                store.patch_draft(
                    &project_id,
                    &shop.draft_id,
                    0,
                    &[crate::DraftChangeInput {
                        path: "引擎/Mir200/Envir/shop.txt".to_string(),
                        content: Some("shopId=1\nprice=2\n".to_string()),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )?;
                Err("CAPABILITY_RUNTIME_FAILURE: rollback before writer retry".to_string())
            },
        );
        assert!(result
            .unwrap_err()
            .starts_with("CAPABILITY_RUNTIME_FAILURE:"));
        retry_tx.send(()).unwrap();
        assert_eq!(completed_rx.recv().unwrap().unwrap(), 1);
        writer.take().unwrap().join().unwrap();
        let preview = store.preview_draft(&project_id, &shop.draft_id).unwrap();
        assert_eq!(preview.draft.revision, 1);
        assert_eq!(preview.changes.len(), 1);
        assert_eq!(
            String::from_utf8(
                store
                    .draft_change_bytes(&project_id, &shop.draft_id, "引擎/Mir200/Envir/shop.txt",)
                    .unwrap()
                    .unwrap()
            )
            .unwrap(),
            "shopId=1\nprice=9\n"
        );
        assert!(store
            .list_draft_operation_evidence(&project_id, &shop.draft_id)
            .unwrap()
            .is_empty());
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn composite_transaction_os_lock_blocks_an_independent_store() {
        let (base, store, project_id, composite_id, bindings) = checkpoint_failure_fixture();
        let independent = governance_test_store(&base);
        let shop = bindings
            .iter()
            .find(|binding| binding.system_id == "shop")
            .unwrap();
        let result = store.with_composite_draft_transaction(
            &project_id,
            &composite_id,
            |_| -> Result<(), String> {
                let competing = independent
                    .patch_draft(
                        &project_id,
                        &shop.draft_id,
                        0,
                        &[crate::DraftChangeInput {
                            path: "引擎/Mir200/Envir/shop.txt".to_string(),
                            content: Some("shopId=1\nprice=8\n".to_string()),
                            deleted: false,
                            expected_sha256: None,
                        }],
                    )
                    .unwrap_err();
                assert!(competing.starts_with("DRAFT_MUTATION_RESERVED:"));
                store.patch_draft(
                    &project_id,
                    &shop.draft_id,
                    0,
                    &[crate::DraftChangeInput {
                        path: "引擎/Mir200/Envir/shop.txt".to_string(),
                        content: Some("shopId=1\nprice=2\n".to_string()),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )?;
                Err("CAPABILITY_RUNTIME_FAILURE: independent store remained isolated".to_string())
            },
        );
        assert!(result
            .unwrap_err()
            .starts_with("CAPABILITY_RUNTIME_FAILURE:"));
        let retried = independent
            .patch_draft(
                &project_id,
                &shop.draft_id,
                0,
                &[crate::DraftChangeInput {
                    path: "引擎/Mir200/Envir/shop.txt".to_string(),
                    content: Some("shopId=1\nprice=8\n".to_string()),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap();
        assert_eq!(retried.draft.revision, 1);
        assert_eq!(retried.changes.len(), 1);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn legacy_clone_holds_cross_process_lock_until_copy_finishes() {
        let (base, store, project_id, _, bindings) = checkpoint_failure_fixture();
        let source = bindings
            .iter()
            .find(|binding| binding.system_id == "shop")
            .unwrap();
        store
            .patch_draft(
                &project_id,
                &source.draft_id,
                0,
                &[crate::DraftChangeInput {
                    path: "引擎/Mir200/Envir/shop.txt".to_string(),
                    content: Some("shopId=1\nprice=2\n".to_string()),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap();
        store
            .project_connection(&project_id)
            .unwrap()
            .execute(
                "UPDATE draft_domains SET legacy=1 WHERE draft_id=?1",
                [&source.draft_id],
            )
            .unwrap();
        let current = fs::read(base.join("项目/引擎/Mir200/Envir/shop.txt")).unwrap();
        let (entered, release) = governance_copy_gate(&store);
        let worker_store = store.clone();
        let worker_project_id = project_id.clone();
        let legacy_draft_id = source.draft_id.clone();
        let worker = std::thread::spawn(move || {
            worker_store.clone_legacy_draft(
                &worker_project_id,
                &LegacyDraftCloneRequest {
                    legacy_draft_id,
                    system_id: "shop".to_string(),
                    plugin_version: "1.2.0".to_string(),
                    expected_sources: BTreeMap::from([(
                        "引擎/Mir200/Envir/shop.txt".to_string(),
                        hash_bytes(&current),
                    )]),
                    intent: "legacy locked clone".to_string(),
                },
            )
        });
        entered.wait();
        let independent = governance_test_store(&base);
        let clone = independent
            .list_drafts(&project_id)
            .unwrap()
            .into_iter()
            .find(|draft| draft.intent == "legacy locked clone")
            .unwrap();
        let competing = independent
            .patch_draft(
                &project_id,
                &clone.id,
                0,
                &[crate::DraftChangeInput {
                    path: "引擎/Mir200/Envir/shop.txt".to_string(),
                    content: Some("shopId=1\nprice=9\n".to_string()),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap_err();
        assert!(competing.starts_with("DRAFT_MUTATION_RESERVED:"));
        release.wait();
        assert_eq!(worker.join().unwrap().unwrap().draft.revision, 1);
        *store.governance_copy_test_gate.lock().unwrap() = None;
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn capability_replay_holds_cross_process_lock_until_cleanup_finishes() {
        let (base, store, project_id, receipt) = applied_shop_source(&["batch-price-shop"]);
        let (entered, release) = governance_copy_gate(&store);
        let worker_store = store.clone();
        let worker_project_id = project_id.clone();
        let worker = std::thread::spawn(move || {
            worker_store.compile_user_capability(
                &worker_project_id,
                &CapabilityCompileRequest {
                    receipt_id: receipt.id,
                    id: "locked-capability-replay".to_string(),
                    name: "Locked replay".to_string(),
                    description: String::new(),
                },
            )
        });
        entered.wait();
        let independent = governance_test_store(&base);
        let replay = independent
            .list_drafts(&project_id)
            .unwrap()
            .into_iter()
            .find(|draft| draft.intent == "capability isolated replay")
            .unwrap();
        let competing = independent
            .patch_draft(
                &project_id,
                &replay.id,
                0,
                &[crate::DraftChangeInput {
                    path: "引擎/Mir200/Envir/shop.txt".to_string(),
                    content: Some("shopId=1\nprice=9\n".to_string()),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap_err();
        assert!(competing.starts_with("DRAFT_MUTATION_RESERVED:"));
        release.wait();
        assert_eq!(
            worker.join().unwrap().unwrap().id,
            "locked-capability-replay"
        );
        *store.governance_copy_test_gate.lock().unwrap() = None;
        assert_eq!(
            independent
                .get_draft(&project_id, &replay.id)
                .unwrap()
                .status,
            DraftStatus::Discarded
        );
        fs::remove_dir_all(base).ok();
    }

    fn governance_copy_gate(
        store: &DomainStore,
    ) -> (Arc<std::sync::Barrier>, Arc<std::sync::Barrier>) {
        let entered = Arc::new(std::sync::Barrier::new(2));
        let release = Arc::new(std::sync::Barrier::new(2));
        *store.governance_copy_test_gate.lock().unwrap() = Some((entered.clone(), release.clone()));
        (entered, release)
    }

    fn checkpoint_failure_fixture() -> (
        std::path::PathBuf,
        DomainStore,
        String,
        String,
        Vec<CompositeDraftBinding>,
    ) {
        let base = std::env::temp_dir().join(format!(
            "mir3-composite-capability-rollback-{}-{}",
            std::process::id(),
            CAPABILITY_TEST_NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("项目");
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Envir")).unwrap();
        fs::write(
            root.join("引擎/Mir200/Envir/shop.txt"),
            "shopId=1\nprice=1\n",
        )
        .unwrap();
        fs::write(
            root.join("引擎/Mir200/Envir/cfg_item.txt"),
            "itemId=1\nstackLimit=1\n",
        )
        .unwrap();
        let store = governance_test_store(&base);
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let composite_id = "checkpoint-failure".to_string();
        for system_id in ["shop", "item"] {
            let draft = store.open_draft(&project.id, system_id).unwrap();
            store
                .bind_draft_domain(
                    &project.id,
                    &draft.id,
                    system_id,
                    "1.2.0",
                    Some(&composite_id),
                )
                .unwrap();
        }
        let bindings = store
            .list_composite_draft_bindings(&project.id, &composite_id)
            .unwrap();
        (base, store, project.id, composite_id, bindings)
    }

    fn assert_composite_checkpoint_restored(
        store: &DomainStore,
        project_id: &str,
        bindings: &[CompositeDraftBinding],
    ) {
        for binding in bindings {
            let draft = store.get_draft(project_id, &binding.draft_id).unwrap();
            assert_eq!(draft.revision, 0);
            assert!(store
                .preview_draft(project_id, &binding.draft_id)
                .unwrap()
                .changes
                .is_empty());
            assert!(store
                .list_draft_operation_evidence(project_id, &binding.draft_id)
                .unwrap()
                .is_empty());
        }
    }

    #[test]
    fn governance_migration_versions_capabilities_and_snapshot_restores_capability_and_memory() {
        let (base, store, project_id, receipt) = applied_shop_source(&["batch-price-shop"]);
        let capability = store
            .compile_user_capability(
                &project_id,
                &CapabilityCompileRequest {
                    receipt_id: receipt.id,
                    id: "migration-shop-pricing".to_string(),
                    name: "Migration shop pricing".to_string(),
                    description: String::new(),
                },
            )
            .unwrap();
        store
            .set_user_capability_status(&project_id, &capability.id, &capability.version, "active")
            .unwrap();
        store
            .promote_user_capability(
                &project_id,
                &CapabilityPromotionRequest {
                    capability_id: capability.id.clone(),
                    version: capability.version.clone(),
                    target_scope: "personal".to_string(),
                },
            )
            .unwrap();
        let memory_id = store
            .list_memory_candidates(&project_id, Some("shop"))
            .unwrap()
            .into_iter()
            .find(|memory| memory.source_task_id.starts_with("draft:"))
            .unwrap()
            .id;
        let snapshot = store.snapshot_domain_governance("shop").unwrap();
        let report = store
            .migrate_domain_governance("shop", "1.2.0", "1.2.0")
            .unwrap();
        assert!(report.compatible);
        assert_eq!(report.status, "applied");
        assert!(report
            .migrated_capabilities
            .iter()
            .any(|value| value.contains("0.1.0->0.1.1")));
        assert_eq!(
            store
                .get_project_capability(&project_id, &capability.id, Some("0.1.0"))
                .unwrap()
                .status,
            "deprecated"
        );
        assert_eq!(
            store
                .get_project_capability(&project_id, &capability.id, Some("0.1.1"))
                .unwrap()
                .status,
            "active"
        );

        store.restore_domain_governance_snapshot(&snapshot).unwrap();
        assert_eq!(
            store
                .get_project_capability(&project_id, &capability.id, Some("0.1.0"))
                .unwrap()
                .status,
            "active"
        );
        assert!(store
            .get_project_capability(&project_id, &capability.id, Some("0.1.1"))
            .is_err());
        assert_eq!(
            store
                .get_domain_memory(&project_id, &memory_id)
                .unwrap()
                .plugin_version,
            "1.2.0"
        );
        let shared = store
            .shared_capability("personal", &capability.id, Some("0.1.0"))
            .unwrap()
            .unwrap()
            .1;
        assert_eq!(shared.status, "active");
        assert!(store
            .shared_capability("personal", &capability.id, Some("0.1.1"))
            .unwrap()
            .is_none());
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn capability_compilation_rejects_forgery_failure_and_replay_mismatch() {
        let (base, store, project_id, receipt) = applied_shop_source(&["batch-price-shop"]);
        let forged = UserCapability {
            id: "forged".to_string(),
            version: "0.1.0".to_string(),
            system_id: "shop".to_string(),
            scope: "project".to_string(),
            name: "Forged".to_string(),
            description: String::new(),
            parameter_schema: serde_json::json!({"type":"object"}),
            steps: serde_json::json!([{"type":"domain-operation","operation":"batch-price-shop"}]),
            read_systems: vec!["shop".to_string()],
            write_systems: vec!["shop".to_string()],
            status: "draft".to_string(),
            source_task_id: receipt.task_id.clone(),
            created_at: now_millis(),
            updated_at: now_millis(),
        };
        assert!(store
            .save_user_capability(&project_id, &forged)
            .unwrap_err()
            .starts_with("CAPABILITY_STEP_VERSION_REQUIRED:"));

        let failed = TaskReceipt {
            id: "failed-receipt".to_string(),
            status: "failed".to_string(),
            ..receipt.clone()
        };
        store.save_task_receipt(&project_id, &failed).unwrap();
        let request = CapabilityCompileRequest {
            receipt_id: failed.id,
            id: "from-failed".to_string(),
            name: "Rejected".to_string(),
            description: String::new(),
        };
        assert!(store
            .compile_user_capability(&project_id, &request)
            .unwrap_err()
            .starts_with("CAPABILITY_RECEIPT_NOT_SUCCESSFUL:"));

        let mut mismatched = receipt.clone();
        mismatched.evidence["diffHash"] = serde_json::json!("forged");
        store.save_task_receipt(&project_id, &mismatched).unwrap();
        let mismatch_request = CapabilityCompileRequest {
            receipt_id: mismatched.id,
            id: "mismatch".to_string(),
            name: "Mismatch".to_string(),
            description: String::new(),
        };
        assert!(store
            .compile_user_capability(&project_id, &mismatch_request)
            .unwrap_err()
            .starts_with("CAPABILITY_RECEIPT_DIFF_MISMATCH:"));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn activation_rejects_tampered_replay_parameters() {
        let (base, store, project_id, receipt) = applied_shop_source(&["batch-price-shop"]);
        let capability = store
            .compile_user_capability(
                &project_id,
                &CapabilityCompileRequest {
                    receipt_id: receipt.id.clone(),
                    id: "tamper-replay".to_string(),
                    name: "Tamper replay".to_string(),
                    description: String::new(),
                },
            )
            .unwrap();
        store
            .project_connection(&project_id)
            .unwrap()
            .execute(
                "UPDATE draft_operation_evidence SET parameters='{}' WHERE draft_id=?1",
                [receipt.draft_id.as_deref().unwrap()],
            )
            .unwrap();
        let error = store
            .set_user_capability_status(&project_id, &capability.id, &capability.version, "active")
            .unwrap_err();
        assert!(
            error.starts_with("CAPABILITY_EVIDENCE_CHAIN_INVALID:")
                || error.starts_with("CAPABILITY_REPLAY_EVIDENCE_MISMATCH:")
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn capability_versions_use_semver_and_incompatible_domain_version_fails_closed() {
        let (base, store, project_id, receipt) = applied_shop_source(&["batch-price-shop"]);
        let compiled = store
            .compile_user_capability(
                &project_id,
                &CapabilityCompileRequest {
                    receipt_id: receipt.id,
                    id: "semver-cap".to_string(),
                    name: "SemVer".to_string(),
                    description: String::new(),
                },
            )
            .unwrap();
        store
            .set_user_capability_status(&project_id, &compiled.id, "0.1.0", "active")
            .unwrap();
        let connection = store.project_connection(&project_id).unwrap();
        for version in ["0.9.0", "0.10.0"] {
            connection
                .execute(
                    "INSERT INTO user_capabilities(id,version,system_id,scope,name,description,parameter_schema,steps,read_systems,write_systems,status,source_task_id,created_at,updated_at)
                     SELECT id,?2,system_id,scope,name,description,parameter_schema,steps,read_systems,write_systems,status,source_task_id,created_at,updated_at FROM user_capabilities WHERE id=?1 AND version='0.1.0'",
                    params![compiled.id, version],
                )
                .unwrap();
        }
        assert_eq!(
            store
                .get_user_capability(&project_id, &compiled.id, None)
                .unwrap()
                .version,
            "0.10.0"
        );
        let invocation = store.open_draft(&project_id, "invoke").unwrap();
        store
            .bind_draft_domain(&project_id, &invocation.id, "shop", "1.2.0", None)
            .unwrap();
        connection
            .execute(
                "UPDATE draft_domains SET plugin_version='9.0.0' WHERE draft_id=?1",
                [&invocation.id],
            )
            .unwrap();
        assert!(store
            .validate_user_capability_version_for_draft(
                &project_id,
                &invocation.id,
                &compiled.id,
                Some("0.1.0"),
            )
            .unwrap_err()
            .starts_with("CAPABILITY_DOMAIN_VERSION_INCOMPATIBLE:"));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn receipt_and_memory_candidate_commit_atomically() {
        let base = std::env::temp_dir().join(format!(
            "mir3-receipt-atomic-{}-{}",
            std::process::id(),
            CAPABILITY_TEST_NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("项目");
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200")).unwrap();
        let store = governance_test_store(&base);
        let project = store.import_project(&root).unwrap();
        store
            .project_connection(&project.id)
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_memory_candidate BEFORE INSERT ON domain_memories
                 BEGIN SELECT RAISE(ABORT, 'memory failure'); END;",
            )
            .unwrap();
        let receipt = TaskReceipt {
            id: "atomic-receipt".to_string(),
            task_id: "atomic-task".to_string(),
            system_id: "shop".to_string(),
            summary: "atomic".to_string(),
            status: "applied".to_string(),
            draft_id: None,
            plugin_versions: serde_json::json!({"shop":"1.2.0"}),
            evidence: serde_json::json!({"diffHash":"x"}),
            created_at: now_millis(),
        };
        assert!(store
            .save_task_receipt(&project.id, &receipt)
            .unwrap_err()
            .starts_with("TASK_MEMORY_WRITE_FAILED:"));
        assert!(store
            .list_task_receipts(&project.id, Some("shop"))
            .unwrap()
            .is_empty());
        fs::remove_dir_all(base).ok();
    }
}
