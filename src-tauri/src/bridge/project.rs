//! 996 项目、索引、Draft、备份和知识治理的 Tauri 命令。

use crate::service::project::{DraftConfirmation, ProjectService, ScanState};
use mir3_domain::{
    Draft, IndexQuery, IndexRecord, IndexStats, KnowledgeFilter, KnowledgeRecord, KnowledgeStatus,
    Mir3Project, Snapshot, WorkspaceDirectory,
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
