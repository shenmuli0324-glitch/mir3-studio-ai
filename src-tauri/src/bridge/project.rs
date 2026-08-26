//! 996 项目、索引、Draft、备份和知识治理的 Tauri 命令。

use crate::service::project::{DraftConfirmation, ProjectService, ScanState};
use mir3_domain::{
    CapabilityCompileRequest, CapabilityPromotionRequest, CapabilityResolution,
    CapabilityRollbackRequest, CompositeApplyResult, CompositeDraftConfirmation,
    DomainDependencyGraph, DomainFileQuery, DomainFileRecord, DomainManifest, DomainMemory,
    DomainResourceQuery, DomainResourceRecord, DomainSystemDescription, DomainValidationReport,
    Draft, GlobalCapabilityCompileRequest, IndexQuery, IndexRecord, IndexStats, KnowledgeFilter,
    KnowledgeRecord, KnowledgeStatus, LegacyDraftCloneRequest, Mir3Project, SafeTextOpen,
    SafeTextPatch, SafeTextPatchResult, SafeXlsDraftPatch, SafeXlsPatchResult, SafeXlsSheet,
    SafeXlsWorkbook, Snapshot, SystemSessionBinding, TaskReceipt, TaskScopeLease, UserCapability,
    WorkspaceDirectory,
};
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn project_pick_directory() -> Result<Option<String>, String> {
    Ok(rfd::AsyncFileDialog::new()
        .set_title("选择 996 传奇3 项目目录")
        .pick_folder()
        .await
        .map(|handle| handle.path().to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn workspace_pick_directory(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<Option<String>, String> {
    let project = service.store().get_project(&project_id)?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title("选择项目内工作区")
        .set_directory(&project.root)
        .pick_folder()
        .await;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = mir3_domain::validate_workspace_path(Path::new(&project.root), selected.path())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn project_import(
    app: AppHandle,
    service: State<'_, ProjectService>,
    path: String,
) -> Result<Mir3Project, String> {
    let project = service.store().import_project(Path::new(&path))?;
    service.start_scan(app, project.id.clone())?;
    Ok(project)
}

#[tauri::command]
pub fn project_list(service: State<'_, ProjectService>) -> Result<Vec<Mir3Project>, String> {
    service.store().list_projects()
}

#[tauri::command]
pub fn project_get_active(
    service: State<'_, ProjectService>,
) -> Result<Option<Mir3Project>, String> {
    service.store().active_project()
}

#[tauri::command]
pub fn project_activate(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<Mir3Project, String> {
    service.store().activate_project(&project_id)
}

#[tauri::command]
pub fn project_relink(
    service: State<'_, ProjectService>,
    project_id: String,
    path: String,
) -> Result<Mir3Project, String> {
    service
        .store()
        .relink_project(&project_id, Path::new(&path))
}

#[tauri::command]
pub fn project_remove(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<(), String> {
    service.store().remove_project(&project_id)
}

#[tauri::command]
pub fn project_validate(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<Mir3Project, String> {
    service.store().validate_project(&project_id)
}

#[tauri::command]
pub fn workspace_select(
    service: State<'_, ProjectService>,
    project_id: String,
    path: String,
) -> Result<Mir3Project, String> {
    service
        .store()
        .select_workspace(&project_id, Path::new(&path))
}

#[tauri::command]
pub fn workspace_list(
    service: State<'_, ProjectService>,
    project_id: String,
    parent: Option<String>,
) -> Result<Vec<WorkspaceDirectory>, String> {
    service
        .store()
        .workspace_directories(&project_id, parent.as_deref().map(Path::new))
}

#[tauri::command]
pub fn scan_start(
    app: AppHandle,
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<ScanState, String> {
    service.start_scan(app, project_id)
}

#[tauri::command]
pub fn scan_cancel(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<ScanState, String> {
    service.cancel_scan(&project_id)
}

#[tauri::command]
pub fn scan_status(service: State<'_, ProjectService>) -> ScanState {
    service.scan_state()
}

#[tauri::command]
pub fn index_stats(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<IndexStats, String> {
    service.store().index_stats(&project_id)
}

#[tauri::command]
pub fn index_search(
    service: State<'_, ProjectService>,
    project_id: String,
    query: IndexQuery,
) -> Result<Vec<IndexRecord>, String> {
    service.store().query_index(&project_id, &query)
}

#[tauri::command]
pub fn domain_system_list(
    service: State<'_, ProjectService>,
) -> Result<Vec<DomainManifest>, String> {
    service.store().list_domain_systems()
}

#[tauri::command]
pub fn domain_system_describe(
    service: State<'_, ProjectService>,
    project_id: String,
    system_id: String,
) -> Result<DomainSystemDescription, String> {
    service
        .store()
        .describe_domain_system(&project_id, &system_id)
}

#[tauri::command]
pub fn domain_file_query(
    service: State<'_, ProjectService>,
    project_id: String,
    system_id: String,
    query: DomainFileQuery,
) -> Result<Vec<DomainFileRecord>, String> {
    service
        .store()
        .query_domain_files(&project_id, &system_id, &query)
}

#[tauri::command]
pub fn domain_unclaimed_file_query(
    service: State<'_, ProjectService>,
    project_id: String,
    query: DomainFileQuery,
) -> Result<Vec<DomainFileRecord>, String> {
    service
        .store()
        .query_unclaimed_domain_files(&project_id, &query)
}

#[tauri::command]
pub fn domain_resource_get(
    service: State<'_, ProjectService>,
    project_id: String,
    system_id: String,
    resource_id: String,
) -> Result<DomainResourceRecord, String> {
    service
        .store()
        .get_domain_resource(&project_id, &system_id, &resource_id)
}

#[tauri::command]
pub fn domain_resource_query(
    service: State<'_, ProjectService>,
    project_id: String,
    system_id: String,
    query: DomainResourceQuery,
) -> Result<Vec<DomainResourceRecord>, String> {
    service
        .store()
        .query_domain_resources(&project_id, &system_id, &query)
}

#[tauri::command]
pub fn domain_dependency_resolve(
    service: State<'_, ProjectService>,
    system_id: String,
) -> Result<DomainDependencyGraph, String> {
    service.store().resolve_domain_dependencies(&system_id)
}

#[tauri::command]
pub fn domain_validate(
    service: State<'_, ProjectService>,
    project_id: String,
    system_id: String,
) -> Result<DomainValidationReport, String> {
    service
        .store()
        .validate_domain_system(&project_id, &system_id)
}

#[tauri::command]
pub fn domain_draft_validate(
    service: State<'_, ProjectService>,
    project_id: String,
    draft_id: String,
) -> Result<DomainValidationReport, String> {
    service
        .store()
        .validate_domain_draft(&project_id, &draft_id)
}

#[tauri::command]
pub fn task_receipt_list(
    service: State<'_, ProjectService>,
    project_id: String,
    system_id: Option<String>,
) -> Result<Vec<TaskReceipt>, String> {
    service
        .store()
        .list_task_receipts(&project_id, system_id.as_deref())
}

#[tauri::command]
pub fn task_receipt_save(
    service: State<'_, ProjectService>,
    project_id: String,
    receipt: TaskReceipt,
) -> Result<TaskReceipt, String> {
    service.store().save_task_receipt(&project_id, &receipt)
}

#[tauri::command]
pub fn user_capability_list(
    service: State<'_, ProjectService>,
    project_id: String,
    system_id: Option<String>,
) -> Result<Vec<UserCapability>, String> {
    service
        .store()
        .list_user_capabilities(&project_id, system_id.as_deref())
}

#[tauri::command]
pub fn user_capability_compile(
    service: State<'_, ProjectService>,
    project_id: String,
    request: CapabilityCompileRequest,
) -> Result<UserCapability, String> {
    service
        .store()
        .compile_user_capability(&project_id, &request)
}

#[tauri::command]
pub fn user_capability_compile_global(
    service: State<'_, ProjectService>,
    project_id: String,
    request: GlobalCapabilityCompileRequest,
) -> Result<UserCapability, String> {
    service
        .store()
        .compile_global_workflow_capability(&project_id, &request)
}

#[tauri::command]
pub fn user_capability_promote(
    service: State<'_, ProjectService>,
    project_id: String,
    request: CapabilityPromotionRequest,
    confirmed: bool,
) -> Result<CapabilityResolution, String> {
    if !confirmed {
        return Err(
            "CAPABILITY_PROMOTION_CONFIRMATION_REQUIRED: review shared scope before promotion"
                .to_string(),
        );
    }
    service
        .store()
        .promote_user_capability(&project_id, &request)
}

#[tauri::command]
pub fn user_capability_resolve(
    service: State<'_, ProjectService>,
    project_id: String,
    system_id: Option<String>,
) -> Result<Vec<CapabilityResolution>, String> {
    service
        .store()
        .resolve_user_capabilities(&project_id, system_id.as_deref())
}

#[tauri::command]
pub fn user_capability_versions(
    service: State<'_, ProjectService>,
    project_id: String,
    system_id: Option<String>,
) -> Result<Vec<CapabilityResolution>, String> {
    service
        .store()
        .list_user_capability_versions(&project_id, system_id.as_deref())
}

#[tauri::command]
pub fn user_capability_rollback(
    service: State<'_, ProjectService>,
    project_id: String,
    request: CapabilityRollbackRequest,
    confirmed: bool,
) -> Result<UserCapability, String> {
    if !confirmed {
        return Err(
            "CAPABILITY_ROLLBACK_CONFIRMATION_REQUIRED: review both versions before rollback"
                .to_string(),
        );
    }
    service
        .store()
        .rollback_user_capability(&project_id, &request)
}

#[tauri::command]
pub fn user_capability_set_shared_status(
    service: State<'_, ProjectService>,
    scope: String,
    capability_id: String,
    version: String,
    status: String,
    confirmed: bool,
) -> Result<UserCapability, String> {
    if status == "active" && !confirmed {
        return Err(
            "CAPABILITY_ACTIVATION_CONFIRMATION_REQUIRED: review and confirm before activation"
                .to_string(),
        );
    }
    service
        .store()
        .set_shared_capability_status(&scope, &capability_id, &version, &status)
}

#[tauri::command]
pub fn user_capability_validate_global(
    service: State<'_, ProjectService>,
    project_id: String,
    composite_id: String,
    capability_id: String,
    version: Option<String>,
) -> Result<UserCapability, String> {
    service.store().validate_global_capability_for_composite(
        &project_id,
        &composite_id,
        &capability_id,
        version.as_deref(),
    )
}

#[tauri::command]
pub fn user_capability_get(
    service: State<'_, ProjectService>,
    project_id: String,
    capability_id: String,
    version: Option<String>,
) -> Result<UserCapability, String> {
    service
        .store()
        .get_user_capability(&project_id, &capability_id, version.as_deref())
}

#[tauri::command]
pub fn user_capability_set_status(
    service: State<'_, ProjectService>,
    project_id: String,
    capability_id: String,
    version: String,
    status: String,
    confirmed: bool,
) -> Result<UserCapability, String> {
    if status == "active" && !confirmed {
        return Err(
            "CAPABILITY_ACTIVATION_CONFIRMATION_REQUIRED: review and confirm before activation"
                .to_string(),
        );
    }
    service
        .store()
        .set_user_capability_status(&project_id, &capability_id, &version, &status)
}

#[tauri::command]
pub fn domain_memory_list(
    service: State<'_, ProjectService>,
    project_id: String,
    system_id: String,
    active_only: bool,
) -> Result<Vec<DomainMemory>, String> {
    service
        .store()
        .list_domain_memories(&project_id, &system_id, active_only)
}

#[tauri::command]
pub fn domain_memory_save(
    service: State<'_, ProjectService>,
    project_id: String,
    mut memory: DomainMemory,
) -> Result<DomainMemory, String> {
    memory.scope = "project".to_string();
    memory.status = "candidate".to_string();
    service.store().save_domain_memory(&project_id, &memory)
}

#[tauri::command]
pub fn memory_candidate_list(
    service: State<'_, ProjectService>,
    project_id: String,
    system_id: Option<String>,
) -> Result<Vec<DomainMemory>, String> {
    service
        .store()
        .list_memory_candidates(&project_id, system_id.as_deref())
}

#[tauri::command]
pub fn memory_candidate_activate(
    service: State<'_, ProjectService>,
    project_id: String,
    memory_id: String,
    confirmed: bool,
) -> Result<DomainMemory, String> {
    if !confirmed {
        return Err(
            "MEMORY_ACTIVATION_CONFIRMATION_REQUIRED: review the proposed memory first".to_string(),
        );
    }
    service
        .store()
        .set_domain_memory_status(&project_id, &memory_id, "active")
}

#[tauri::command]
pub fn memory_candidate_contest(
    service: State<'_, ProjectService>,
    project_id: String,
    memory_id: String,
) -> Result<DomainMemory, String> {
    service
        .store()
        .set_domain_memory_status(&project_id, &memory_id, "contested")
}

#[tauri::command]
pub fn memory_candidate_revoke(
    service: State<'_, ProjectService>,
    project_id: String,
    memory_id: String,
) -> Result<DomainMemory, String> {
    service
        .store()
        .set_domain_memory_status(&project_id, &memory_id, "revoked")
}

#[tauri::command]
pub fn system_session_get(
    service: State<'_, ProjectService>,
    project_id: String,
    task_id: String,
) -> Result<Option<SystemSessionBinding>, String> {
    service.store().get_system_session(&project_id, &task_id)
}

#[tauri::command]
pub fn system_session_bind(
    service: State<'_, ProjectService>,
    project_id: String,
    binding: SystemSessionBinding,
) -> Result<SystemSessionBinding, String> {
    service.store().bind_system_session(&project_id, &binding)
}

#[tauri::command]
pub fn task_scope_issue(
    service: State<'_, ProjectService>,
    project_id: String,
    task_id: String,
    read_systems: Vec<String>,
    write_systems: Vec<String>,
    draft_ids: Vec<String>,
    plugin_versions: serde_json::Value,
    expires_at: i64,
) -> Result<TaskScopeLease, String> {
    service.store().issue_task_scope(
        &project_id,
        &task_id,
        &read_systems,
        &write_systems,
        &draft_ids,
        plugin_versions,
        expires_at,
    )
}

#[tauri::command]
pub fn task_scope_revoke(
    service: State<'_, ProjectService>,
    project_id: String,
    token: String,
) -> Result<(), String> {
    service.store().revoke_task_scope(&project_id, &token)
}

#[tauri::command]
pub fn draft_list(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<Vec<Draft>, String> {
    service.store().list_drafts(&project_id)
}

/// 为 Studio 人工编辑创建并绑定当前领域版本的外置 Draft。
#[tauri::command]
pub fn domain_draft_open(
    service: State<'_, ProjectService>,
    project_id: String,
    system_id: String,
    plugin_version: String,
    intent: String,
    composite_id: Option<String>,
) -> Result<Draft, String> {
    let description = service
        .store()
        .describe_domain_system(&project_id, &system_id)?;
    if description.manifest.version != plugin_version {
        return Err(format!(
            "DOMAIN_DRAFT_PLUGIN_VERSION_MISMATCH: expected {}, got {plugin_version}",
            description.manifest.version
        ));
    }
    let draft = service.store().open_draft(&project_id, &intent)?;
    if let Err(error) = service.store().bind_draft_domain(
        &project_id,
        &draft.id,
        &system_id,
        &plugin_version,
        composite_id.as_deref(),
    ) {
        let _ = service.store().discard_draft(&project_id, &draft.id);
        return Err(error);
    }
    Ok(draft)
}

/// 将已有的领域 Draft 安全关联到全局组合任务。
#[tauri::command]
pub fn domain_draft_composite_associate(
    service: State<'_, ProjectService>,
    project_id: String,
    draft_id: String,
    system_id: String,
    plugin_version: String,
    composite_id: String,
) -> Result<(), String> {
    service.store().associate_draft_composite(
        &project_id,
        &draft_id,
        &system_id,
        &plugin_version,
        &composite_id,
    )
}

/// 仅用于全局任务初始化失败后的精确关联补偿。
#[tauri::command]
pub fn domain_draft_composite_disassociate(
    service: State<'_, ProjectService>,
    project_id: String,
    draft_id: String,
    system_id: String,
    plugin_version: String,
    composite_id: String,
) -> Result<(), String> {
    service.store().disassociate_draft_composite(
        &project_id,
        &draft_id,
        &system_id,
        &plugin_version,
        &composite_id,
    )
}

#[tauri::command]
pub fn draft_preview(
    service: State<'_, ProjectService>,
    project_id: String,
    draft_id: String,
) -> Result<DraftConfirmation, String> {
    service.create_confirmation(&project_id, &draft_id)
}

#[tauri::command]
pub fn draft_legacy_clone(
    service: State<'_, ProjectService>,
    project_id: String,
    request: LegacyDraftCloneRequest,
) -> Result<DraftConfirmation, String> {
    let preview = service.store().clone_legacy_draft(&project_id, &request)?;
    service.create_confirmation(&project_id, &preview.draft.id)
}

#[tauri::command]
pub fn draft_apply(
    service: State<'_, ProjectService>,
    project_id: String,
    draft_id: String,
    confirmation_token: String,
) -> Result<Snapshot, String> {
    let confirmation = service.consume_confirmation(&project_id, &draft_id, &confirmation_token)?;
    let snapshot = service.store().apply_validated_domain_draft(
        &project_id,
        &draft_id,
        confirmation.revision,
        &confirmation.diff_hash,
    )?;
    if let Err(error) = service.store().record_applied_draft_receipt(
        &project_id,
        &draft_id,
        &confirmation.diff_hash,
        &snapshot,
    ) {
        log::error!("Task receipt write failed after applying {draft_id}: {error}");
    }
    Ok(snapshot)
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositeDraftApplyInput {
    pub draft_id: String,
    pub confirmation_token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositeDraftReviewItem {
    pub draft_id: String,
    pub system_id: String,
    pub plugin_version: String,
    pub confirmation: DraftConfirmation,
    pub validation: DomainValidationReport,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositeDraftReview {
    pub composite_id: String,
    pub drafts: Vec<CompositeDraftReviewItem>,
}

/// 联合审查一次返回完整绑定集合、真实 Diff、逐 Draft 校验和一次性确认令牌。
#[tauri::command]
pub fn draft_composite_preview(
    service: State<'_, ProjectService>,
    project_id: String,
    composite_id: String,
) -> Result<CompositeDraftReview, String> {
    let bindings = service
        .store()
        .list_composite_draft_bindings(&project_id, &composite_id)?;
    let mut drafts = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let validation = service
            .store()
            .validate_domain_draft(&project_id, &binding.draft_id)?;
        let confirmation = service.create_confirmation(&project_id, &binding.draft_id)?;
        drafts.push(CompositeDraftReviewItem {
            draft_id: binding.draft_id,
            system_id: binding.system_id,
            plugin_version: binding.plugin_version,
            confirmation,
            validation,
        });
    }
    Ok(CompositeDraftReview {
        composite_id,
        drafts,
    })
}

#[tauri::command]
pub fn draft_composite_apply(
    service: State<'_, ProjectService>,
    project_id: String,
    composite_id: String,
    drafts: Vec<CompositeDraftApplyInput>,
) -> Result<CompositeApplyResult, String> {
    let requests = drafts
        .iter()
        .map(|draft| (draft.draft_id.clone(), draft.confirmation_token.clone()))
        .collect::<Vec<_>>();
    let values = service.consume_composite_confirmations(&project_id, &requests)?;
    let mut confirmations = Vec::with_capacity(drafts.len());
    for (draft, confirmation) in drafts.into_iter().zip(values) {
        confirmations.push(CompositeDraftConfirmation {
            draft_id: draft.draft_id,
            expected_revision: confirmation.revision,
            expected_diff_hash: confirmation.diff_hash,
        });
    }
    let result = service.store().apply_validated_composite_drafts(
        &project_id,
        &composite_id,
        &confirmations,
    )?;
    for confirmation in &confirmations {
        if let Err(error) = service.store().record_applied_draft_receipt(
            &project_id,
            &confirmation.draft_id,
            &confirmation.expected_diff_hash,
            &result.snapshot,
        ) {
            log::error!(
                "Task receipt write failed after composite apply {}: {error}",
                confirmation.draft_id
            );
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn draft_discard(
    service: State<'_, ProjectService>,
    project_id: String,
    draft_id: String,
) -> Result<Draft, String> {
    service.store().discard_draft(&project_id, &draft_id)
}

#[tauri::command]
pub fn safe_file_open(
    service: State<'_, ProjectService>,
    project_id: String,
    relative_path: String,
    draft_id: Option<String>,
) -> Result<SafeTextOpen, String> {
    ensure_safe_project(&service, &project_id)?;
    service
        .store()
        .safe_text_open(&project_id, &relative_path, draft_id.as_deref())
}

#[tauri::command]
pub fn safe_text_patch(
    service: State<'_, ProjectService>,
    project_id: String,
    operation: SafeTextPatch,
) -> Result<SafeTextPatchResult, String> {
    ensure_safe_project(&service, &project_id)?;
    service.store().safe_text_patch(&project_id, &operation)
}

#[tauri::command]
pub fn safe_lua_patch(
    service: State<'_, ProjectService>,
    project_id: String,
    operation: SafeTextPatch,
) -> Result<SafeTextPatchResult, String> {
    ensure_safe_project(&service, &project_id)?;
    if !operation
        .relative_path
        .to_ascii_lowercase()
        .ends_with(".lua")
    {
        return Err("SAFE_LUA_TYPE_UNSUPPORTED: expected a .lua file".to_string());
    }
    service.store().safe_text_patch(&project_id, &operation)
}

#[tauri::command]
pub fn safe_xls_open(
    service: State<'_, ProjectService>,
    project_id: String,
    relative_path: String,
) -> Result<SafeXlsWorkbook, String> {
    ensure_safe_project(&service, &project_id)?;
    service.store().safe_xls_open(&project_id, &relative_path)
}

#[tauri::command]
pub fn safe_xls_sheet_read(
    service: State<'_, ProjectService>,
    project_id: String,
    relative_path: String,
    sheet: String,
    expected_sha256: String,
) -> Result<SafeXlsSheet, String> {
    ensure_safe_project(&service, &project_id)?;
    service
        .store()
        .safe_xls_sheet_read(&project_id, &relative_path, &sheet, &expected_sha256)
}

#[tauri::command]
pub fn safe_xls_patch(
    service: State<'_, ProjectService>,
    project_id: String,
    operation: SafeXlsDraftPatch,
) -> Result<SafeXlsPatchResult, String> {
    ensure_safe_project(&service, &project_id)?;
    service.store().safe_xls_patch(&project_id, &operation)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeFileStatus {
    pub available: bool,
    pub editable_extensions: Vec<&'static str>,
    pub read_only_extensions: Vec<&'static str>,
}

#[tauri::command]
pub fn safe_file_status() -> SafeFileStatus {
    SafeFileStatus {
        available: true,
        editable_extensions: vec!["txt", "lua", "xls"],
        read_only_extensions: Vec::new(),
    }
}

fn ensure_safe_project(service: &ProjectService, project_id: &str) -> Result<(), String> {
    let active = service
        .store()
        .active_project()?
        .ok_or_else(|| "SAFE_FILES_PROJECT_UNBOUND: no active MIR3 project".to_string())?;
    if active.id == project_id {
        Ok(())
    } else {
        Err("SAFE_FILES_PROJECT_MISMATCH: request is not for the active project".to_string())
    }
}

#[tauri::command]
pub fn snapshot_list(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<Vec<Snapshot>, String> {
    service.store().list_snapshots(&project_id)
}

#[tauri::command]
pub fn snapshot_create(
    service: State<'_, ProjectService>,
    project_id: String,
    paths: Vec<String>,
) -> Result<Snapshot, String> {
    service.store().create_snapshot(&project_id, None, &paths)
}

#[tauri::command]
pub fn snapshot_restore(
    service: State<'_, ProjectService>,
    project_id: String,
    snapshot_id: String,
) -> Result<Snapshot, String> {
    service.store().restore_snapshot(&project_id, &snapshot_id)
}

#[tauri::command]
pub fn knowledge_list(
    service: State<'_, ProjectService>,
    project_id: String,
    filter: KnowledgeFilter,
) -> Result<Vec<KnowledgeRecord>, String> {
    service.store().list_knowledge(&project_id, &filter)
}

#[tauri::command]
pub fn knowledge_get(
    service: State<'_, ProjectService>,
    project_id: String,
    knowledge_id: String,
) -> Result<KnowledgeRecord, String> {
    service.store().get_knowledge(&project_id, &knowledge_id)
}

#[tauri::command]
pub fn knowledge_set_status(
    service: State<'_, ProjectService>,
    project_id: String,
    knowledge_id: String,
    status: KnowledgeStatus,
) -> Result<KnowledgeRecord, String> {
    service
        .store()
        .set_knowledge_status(&project_id, &knowledge_id, status)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDiagnostics {
    pub data_root: String,
    pub active_project: Option<Mir3Project>,
    pub project_count: usize,
    pub scan: ScanState,
    pub mcp_binary: Option<String>,
}

#[tauri::command]
pub fn diagnostics_get(
    app: AppHandle,
    service: State<'_, ProjectService>,
) -> Result<ProjectDiagnostics, String> {
    let projects = service.store().list_projects()?;
    Ok(ProjectDiagnostics {
        data_root: service.store().data_root().to_string_lossy().into_owned(),
        active_project: service.store().active_project()?,
        project_count: projects.len(),
        scan: service.scan_state(),
        mcp_binary: crate::service::project::mcp_binary_path(&app)
            .filter(|path| path.is_file())
            .map(|path| path.to_string_lossy().into_owned()),
    })
}

#[tauri::command]
pub async fn core_mcp_canary_run(
    app: AppHandle,
) -> Result<crate::service::project::canary::CoreMcpCanaryReport, String> {
    crate::service::project::canary::run(&app).await
}
