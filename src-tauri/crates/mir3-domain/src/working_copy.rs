//! 领域工作副本与保存节点。
//!
//! Draft 仍是内部的权限、版本和原子事务载体，但产品层只暴露工作副本、保存与
//! 撤回语义。人工编辑和单系统 AI 会话因而可以复用同一个活动工作副本。

use crate::draft::{hash_bytes, safe_project_target};
use crate::{
    now_millis, CompositeApplyResult, CompositeDraftConfirmation, DomainStore,
    DomainValidationReport, Draft, DraftPreview, DraftStatus, SafeTextOpen, SafeTextPatch,
    SafeTextPatchResult, SafeXlsDraftPatch, SafeXlsPatchResult, Snapshot,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

const SAVE_NODE_SCHEMA_VERSION: u32 = 1;
const SAVE_NODE_LIMIT: usize = 100;
const SAVE_NODE_RETENTION_MILLIS: i64 = 30 * 24 * 60 * 60 * 1_000;
const WORKING_COPY_INTENT_PREFIX: &str = "[working-copy]";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DomainWorkingCopy {
    pub id: String,
    pub system_id: String,
    pub plugin_version: String,
    pub composite_id: Option<String>,
    pub revision: i64,
    pub dirty: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DomainSaveNodeOrigin {
    Studio,
    Restore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DomainSaveNodeFile {
    pub path: String,
    pub before_existed: bool,
    pub before_sha256: Option<String>,
    pub after_existed: bool,
    pub after_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DomainSaveNode {
    pub schema_version: u32,
    pub id: String,
    pub project_id: String,
    pub system_id: Option<String>,
    pub system_ids: Vec<String>,
    pub origin: DomainSaveNodeOrigin,
    pub previous_node_id: Option<String>,
    pub restored_from_node_id: Option<String>,
    pub snapshot_id: String,
    pub working_copy_ids: Vec<String>,
    pub files: Vec<DomainSaveNodeFile>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainWorkingSaveResult {
    pub save_node: DomainSaveNode,
    pub validation: DomainValidationReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainCompositeWorkingSaveResult {
    pub save_node: DomainSaveNode,
    pub validations: Vec<DomainValidationReport>,
    pub apply_result: CompositeApplyResult,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainWorkingCopyRevision {
    pub working_copy_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainWorkingRestoreResult {
    pub save_node: DomainSaveNode,
    pub restored_snapshot: Snapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainWorkingXlsOpen {
    pub workbook: crate::SafeXlsWorkbook,
    pub sheets: Vec<crate::SafeXlsSheet>,
    pub revision: i64,
}

impl DomainStore {
    /// Apply 和治理 Receipt 已落盘但保存节点尚未写入时，从权威 Draft/Snapshot 恢复节点。
    /// 这覆盖进程在原子 Apply 完成后、节点提交前退出的窄窗口。
    pub(crate) fn recover_domain_save_nodes(&self) -> Result<(), String> {
        for project in self.list_projects()? {
            let _project_mutation = self.reserve_composite_mutation(&project.id)?;
            let connection = self.project_connection(&project.id)?;
            let mut statement = connection
                .prepare(
                    "SELECT tr.system_id,tr.draft_id,tr.evidence,d.intent
                     FROM task_receipts tr JOIN drafts d ON d.id=tr.draft_id
                     WHERE tr.status='applied' ORDER BY tr.created_at,tr.id",
                )
                .map_err(|error| format!("DOMAIN_SAVE_NODE_RECOVERY_READ_FAILED: {error}"))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(|error| format!("DOMAIN_SAVE_NODE_RECOVERY_READ_FAILED: {error}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("DOMAIN_SAVE_NODE_RECOVERY_READ_FAILED: {error}"))?;
            drop(statement);
            drop(connection);

            let mut groups: HashMap<String, Vec<(String, String)>> = HashMap::new();
            for (system_id, draft_id, evidence, intent) in rows {
                if !intent.starts_with(WORKING_COPY_INTENT_PREFIX) {
                    continue;
                }
                let evidence: serde_json::Value = serde_json::from_str(&evidence)
                    .map_err(|error| format!("DOMAIN_SAVE_NODE_RECOVERY_INVALID: {error}"))?;
                let snapshot_id = evidence
                    .get("snapshotId")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        "DOMAIN_SAVE_NODE_RECOVERY_INVALID: receipt has no snapshotId".to_string()
                    })?;
                groups
                    .entry(snapshot_id.to_string())
                    .or_default()
                    .push((system_id, draft_id));
            }
            for (snapshot_id, bindings) in groups {
                let exists = self
                    .project_connection(&project.id)?
                    .query_row(
                        "SELECT 1 FROM domain_save_nodes WHERE snapshot_id=?1 LIMIT 1",
                        [&snapshot_id],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(|error| format!("DOMAIN_SAVE_NODE_RECOVERY_READ_FAILED: {error}"))?
                    .is_some();
                if exists {
                    continue;
                }
                let snapshot = self
                    .list_snapshots(&project.id)?
                    .into_iter()
                    .find(|snapshot| snapshot.id == snapshot_id)
                    .ok_or_else(|| {
                        format!("DOMAIN_SAVE_NODE_RECOVERY_SNAPSHOT_MISSING: {snapshot_id}")
                    })?;
                let mut previews = HashMap::new();
                let mut system_ids = Vec::new();
                let mut working_copy_ids = Vec::new();
                for (system_id, draft_id) in bindings {
                    system_ids.push(system_id);
                    previews.insert(
                        draft_id.clone(),
                        self.preview_draft(&project.id, &draft_id)?,
                    );
                    working_copy_ids.push(draft_id);
                }
                let files = files_from_composite_apply(&snapshot, &previews);
                self.persist_domain_save_node(
                    &project.id,
                    system_ids,
                    DomainSaveNodeOrigin::Studio,
                    None,
                    snapshot,
                    working_copy_ids.clone(),
                    files,
                )?;
                for id in working_copy_ids {
                    self.remove_working_copy_mapping(&project.id, &id)?;
                }
            }
        }
        Ok(())
    }

    /// 返回当前系统唯一的活动工作副本；没有时惰性创建并固定领域包版本。
    pub fn get_or_create_domain_working_copy(
        &self,
        project_id: &str,
        system_id: &str,
        plugin_version: &str,
        intent: Option<&str>,
    ) -> Result<DomainWorkingCopy, String> {
        self.ensure_writable()?;
        let description = self.describe_domain_system(project_id, system_id)?;
        if description.manifest.version != plugin_version {
            return Err(format!(
                "DOMAIN_WORKING_COPY_PLUGIN_VERSION_MISMATCH: expected {}, got {plugin_version}",
                description.manifest.version
            ));
        }
        let _project_mutation = self.reserve_composite_mutation(project_id)?;
        if let Some(working_copy) = self.mapped_working_copy(project_id, system_id)? {
            if working_copy.plugin_version == plugin_version {
                return Ok(working_copy);
            }
            self.remove_working_copy_mapping(project_id, &working_copy.id)?;
        }

        // 兼容正在运行的旧 AI/Studio 会话：先接管同系统、同版本、非组合的开放 Draft。
        if let Some(draft) = self.latest_open_domain_draft(project_id, system_id, plugin_version)? {
            self.persist_working_copy_mapping(project_id, system_id, plugin_version, &draft)?;
            return self.domain_working_copy_by_id(project_id, &draft.id);
        }

        let label = intent
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("编辑领域配置");
        let draft = self.open_draft(
            project_id,
            &format!("{WORKING_COPY_INTENT_PREFIX} {system_id}: {label}"),
        )?;
        if let Err(error) =
            self.bind_draft_domain(project_id, &draft.id, system_id, plugin_version, None)
        {
            let _ = self.discard_draft(project_id, &draft.id);
            return Err(error);
        }
        self.persist_working_copy_mapping(project_id, system_id, plugin_version, &draft)?;
        self.domain_working_copy_by_id(project_id, &draft.id)
    }

    /// 使用工作副本覆盖层打开文本；Draft id 不再作为产品 API 暴露。
    pub fn domain_working_file_open(
        &self,
        project_id: &str,
        relative_path: &str,
        working_copy_id: Option<&str>,
    ) -> Result<SafeTextOpen, String> {
        if let Some(id) = working_copy_id {
            self.domain_working_copy_by_id(project_id, id)?;
        }
        self.safe_text_open(project_id, relative_path, working_copy_id)
    }

    /// 从一个确定的 Working Copy revision 一次读取完整 XLS，保证所有 sheet 同步刷新。
    pub fn domain_working_xls_open(
        &self,
        project_id: &str,
        relative_path: &str,
        working_copy_id: Option<&str>,
    ) -> Result<DomainWorkingXlsOpen, String> {
        crate::safe_files::validate_xls_path(relative_path)?;
        let working_copy = working_copy_id
            .map(|id| self.domain_working_copy_by_id(project_id, id))
            .transpose()?;
        let target = self.safe_file_target(project_id, relative_path)?;
        let source = fs::read(&target)
            .map_err(|error| format!("SAFE_XLS_READ_FAILED: {}: {error}", target.display()))?;
        let base_sha256 = hash_bytes(&source);
        let bytes = match working_copy.as_ref() {
            Some(copy) => self
                .draft_change_bytes(project_id, &copy.id, relative_path)?
                .unwrap_or(source),
            None => source,
        };
        let (workbook, sheets) =
            crate::safe_files::parse_xls_snapshot(relative_path, &bytes, &base_sha256)?;
        Ok(DomainWorkingXlsOpen {
            workbook,
            sheets,
            revision: working_copy.map_or(0, |copy| copy.revision),
        })
    }

    /// 人工文本编辑与 AI 共享内部 Draft revision，不允许调用方替换目标 Draft。
    pub fn domain_working_text_patch(
        &self,
        project_id: &str,
        working_copy_id: &str,
        mut request: SafeTextPatch,
    ) -> Result<SafeTextPatchResult, String> {
        self.domain_working_copy_by_id(project_id, working_copy_id)?;
        request.draft_id = Some(working_copy_id.to_string());
        let result = self.safe_text_patch(project_id, &request)?;
        self.touch_working_copy(project_id, working_copy_id)?;
        Ok(result)
    }

    /// XLS 编辑同样写入统一工作副本，避免表格编辑保留第二套 Draft 流程。
    pub fn domain_working_xls_patch(
        &self,
        project_id: &str,
        working_copy_id: &str,
        mut request: SafeXlsDraftPatch,
    ) -> Result<SafeXlsPatchResult, String> {
        self.domain_working_copy_by_id(project_id, working_copy_id)?;
        request.draft_id = working_copy_id.to_string();
        let result = self.safe_xls_patch(project_id, &request)?;
        self.touch_working_copy(project_id, working_copy_id)?;
        Ok(result)
    }

    /// “查看修改”按需生成 Diff；普通保存无需先打开审查弹窗。
    pub fn preview_domain_working_copy(
        &self,
        project_id: &str,
        working_copy_id: &str,
    ) -> Result<DraftPreview, String> {
        self.domain_working_copy_by_id(project_id, working_copy_id)?;
        self.preview_draft(project_id, working_copy_id)
    }

    /// 校验并原子保存单系统工作副本，成功后追加一个用户可理解的保存节点。
    pub fn save_domain_working_copy(
        &self,
        project_id: &str,
        working_copy_id: &str,
        expected_revision: i64,
        confirmed: bool,
    ) -> Result<DomainWorkingSaveResult, String> {
        let _project_mutation = self.reserve_composite_mutation(project_id)?;
        let working_copy = self.domain_working_copy_by_id(project_id, working_copy_id)?;
        if working_copy.composite_id.is_some() {
            return Err(
                "DOMAIN_WORKING_COMPOSITE_SAVE_REQUIRED: save all linked systems together"
                    .to_string(),
            );
        }
        if working_copy.revision != expected_revision {
            return Err(format!(
                "DOMAIN_WORKING_COPY_REVISION_CONFLICT: expected {expected_revision}, current {}",
                working_copy.revision
            ));
        }
        let preview = self.preview_draft(project_id, working_copy_id)?;
        if preview.changes.is_empty() {
            return Err("DOMAIN_WORKING_COPY_CLEAN: no changes to save".to_string());
        }
        if save_confirmation_required(&working_copy.system_id, &preview) && !confirmed {
            return Err(
                "DOMAIN_SAVE_CONFIRMATION_REQUIRED: high-risk changes require confirmation"
                    .to_string(),
            );
        }
        let validation = self.validate_domain_draft(project_id, working_copy_id)?;
        if !validation.valid {
            return Err(format!(
                "DRAFT_VALIDATION_FAILED: {}: {}",
                validation.system_id,
                validation.diagnostics.join(" | ")
            ));
        }
        let snapshot = self.apply_validated_domain_draft_with_governance(
            project_id,
            working_copy_id,
            expected_revision,
            &preview.diff_hash,
        )?;
        let node = match self.persist_domain_save_node(
            project_id,
            vec![working_copy.system_id.clone()],
            DomainSaveNodeOrigin::Studio,
            None,
            snapshot.clone(),
            vec![working_copy_id.to_string()],
            files_from_apply(&snapshot, &preview),
        ) {
            Ok(node) => node,
            Err(error) => {
                let compensation = self
                    .restore_snapshot_with_governance(project_id, &snapshot.id)
                    .err();
                return Err(join_compensation_error(error, compensation));
            }
        };
        self.remove_working_copy_mapping(project_id, working_copy_id)?;
        Ok(DomainWorkingSaveResult {
            save_node: node,
            validation,
        })
    }

    /// 跨系统工作副本仍使用原有组合事务，且必须由用户明确确认一次。
    pub fn save_domain_working_copies(
        &self,
        project_id: &str,
        composite_id: &str,
        working_copies: &[DomainWorkingCopyRevision],
        confirmed: bool,
    ) -> Result<DomainCompositeWorkingSaveResult, String> {
        let _project_mutation = self.reserve_composite_mutation(project_id)?;
        if !confirmed {
            return Err(
                "DOMAIN_SAVE_CONFIRMATION_REQUIRED: cross-system changes require confirmation"
                    .to_string(),
            );
        }
        if working_copies.len() < 2 {
            return Err(
                "DOMAIN_WORKING_COMPOSITE_INVALID: at least two working copies are required"
                    .to_string(),
            );
        }
        let mut ids = BTreeSet::new();
        let mut systems = Vec::with_capacity(working_copies.len());
        let mut validations = Vec::with_capacity(working_copies.len());
        let mut previews = HashMap::new();
        let mut confirmations = Vec::with_capacity(working_copies.len());
        for item in working_copies {
            if !ids.insert(item.working_copy_id.clone()) {
                return Err(format!(
                    "DOMAIN_WORKING_COMPOSITE_DUPLICATE: {}",
                    item.working_copy_id
                ));
            }
            let working_copy = self.domain_working_copy_by_id(project_id, &item.working_copy_id)?;
            if working_copy.composite_id.as_deref() != Some(composite_id) {
                return Err(format!(
                    "DOMAIN_WORKING_COMPOSITE_MISMATCH: {} is not linked to {composite_id}",
                    item.working_copy_id
                ));
            }
            if working_copy.revision != item.expected_revision {
                return Err(format!(
                    "DOMAIN_WORKING_COPY_REVISION_CONFLICT: expected {}, current {}",
                    item.expected_revision, working_copy.revision
                ));
            }
            let preview = self.preview_draft(project_id, &item.working_copy_id)?;
            if preview.changes.is_empty() {
                return Err(format!(
                    "DOMAIN_WORKING_COPY_CLEAN: {} has no changes to save",
                    item.working_copy_id
                ));
            }
            let validation = self.validate_domain_draft(project_id, &item.working_copy_id)?;
            if !validation.valid {
                return Err(format!(
                    "DRAFT_VALIDATION_FAILED: {}: {}",
                    validation.system_id,
                    validation.diagnostics.join(" | ")
                ));
            }
            systems.push(working_copy.system_id);
            validations.push(validation);
            confirmations.push(CompositeDraftConfirmation {
                draft_id: item.working_copy_id.clone(),
                expected_revision: item.expected_revision,
                expected_diff_hash: preview.diff_hash.clone(),
            });
            previews.insert(item.working_copy_id.clone(), preview);
        }
        let apply_result = self.apply_validated_composite_drafts_with_governance(
            project_id,
            composite_id,
            &confirmations,
        )?;
        let files = files_from_composite_apply(&apply_result.snapshot, &previews);
        let working_copy_ids = working_copies
            .iter()
            .map(|item| item.working_copy_id.clone())
            .collect::<Vec<_>>();
        let node = match self.persist_domain_save_node(
            project_id,
            systems,
            DomainSaveNodeOrigin::Studio,
            None,
            apply_result.snapshot.clone(),
            working_copy_ids.clone(),
            files,
        ) {
            Ok(node) => node,
            Err(error) => {
                let compensation = self
                    .restore_snapshot_with_governance(project_id, &apply_result.snapshot.id)
                    .err();
                return Err(join_compensation_error(error, compensation));
            }
        };
        for working_copy_id in &working_copy_ids {
            self.remove_working_copy_mapping(project_id, working_copy_id)?;
        }
        Ok(DomainCompositeWorkingSaveResult {
            save_node: node,
            validations,
            apply_result,
        })
    }

    pub fn list_domain_save_nodes(
        &self,
        project_id: &str,
        system_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<DomainSaveNode>, String> {
        self.get_project(project_id)?;
        let connection = self.project_connection(project_id)?;
        let sql = if system_id.is_some() {
            "SELECT manifest FROM domain_save_nodes WHERE system_id=?1 ORDER BY created_at DESC,id DESC LIMIT ?2"
        } else {
            "SELECT manifest FROM domain_save_nodes ORDER BY created_at DESC,id DESC LIMIT ?2"
        };
        let mut statement = connection
            .prepare(sql)
            .map_err(|error| format!("DOMAIN_SAVE_NODE_LIST_FAILED: {error}"))?;
        let limit = limit.clamp(1, SAVE_NODE_LIMIT) as i64;
        let manifests = if let Some(system_id) = system_id {
            statement
                .query_map(params![system_id, limit], |row| row.get::<_, String>(0))
                .map_err(|error| format!("DOMAIN_SAVE_NODE_LIST_FAILED: {error}"))?
                .collect::<Result<Vec<_>, _>>()
        } else {
            statement
                .query_map(params!["", limit], |row| row.get::<_, String>(0))
                .map_err(|error| format!("DOMAIN_SAVE_NODE_LIST_FAILED: {error}"))?
                .collect::<Result<Vec<_>, _>>()
        }
        .map_err(|error| format!("DOMAIN_SAVE_NODE_LIST_FAILED: {error}"))?;
        manifests
            .into_iter()
            .map(|manifest| {
                serde_json::from_str(&manifest)
                    .map_err(|error| format!("DOMAIN_SAVE_NODE_INVALID: {error}"))
            })
            .collect()
    }

    /// 只允许撤回历史栈顶，恢复前逐文件核对保存后的 SHA，并把撤回追加为新节点。
    pub fn restore_domain_save_node(
        &self,
        project_id: &str,
        node_id: &str,
    ) -> Result<DomainWorkingRestoreResult, String> {
        let _project_mutation = self.reserve_composite_mutation(project_id)?;
        let latest = self
            .list_domain_save_nodes(project_id, None, 1)?
            .into_iter()
            .next()
            .ok_or_else(|| "DOMAIN_SAVE_NODE_NOT_FOUND: no save node exists".to_string())?;
        if latest.id != node_id {
            return Err(
                "DOMAIN_SAVE_NODE_NOT_LATEST: only the latest save can be restored".to_string(),
            );
        }
        let project = self.get_project(project_id)?;
        verify_save_node_current_state(PathBuf::from(project.root), &latest)?;
        let (restored_snapshot, inverse_snapshot) =
            self.restore_snapshot_with_governance_and_inverse(project_id, &latest.snapshot_id)?;
        let restored_files = files_after_restore(&latest.files);
        let restore_node = self.persist_domain_save_node(
            project_id,
            latest.system_ids.clone(),
            DomainSaveNodeOrigin::Restore,
            Some(latest.id.clone()),
            inverse_snapshot,
            latest.working_copy_ids.clone(),
            restored_files,
        )?;
        for working_copy_id in &latest.working_copy_ids {
            if self
                .get_draft(project_id, working_copy_id)
                .is_ok_and(|draft| draft.status == DraftStatus::Open)
            {
                let _ = self.discard_draft(project_id, working_copy_id);
            }
            let _ = self.remove_working_copy_mapping(project_id, working_copy_id);
        }
        Ok(DomainWorkingRestoreResult {
            save_node: restore_node,
            restored_snapshot,
        })
    }

    pub fn domain_working_copy_by_id(
        &self,
        project_id: &str,
        working_copy_id: &str,
    ) -> Result<DomainWorkingCopy, String> {
        let connection = self.project_connection(project_id)?;
        connection
            .query_row(
                "SELECT d.id,dd.system_id,dd.plugin_version,dd.composite_id,d.revision,d.created_at,d.updated_at,
                        EXISTS(SELECT 1 FROM draft_changes dc WHERE dc.draft_id=d.id)
                 FROM drafts d
                 JOIN draft_domains dd ON dd.draft_id=d.id
                 JOIN domain_working_copies wc ON wc.draft_id=d.id
                 WHERE d.id=?1 AND d.status='open' AND dd.legacy=0",
                [working_copy_id],
                |row| {
                    Ok(DomainWorkingCopy {
                        id: row.get(0)?,
                        system_id: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        plugin_version: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        composite_id: row.get(3)?,
                        revision: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                        dirty: row.get::<_, i64>(7)? != 0,
                    })
                },
            )
            .optional()
            .map_err(|error| format!("DOMAIN_WORKING_COPY_READ_FAILED: {error}"))?
            .filter(|copy| !copy.system_id.is_empty() && !copy.plugin_version.is_empty())
            .ok_or_else(|| format!("DOMAIN_WORKING_COPY_NOT_FOUND: {working_copy_id}"))
    }

    fn mapped_working_copy(
        &self,
        project_id: &str,
        system_id: &str,
    ) -> Result<Option<DomainWorkingCopy>, String> {
        let id = self
            .project_connection(project_id)?
            .query_row(
                "SELECT draft_id FROM domain_working_copies WHERE system_id=?1",
                [system_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("DOMAIN_WORKING_COPY_READ_FAILED: {error}"))?;
        id.map(|id| self.domain_working_copy_by_id(project_id, &id))
            .transpose()
            .or_else(|error| {
                // 应用崩溃后残留的映射不应阻止重新创建干净工作副本。
                if error.starts_with("DOMAIN_WORKING_COPY_NOT_FOUND:") {
                    self.project_connection(project_id)?
                        .execute(
                            "DELETE FROM domain_working_copies WHERE system_id=?1",
                            [system_id],
                        )
                        .map_err(|error| format!("DOMAIN_WORKING_COPY_CLEANUP_FAILED: {error}"))?;
                    Ok(None)
                } else {
                    Err(error)
                }
            })
    }

    fn latest_open_domain_draft(
        &self,
        project_id: &str,
        system_id: &str,
        plugin_version: &str,
    ) -> Result<Option<Draft>, String> {
        self.project_connection(project_id)?
            .query_row(
                "SELECT d.id,d.intent,d.revision,d.status,d.created_at,d.updated_at
                 FROM drafts d JOIN draft_domains dd ON dd.draft_id=d.id
                 WHERE d.status='open' AND dd.system_id=?1 AND dd.plugin_version=?2
                   AND dd.legacy=0 AND dd.composite_id IS NULL
                 ORDER BY d.updated_at DESC LIMIT 1",
                params![system_id, plugin_version],
                |row| {
                    Ok(Draft {
                        id: row.get(0)?,
                        intent: row.get(1)?,
                        revision: row.get(2)?,
                        status: DraftStatus::Open,
                        created_at: row.get(4)?,
                        updated_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(|error| format!("DOMAIN_WORKING_COPY_READ_FAILED: {error}"))
    }

    fn persist_working_copy_mapping(
        &self,
        project_id: &str,
        system_id: &str,
        plugin_version: &str,
        draft: &Draft,
    ) -> Result<(), String> {
        self.project_connection(project_id)?
            .execute(
                "INSERT INTO domain_working_copies(system_id,draft_id,plugin_version,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5)
                 ON CONFLICT(system_id) DO UPDATE SET draft_id=excluded.draft_id,
                   plugin_version=excluded.plugin_version,updated_at=excluded.updated_at",
                params![
                    system_id,
                    draft.id,
                    plugin_version,
                    draft.created_at,
                    draft.updated_at
                ],
            )
            .map_err(|error| format!("DOMAIN_WORKING_COPY_WRITE_FAILED: {error}"))?;
        Ok(())
    }

    fn touch_working_copy(&self, project_id: &str, working_copy_id: &str) -> Result<(), String> {
        self.project_connection(project_id)?
            .execute(
                "UPDATE domain_working_copies SET updated_at=?2 WHERE draft_id=?1",
                params![working_copy_id, now_millis()],
            )
            .map_err(|error| format!("DOMAIN_WORKING_COPY_WRITE_FAILED: {error}"))?;
        Ok(())
    }

    fn remove_working_copy_mapping(
        &self,
        project_id: &str,
        working_copy_id: &str,
    ) -> Result<(), String> {
        self.project_connection(project_id)?
            .execute(
                "DELETE FROM domain_working_copies WHERE draft_id=?1",
                [working_copy_id],
            )
            .map_err(|error| format!("DOMAIN_WORKING_COPY_CLEANUP_FAILED: {error}"))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn persist_domain_save_node(
        &self,
        project_id: &str,
        mut system_ids: Vec<String>,
        origin: DomainSaveNodeOrigin,
        restored_from_node_id: Option<String>,
        snapshot: Snapshot,
        working_copy_ids: Vec<String>,
        files: Vec<DomainSaveNodeFile>,
    ) -> Result<DomainSaveNode, String> {
        system_ids.sort();
        system_ids.dedup();
        let previous_node_id = self
            .list_domain_save_nodes(project_id, None, 1)?
            .into_iter()
            .next()
            .map(|node| node.id);
        let created_at = now_millis();
        let id = save_node_id(project_id, &snapshot.id, created_at);
        let system_id = (system_ids.len() == 1).then(|| system_ids[0].clone());
        let node = DomainSaveNode {
            schema_version: SAVE_NODE_SCHEMA_VERSION,
            id,
            project_id: project_id.to_string(),
            system_id: system_id.clone(),
            system_ids,
            origin,
            previous_node_id,
            restored_from_node_id,
            snapshot_id: snapshot.id,
            working_copy_ids,
            files,
            created_at,
        };
        let manifest = serde_json::to_string(&node)
            .map_err(|error| format!("DOMAIN_SAVE_NODE_SERIALIZE_FAILED: {error}"))?;
        let mut connection = self.project_connection(project_id)?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("DOMAIN_SAVE_NODE_TRANSACTION_FAILED: {error}"))?;
        transaction
            .execute(
                "INSERT INTO domain_save_nodes(id,system_id,origin,snapshot_id,restored_from_node_id,manifest,created_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    node.id,
                    system_id,
                    save_node_origin(&node.origin),
                    node.snapshot_id,
                    node.restored_from_node_id,
                    manifest,
                    created_at
                ],
            )
            .map_err(|error| format!("DOMAIN_SAVE_NODE_WRITE_FAILED: {error}"))?;
        let retention_start = created_at.saturating_sub(SAVE_NODE_RETENTION_MILLIS);
        transaction
            .execute(
                "DELETE FROM domain_save_nodes WHERE id IN (
                   SELECT id FROM domain_save_nodes
                   WHERE created_at < ?1
                      OR id NOT IN (SELECT id FROM domain_save_nodes ORDER BY created_at DESC,id DESC LIMIT ?2)
                 )",
                params![retention_start, SAVE_NODE_LIMIT as i64],
            )
            .map_err(|error| format!("DOMAIN_SAVE_NODE_PRUNE_FAILED: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("DOMAIN_SAVE_NODE_COMMIT_FAILED: {error}"))?;
        Ok(node)
    }
}

fn save_confirmation_required(system_id: &str, preview: &DraftPreview) -> bool {
    const CRITICAL_SYSTEMS: &[&str] = &[
        "first_charge",
        "cumulative_charge",
        "vip",
        "shop",
        "recycle",
        "cross_server",
    ];
    CRITICAL_SYSTEMS.contains(&system_id) || preview.changes.iter().any(|change| change.deleted)
}

fn files_from_apply(snapshot: &Snapshot, preview: &DraftPreview) -> Vec<DomainSaveNodeFile> {
    let after = preview
        .changes
        .iter()
        .map(|change| (change.path.as_str(), change))
        .collect::<HashMap<_, _>>();
    snapshot
        .files
        .iter()
        .filter_map(|before| {
            let change = after.get(before.path.as_str())?;
            Some(DomainSaveNodeFile {
                path: before.path.clone(),
                before_existed: before.existed,
                before_sha256: before.sha256.clone(),
                after_existed: !change.deleted,
                after_sha256: change.new_sha256.clone(),
            })
        })
        .collect()
}

fn files_from_composite_apply(
    snapshot: &Snapshot,
    previews: &HashMap<String, DraftPreview>,
) -> Vec<DomainSaveNodeFile> {
    let changes = previews
        .values()
        .flat_map(|preview| preview.changes.iter())
        .map(|change| (change.path.as_str(), change))
        .collect::<HashMap<_, _>>();
    snapshot
        .files
        .iter()
        .filter_map(|before| {
            let change = changes.get(before.path.as_str())?;
            Some(DomainSaveNodeFile {
                path: before.path.clone(),
                before_existed: before.existed,
                before_sha256: before.sha256.clone(),
                after_existed: !change.deleted,
                after_sha256: change.new_sha256.clone(),
            })
        })
        .collect()
}

fn files_after_restore(files: &[DomainSaveNodeFile]) -> Vec<DomainSaveNodeFile> {
    files
        .iter()
        .map(|file| DomainSaveNodeFile {
            path: file.path.clone(),
            before_existed: file.after_existed,
            before_sha256: file.after_sha256.clone(),
            after_existed: file.before_existed,
            after_sha256: file.before_sha256.clone(),
        })
        .collect()
}

fn verify_save_node_current_state(
    project_root: PathBuf,
    node: &DomainSaveNode,
) -> Result<(), String> {
    for file in &node.files {
        let target = safe_project_target(&project_root, &file.path)?;
        let current = match fs::read(&target) {
            Ok(bytes) => Some(hash_bytes(&bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(format!(
                    "DOMAIN_SAVE_NODE_CURRENT_READ_FAILED: {}: {error}",
                    target.display()
                ));
            }
        };
        if target.is_file() != file.after_existed || current != file.after_sha256 {
            return Err(format!(
                "DOMAIN_SAVE_NODE_EXTERNAL_EDIT_CONFLICT: {} changed after the save",
                file.path
            ));
        }
    }
    Ok(())
}

fn save_node_id(project_id: &str, snapshot_id: &str, created_at: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project_id.as_bytes());
    hasher.update(snapshot_id.as_bytes());
    hasher.update(created_at.to_le_bytes());
    format!("domain-save-{:x}", hasher.finalize())
}

fn save_node_origin(origin: &DomainSaveNodeOrigin) -> &'static str {
    match origin {
        DomainSaveNodeOrigin::Studio => "studio",
        DomainSaveNodeOrigin::Restore => "restore",
    }
}

fn join_compensation_error(error: String, compensation: Option<String>) -> String {
    match compensation {
        Some(compensation) => format!(
            "DOMAIN_SAVE_NODE_PERSIST_FAILED: {error}; DOMAIN_SAVE_COMPENSATION_FAILED: {compensation}"
        ),
        None => format!("DOMAIN_SAVE_NODE_PERSIST_FAILED: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyexcel_xls::biff8::{Biff8Book, Biff8Cell, Biff8Sheet, Biff8Value};

    fn shop_record(price: usize) -> String {
        format!(
            "offerId=O1\nshopId=S1\nitemId=I1\ncurrencyItemId=I1\nprice={price}\nstartEpochSeconds=0\nendEpochSeconds=1\n"
        )
    }

    fn fixture(name: &str) -> (PathBuf, PathBuf, DomainStore, String) {
        let base = std::env::temp_dir().join(format!(
            "mir3-working-copy-{name}-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let project = base.join("木立");
        fs::create_dir_all(project.join("客户端/dev")).unwrap();
        fs::create_dir_all(project.join("引擎/Mir200/Envir")).unwrap();
        let shop = shop_record(1);
        fs::write(project.join("客户端/dev/shop.txt"), &shop).unwrap();
        fs::write(project.join("引擎/Mir200/Envir/shop.txt"), &shop).unwrap();
        fs::write(
            project.join("引擎/Mir200/Envir/cfg_item.txt"),
            "itemId=I1\nitemType=consumable\nstackLimit=1\nclientIcon=icons/I1.png\nengineStdMode=1\nlinkedBuffId=B1\n",
        )
        .unwrap();
        fs::write(project.join("引擎/Mir200/Envir/buff.txt"), "buffId=B1\n").unwrap();
        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let imported = store.import_project(&project).unwrap();
        store.scan_project(&imported.id, || false).unwrap();
        (base, project, store, imported.id)
    }

    #[test]
    fn manual_and_ai_reuse_one_working_copy_then_save_and_restore() {
        let (base, project, store, project_id) = fixture("save-restore");
        let version = store
            .describe_domain_system(&project_id, "shop")
            .unwrap()
            .manifest
            .version;
        let first = store
            .get_or_create_domain_working_copy(&project_id, "shop", &version, Some("人工修改"))
            .unwrap();
        let ai = store
            .get_or_create_domain_working_copy(&project_id, "shop", &version, Some("AI 修改"))
            .unwrap();
        assert_eq!(first.id, ai.id);

        let relative_path = "客户端/dev/shop.txt";
        let opened = store
            .domain_working_file_open(&project_id, relative_path, Some(&first.id))
            .unwrap();
        let patched = store
            .domain_working_text_patch(
                &project_id,
                &first.id,
                SafeTextPatch {
                    relative_path: relative_path.to_string(),
                    draft_id: None,
                    expected_revision: opened.revision,
                    expected_sha256: opened.sha256,
                    original_content: opened.content,
                    new_content: shop_record(2),
                    newline: Some("LF".to_string()),
                },
            )
            .unwrap();
        let engine_path = "引擎/Mir200/Envir/shop.txt";
        let engine = store
            .domain_working_file_open(&project_id, engine_path, Some(&first.id))
            .unwrap();
        let patched = store
            .domain_working_text_patch(
                &project_id,
                &first.id,
                SafeTextPatch {
                    relative_path: engine_path.to_string(),
                    draft_id: None,
                    expected_revision: patched.revision,
                    expected_sha256: engine.sha256,
                    original_content: engine.content,
                    new_content: shop_record(2),
                    newline: Some("LF".to_string()),
                },
            )
            .unwrap();
        assert_eq!(
            fs::read_to_string(project.join(relative_path)).unwrap(),
            shop_record(1)
        );
        let confirmation = store
            .save_domain_working_copy(&project_id, &first.id, patched.revision, false)
            .unwrap_err();
        assert!(confirmation.starts_with("DOMAIN_SAVE_CONFIRMATION_REQUIRED:"));
        let saved = store
            .save_domain_working_copy(&project_id, &first.id, patched.revision, true)
            .unwrap();
        assert_eq!(saved.save_node.origin, DomainSaveNodeOrigin::Studio);
        assert_eq!(saved.save_node.system_id.as_deref(), Some("shop"));
        assert_eq!(
            fs::read_to_string(project.join(relative_path)).unwrap(),
            shop_record(2)
        );
        assert!(store
            .domain_working_copy_by_id(&project_id, &first.id)
            .unwrap_err()
            .starts_with("DOMAIN_WORKING_COPY_NOT_FOUND:"));

        let restored = store
            .restore_domain_save_node(&project_id, &saved.save_node.id)
            .unwrap();
        assert_eq!(restored.save_node.origin, DomainSaveNodeOrigin::Restore);
        assert_eq!(
            restored.save_node.restored_from_node_id.as_deref(),
            Some(saved.save_node.id.as_str())
        );
        assert_eq!(
            fs::read_to_string(project.join(relative_path)).unwrap(),
            shop_record(1)
        );
        assert_eq!(
            store
                .list_domain_save_nodes(&project_id, None, 10)
                .unwrap()
                .len(),
            2
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn xls_working_open_reads_the_current_overlay_without_touching_disk() {
        let (base, project, store, project_id) = fixture("xls-overlay-open");
        let relative_path = "引擎/Mir200/Envir/Shop/cfg_store.xls";
        let target = project.join(relative_path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let mut sheet = Biff8Sheet::new("商品");
        sheet
            .set(
                0,
                0,
                Biff8Cell::general(Biff8Value::Text("旧价格".to_string())),
            )
            .unwrap();
        let mut book = Biff8Book::default();
        book.sheets.push(sheet);
        let original = book.to_cfb_bytes().unwrap();
        fs::write(&target, &original).unwrap();

        let version = store
            .describe_domain_system(&project_id, "shop")
            .unwrap()
            .manifest
            .version;
        let working_copy = store
            .get_or_create_domain_working_copy(&project_id, "shop", &version, Some("表格修改"))
            .unwrap();
        let disk = store
            .domain_working_xls_open(&project_id, relative_path, None)
            .unwrap();
        assert_eq!(disk.revision, 0);
        assert_eq!(disk.workbook.sha256, disk.workbook.content_sha256);

        let patched = store
            .domain_working_xls_patch(
                &project_id,
                &working_copy.id,
                SafeXlsDraftPatch {
                    relative_path: relative_path.to_string(),
                    draft_id: "调用方不能替换工作副本".to_string(),
                    expected_revision: working_copy.revision,
                    expected_sha256: disk.workbook.sha256.clone(),
                    updates: vec![crate::SafeXlsCellUpdate {
                        sheet: "商品".to_string(),
                        row: 0,
                        column: 0,
                        expected_value: Some("旧价格".to_string()),
                        value: serde_json::json!("新价格"),
                    }],
                },
            )
            .unwrap();
        let overlay = store
            .domain_working_xls_open(&project_id, relative_path, Some(&working_copy.id))
            .unwrap();
        assert_eq!(overlay.revision, patched.revision);
        assert_eq!(overlay.workbook.sha256, disk.workbook.sha256);
        assert_ne!(
            overlay.workbook.content_sha256,
            disk.workbook.content_sha256
        );
        assert_eq!(overlay.sheets.len(), 1);
        assert_eq!(overlay.sheets[0].rows[0][0], "新价格");
        assert_eq!(fs::read(&target).unwrap(), original);

        let mut external_sheet = Biff8Sheet::new("商品");
        external_sheet
            .set(
                0,
                0,
                Biff8Cell::general(Biff8Value::Text("游戏外部保存".to_string())),
            )
            .unwrap();
        let mut external_book = Biff8Book::default();
        external_book.sheets.push(external_sheet);
        let external = external_book.to_cfb_bytes().unwrap();
        fs::write(&target, &external).unwrap();
        let reopened = store
            .domain_working_xls_open(&project_id, relative_path, Some(&working_copy.id))
            .unwrap();
        let conflict = store
            .domain_working_xls_patch(
                &project_id,
                &working_copy.id,
                SafeXlsDraftPatch {
                    relative_path: relative_path.to_string(),
                    draft_id: working_copy.id.clone(),
                    expected_revision: patched.revision,
                    expected_sha256: reopened.workbook.sha256,
                    updates: vec![crate::SafeXlsCellUpdate {
                        sheet: "商品".to_string(),
                        row: 0,
                        column: 0,
                        expected_value: Some("新价格".to_string()),
                        value: serde_json::json!("再次修改"),
                    }],
                },
            )
            .unwrap_err();
        assert!(conflict.starts_with("DRAFT_BASE_CONFLICT:"));
        assert_eq!(fs::read(&target).unwrap(), external);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn restore_rejects_external_changes_after_save() {
        let (base, project, store, project_id) = fixture("external-conflict");
        let version = store
            .describe_domain_system(&project_id, "shop")
            .unwrap()
            .manifest
            .version;
        let working_copy = store
            .get_or_create_domain_working_copy(&project_id, "shop", &version, None)
            .unwrap();
        let relative_path = "客户端/dev/shop.txt";
        let opened = store
            .domain_working_file_open(&project_id, relative_path, Some(&working_copy.id))
            .unwrap();
        let patched = store
            .domain_working_text_patch(
                &project_id,
                &working_copy.id,
                SafeTextPatch {
                    relative_path: relative_path.to_string(),
                    draft_id: None,
                    expected_revision: opened.revision,
                    expected_sha256: opened.sha256,
                    original_content: opened.content,
                    new_content: shop_record(2),
                    newline: Some("LF".to_string()),
                },
            )
            .unwrap();
        let engine_path = "引擎/Mir200/Envir/shop.txt";
        let engine = store
            .domain_working_file_open(&project_id, engine_path, Some(&working_copy.id))
            .unwrap();
        let patched = store
            .domain_working_text_patch(
                &project_id,
                &working_copy.id,
                SafeTextPatch {
                    relative_path: engine_path.to_string(),
                    draft_id: None,
                    expected_revision: patched.revision,
                    expected_sha256: engine.sha256,
                    original_content: engine.content,
                    new_content: shop_record(2),
                    newline: Some("LF".to_string()),
                },
            )
            .unwrap();
        let saved = store
            .save_domain_working_copy(&project_id, &working_copy.id, patched.revision, true)
            .unwrap();
        fs::write(project.join(relative_path), shop_record(99)).unwrap();
        let error = store
            .restore_domain_save_node(&project_id, &saved.save_node.id)
            .unwrap_err();
        assert!(error.starts_with("DOMAIN_SAVE_NODE_EXTERNAL_EDIT_CONFLICT:"));
        assert_eq!(
            fs::read_to_string(project.join(relative_path)).unwrap(),
            shop_record(99)
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn startup_recovers_save_node_after_apply_completed_first() {
        let (base, _project, store, project_id) = fixture("startup-recovery");
        let version = store
            .describe_domain_system(&project_id, "shop")
            .unwrap()
            .manifest
            .version;
        let working_copy = store
            .get_or_create_domain_working_copy(&project_id, "shop", &version, None)
            .unwrap();
        let client_path = "客户端/dev/shop.txt";
        let engine_path = "引擎/Mir200/Envir/shop.txt";
        let preview = store
            .patch_draft(
                &project_id,
                &working_copy.id,
                working_copy.revision,
                &[
                    crate::DraftChangeInput {
                        path: client_path.to_string(),
                        content: Some(shop_record(2)),
                        deleted: false,
                        expected_sha256: None,
                    },
                    crate::DraftChangeInput {
                        path: engine_path.to_string(),
                        content: Some(shop_record(2)),
                        deleted: false,
                        expected_sha256: None,
                    },
                ],
            )
            .unwrap();
        store
            .apply_validated_domain_draft_with_governance(
                &project_id,
                &working_copy.id,
                preview.draft.revision,
                &preview.diff_hash,
            )
            .unwrap();
        assert!(store
            .list_domain_save_nodes(&project_id, None, 10)
            .unwrap()
            .is_empty());
        drop(store);

        let reopened = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let nodes = reopened
            .list_domain_save_nodes(&project_id, None, 10)
            .unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].working_copy_ids, vec![working_copy.id.clone()]);
        assert!(reopened
            .domain_working_copy_by_id(&project_id, &working_copy.id)
            .unwrap_err()
            .starts_with("DOMAIN_WORKING_COPY_NOT_FOUND:"));
        fs::remove_dir_all(base).ok();
    }
}
