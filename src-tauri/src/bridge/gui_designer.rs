//! GUI Designer 的本地 Tauri 命令。
//!
//! 这些命令只供 Studio 壳层调用，不注册到 MCP 或 Harness iframe。

use crate::service::gui_designer::{
    self, GuiAssetMeta, GuiDesignerStatus, GuiDevTreePage, GuiDocumentEntry, GuiDocumentEnvelope,
    GuiDocumentProbe, GuiDraftChangeSet, GuiDraftPrepareResult, GuiExternalChangeRequest,
    GuiExternalChangeResult, GuiGameProcessStatus, GuiReadonlyDocument, GuiReparseRequest,
    GuiSaveNode, GuiTemplateRequest, GuiTemplateResponse, GuiWorkingSaveResult,
};
use crate::service::project::{DraftConfirmation, ProjectService};
use mir3_domain::{
    GuiWorkspaceSnapshot, GuiWorkspaceSyncRequest, GuiWorkspaceSyncResult, Snapshot,
};
use tauri::{ipc::Response, State};

#[tauri::command]
pub fn gui_designer_status(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<GuiDesignerStatus, String> {
    gui_designer::status(&service, &project_id)
}

#[tauri::command]
pub async fn gui_document_list(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<Vec<GuiDocumentEntry>, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_designer::list_documents(&service, &project_id).map(|result| result.entries)
    })
    .await
    .map_err(|e| format!("GUI_DOCUMENT_LIST_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_document_open(
    service: State<'_, ProjectService>,
    project_id: String,
    dev_relative_path: String,
    draft_id: Option<String>,
) -> Result<GuiDocumentEnvelope, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_designer::open_document(
            &service,
            &project_id,
            &dev_relative_path,
            draft_id.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("GUI_DOCUMENT_OPEN_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_document_reparse(
    service: State<'_, ProjectService>,
    project_id: String,
    request: GuiReparseRequest,
) -> Result<GuiDocumentEnvelope, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_designer::reparse_document(&service, &project_id, request)
    })
    .await
    .map_err(|e| format!("GUI_DOCUMENT_REPARSE_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub fn gui_document_template(
    service: State<'_, ProjectService>,
    project_id: String,
    request: GuiTemplateRequest,
) -> Result<GuiTemplateResponse, String> {
    gui_designer::create_template(&service, &project_id, request)
}

#[tauri::command]
pub async fn gui_dev_tree_list(
    service: State<'_, ProjectService>,
    project_id: String,
    parent_path: String,
    cursor: Option<String>,
) -> Result<GuiDevTreePage, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_designer::list_dev_tree(&service, &project_id, &parent_path, cursor.as_deref())
    })
    .await
    .map_err(|e| format!("GUI_DEV_TREE_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_asset_meta(
    service: State<'_, ProjectService>,
    project_id: String,
    logical_path: String,
) -> Result<GuiAssetMeta, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_designer::read_asset_meta(&service, &project_id, &logical_path)
    })
    .await
    .map_err(|e| format!("GUI_ASSET_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_asset_read(
    service: State<'_, ProjectService>,
    project_id: String,
    logical_path: String,
) -> Result<Response, String> {
    let service = service.inner().clone();
    let content = tauri::async_runtime::spawn_blocking(move || {
        gui_designer::read_asset_content(&service, &project_id, &logical_path)
    })
    .await
    .map_err(|e| format!("GUI_ASSET_TASK_FAILED: {e}"))??;
    Ok(Response::new(content.bytes))
}

#[tauri::command]
pub async fn gui_readonly_document_open(
    service: State<'_, ProjectService>,
    project_id: String,
    dev_relative_path: String,
) -> Result<GuiReadonlyDocument, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_designer::open_readonly_document(&service, &project_id, &dev_relative_path)
    })
    .await
    .map_err(|e| format!("GUI_READONLY_DOCUMENT_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub fn gui_draft_prepare(
    service: State<'_, ProjectService>,
    project_id: String,
    change_set: GuiDraftChangeSet,
) -> Result<GuiDraftPrepareResult, String> {
    gui_designer::prepare_draft(&service, &project_id, change_set)
}

#[tauri::command]
pub fn gui_draft_confirm(
    service: State<'_, ProjectService>,
    project_id: String,
    draft_id: String,
) -> Result<DraftConfirmation, String> {
    gui_designer::confirm_draft(&service, &project_id, &draft_id)
}

#[tauri::command]
pub fn gui_draft_apply(
    service: State<'_, ProjectService>,
    project_id: String,
    draft_id: String,
    confirmation_token: String,
) -> Result<Snapshot, String> {
    gui_designer::apply_draft(&service, &project_id, &draft_id, &confirmation_token)
}

#[tauri::command]
pub async fn gui_working_save(
    service: State<'_, ProjectService>,
    project_id: String,
    change_set: GuiDraftChangeSet,
) -> Result<GuiWorkingSaveResult, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_designer::working_save(&service, &project_id, change_set)
    })
    .await
    .map_err(|e| format!("GUI_WORKING_SAVE_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_save_node_list(
    service: State<'_, ProjectService>,
    project_id: String,
    limit: usize,
) -> Result<Vec<GuiSaveNode>, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_designer::list_save_nodes(&service, &project_id, limit)
    })
    .await
    .map_err(|e| format!("GUI_SAVE_NODE_LIST_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_save_node_restore(
    service: State<'_, ProjectService>,
    project_id: String,
    node_id: String,
) -> Result<GuiWorkingSaveResult, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_designer::restore_save_node(&service, &project_id, &node_id)
    })
    .await
    .map_err(|e| format!("GUI_SAVE_NODE_RESTORE_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_document_probe(
    service: State<'_, ProjectService>,
    project_id: String,
    dev_relative_path: String,
    known_sha256: Option<String>,
    known_modified_at: Option<i64>,
    known_size: Option<u64>,
    force_hash: Option<bool>,
) -> Result<GuiDocumentProbe, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_designer::probe_document(
            &service,
            &project_id,
            &dev_relative_path,
            known_sha256.as_deref(),
            known_modified_at,
            known_size,
            force_hash.unwrap_or(false),
        )
    })
    .await
    .map_err(|e| format!("GUI_DOCUMENT_PROBE_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_external_change_record(
    service: State<'_, ProjectService>,
    project_id: String,
    request: GuiExternalChangeRequest,
) -> Result<GuiExternalChangeResult, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_designer::record_external_change(&service, &project_id, request)
    })
    .await
    .map_err(|e| format!("GUI_EXTERNAL_CHANGE_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_game_process_status(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<GuiGameProcessStatus, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_designer::game_process_status(&service, &project_id)
    })
    .await
    .map_err(|e| format!("GUI_GAME_PROCESS_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_ai_workspace_sync(
    service: State<'_, ProjectService>,
    project_id: String,
    context: GuiWorkspaceSyncRequest,
) -> Result<GuiWorkspaceSyncResult, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.store().sync_gui_workspace(&project_id, &context)
    })
    .await
    .map_err(|e| format!("GUI_AI_WORKSPACE_SYNC_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_ai_workspace_get(
    service: State<'_, ProjectService>,
    project_id: String,
    path: String,
) -> Result<GuiWorkspaceSnapshot, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.store().get_gui_workspace(&project_id, &path)
    })
    .await
    .map_err(|e| format!("GUI_AI_WORKSPACE_GET_TASK_FAILED: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::ipc::{InvokeResponseBody, IpcResponse};

    #[test]
    fn asset_response_uses_raw_ipc_bytes() {
        let response = Response::new(vec![0_u8, 1, 2, 255]);
        match response.body().unwrap() {
            InvokeResponseBody::Raw(bytes) => assert_eq!(bytes, vec![0, 1, 2, 255]),
            InvokeResponseBody::Json(_) => panic!("素材响应不能经过 JSON 编码"),
        }
    }
}
