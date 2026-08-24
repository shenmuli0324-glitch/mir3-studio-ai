//! 996 项目、索引、Draft、备份和知识治理的 Tauri 命令。

use crate::service::gui_runtime::GuiRuntimeService;
use crate::service::project::{DraftConfirmation, ProjectService, ScanState};
use mir3_domain::{
    Draft, IndexQuery, IndexRecord, IndexStats, KnowledgeFilter, KnowledgeRecord, KnowledgeStatus,
    Mir3Project, SafeTextOpen, SafeTextPatch, SafeTextPatchResult, SafeXlsSheet, SafeXlsWorkbook,
    Snapshot, WorkspaceDirectory,
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
    runtime_service: State<'_, GuiRuntimeService>,
    project_id: String,
) -> Result<Mir3Project, String> {
    let previous = service.store().active_project()?;
    let project = service.store().activate_project(&project_id)?;
    if let Some(previous) = previous {
        if previous.id != project.id {
            runtime_service.stop_project_sessions(&previous.id);
        }
    }
    Ok(project)
}

#[tauri::command]
pub fn project_relink(
    service: State<'_, ProjectService>,
    runtime_service: State<'_, GuiRuntimeService>,
    project_id: String,
    path: String,
) -> Result<Mir3Project, String> {
    let project = service
        .store()
        .relink_project(&project_id, Path::new(&path))?;
    runtime_service.stop_project_sessions(&project_id);
    Ok(project)
}

#[tauri::command]
pub fn project_remove(
    service: State<'_, ProjectService>,
    runtime_service: State<'_, GuiRuntimeService>,
    project_id: String,
) -> Result<(), String> {
    service.store().remove_project(&project_id)?;
    runtime_service.stop_project_sessions(&project_id);
    Ok(())
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
pub fn draft_list(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<Vec<Draft>, String> {
    service.store().list_drafts(&project_id)
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
pub fn draft_apply(
    service: State<'_, ProjectService>,
    project_id: String,
    draft_id: String,
    confirmation_token: String,
) -> Result<Snapshot, String> {
    let confirmation = service.consume_confirmation(&project_id, &draft_id, &confirmation_token)?;
    service.store().apply_draft(
        &project_id,
        &draft_id,
        confirmation.revision,
        &confirmation.diff_hash,
    )
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
pub fn safe_xls_patch() -> Result<(), String> {
    Err("SAFE_XLS_READ_ONLY: structured XLS Draft editing is planned for plugin 0.2.0".to_string())
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
        editable_extensions: vec!["txt", "lua"],
        read_only_extensions: vec!["xls"],
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
