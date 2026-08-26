//! 任务回执、用户能力、系统会话和作用域凭证的项目外治理数据。

use crate::{now_millis, DomainStore, DraftPreview, Snapshot};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path};

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
        self.bind_draft_domain(
            project_id,
            &draft.id,
            &request.system_id,
            &request.plugin_version,
            None,
        )?;
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
        transaction
            .execute(
                "UPDATE drafts SET revision=1,updated_at=?2 WHERE id=?1",
                params![draft.id, now_millis()],
            )
            .map_err(|error| format!("DRAFT_LEGACY_CLONE_FAILED: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("DRAFT_LEGACY_CLONE_FAILED: {error}"))?;
        self.preview_draft(project_id, &draft.id)
    }

    pub fn save_task_receipt(
        &self,
        project_id: &str,
        receipt: &TaskReceipt,
    ) -> Result<TaskReceipt, String> {
        self.ensure_known_system(&receipt.system_id)?;
        self.project_connection(project_id)?
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
        if !matches!(receipt.status.as_str(), "applied" | "completed" | "success") {
            return Ok(receipt.clone());
        }
        let plugin_version = receipt
            .plugin_versions
            .get("domain")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("1.0.0")
            .to_string();
        let candidate = DomainMemory {
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
        };
        match self.get_domain_memory(project_id, &candidate.id) {
            Ok(_) => {}
            Err(error) if error.starts_with("MEMORY_NOT_FOUND:") => {
                self.save_domain_memory(project_id, &candidate)?;
            }
            Err(error) => return Err(error),
        }
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
        self.ensure_known_system(&capability.system_id)?;
        let manifest = self
            .list_domain_systems()?
            .into_iter()
            .find(|manifest| manifest.system_id == capability.system_id)
            .ok_or_else(|| format!("DOMAIN_SYSTEM_NOT_FOUND: {}", capability.system_id))?;
        for operation in capability
            .steps
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|step| step.get("operation").and_then(serde_json::Value::as_str))
        {
            if !manifest
                .operations
                .iter()
                .any(|registered| registered.id == operation)
            {
                return Err(format!(
                    "CAPABILITY_OPERATION_NOT_REGISTERED: {}:{operation}",
                    capability.system_id
                ));
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
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("CAPABILITY_LIST_FAILED: {error}"))
    }

    pub fn get_user_capability(
        &self,
        project_id: &str,
        capability_id: &str,
        version: Option<&str>,
    ) -> Result<UserCapability, String> {
        self.project_connection(project_id)?
            .query_row(
                "SELECT id,version,system_id,scope,name,description,parameter_schema,steps,read_systems,write_systems,status,source_task_id,created_at,updated_at
                 FROM user_capabilities WHERE id=?1 AND (?2 IS NULL OR version=?2) ORDER BY version DESC LIMIT 1",
                params![capability_id, version],
                row_to_capability,
            )
            .optional()
            .map_err(|error| format!("CAPABILITY_READ_FAILED: {error}"))?
            .ok_or_else(|| format!("CAPABILITY_NOT_FOUND: {capability_id}"))
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
        let changed = self
            .project_connection(project_id)?
            .execute(
                "UPDATE user_capabilities SET status=?3,updated_at=?4 WHERE id=?1 AND version=?2",
                params![capability_id, version, status, now_millis()],
            )
            .map_err(|error| format!("CAPABILITY_STATUS_FAILED: {error}"))?;
        if changed == 0 {
            return Err(format!("CAPABILITY_NOT_FOUND: {capability_id}@{version}"));
        }
        self.get_user_capability(project_id, capability_id, Some(version))
    }

    pub fn validate_user_capability_for_draft(
        &self,
        project_id: &str,
        draft_id: &str,
        capability_id: &str,
    ) -> Result<UserCapability, String> {
        let capability = self.get_user_capability(project_id, capability_id, None)?;
        if capability.status != "active" {
            return Err(format!("CAPABILITY_NOT_ACTIVE: {capability_id}"));
        }
        validate_capability(&capability)?;
        let draft_system = self
            .project_connection(project_id)?
            .query_row(
                "SELECT system_id FROM draft_domains WHERE draft_id=?1 AND legacy=0",
                [draft_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| format!("DRAFT_DOMAIN_READ_FAILED: {error}"))?
            .flatten()
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
        Ok(capability)
    }

    pub fn save_domain_memory(
        &self,
        project_id: &str,
        memory: &DomainMemory,
    ) -> Result<DomainMemory, String> {
        self.ensure_known_system(&memory.system_id)?;
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
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("MEMORY_LIST_FAILED: {error}"))
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
        for system_id in [read_system, write_system].into_iter().flatten() {
            let pinned_version = serde_json::from_str::<serde_json::Value>(&versions_json)
                .unwrap_or_default()
                .get(system_id)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("TASK_SCOPE_PLUGIN_VERSION_REQUIRED: {system_id}"))?;
            let active = self.runtime_manifest(system_id)?;
            if active.version != pinned_version {
                return Err(format!(
                    "TASK_SCOPE_PLUGIN_VERSION_CHANGED: {system_id} is pinned to {pinned_version}, active is {}",
                    active.version
                ));
            }
        }
        Ok(TaskScopeLease {
            token: token.to_string(),
            task_id,
            read_systems,
            write_systems,
            draft_ids,
            plugin_versions: serde_json::from_str(&versions_json).unwrap_or_default(),
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
        self.get_draft(project_id, draft_id)?;
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
    if !is_semver(&capability.version) {
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

fn is_semver(value: &str) -> bool {
    let core = value.split_once('-').map_or(value, |(core, _)| core);
    let parts = core.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
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
        let store = DomainStore::new(base.join("data")).unwrap();
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
        store
            .save_user_capability(&project.id, &capability)
            .unwrap();
        let active = store
            .set_user_capability_status(&project.id, &capability.id, &capability.version, "active")
            .unwrap();
        assert_eq!(active.status, "active");

        let memory = DomainMemory {
            id: "memory-1".to_string(),
            system_id: "shop".to_string(),
            scope: "project".to_string(),
            kind: "rule".to_string(),
            summary: "价格规则".to_string(),
            body: serde_json::json!({"minimum":1}),
            status: "candidate".to_string(),
            source_task_id: "task-1".to_string(),
            plugin_version: "1.0.0".to_string(),
            created_at: now,
            updated_at: now,
        };
        store.save_domain_memory(&project.id, &memory).unwrap();
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
            plugin_versions: serde_json::json!({"domain":"1.0.0"}),
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

        let lease = store
            .issue_task_scope(
                &project.id,
                "task-1",
                &["shop".to_string(), "item".to_string()],
                &["shop".to_string()],
                &[],
                serde_json::json!({"shop":"1.0.0","item":"1.0.0"}),
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
}
