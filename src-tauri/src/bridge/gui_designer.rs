//! GUI Designer 的本地 Tauri 命令。
//!
//! 这些命令只供 Studio 壳层调用，不注册到 MCP 或 Harness iframe。

use crate::service::gui_designer::{
    self, GuiAsset, GuiDesignerStatus, GuiDocumentEntry, GuiDocumentEnvelope, GuiDraftChangeSet,
    GuiDraftPrepareResult, GuiReparseRequest, GuiTemplateRequest, GuiTemplateResponse,
};
use crate::service::project::{DraftConfirmation, ProjectService};
use mir3_domain::Snapshot;
use tauri::State;

#[tauri::command]
pub fn gui_designer_status(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<GuiDesignerStatus, String> {
    gui_designer::status(&service, &project_id)
}

#[tauri::command]
pub fn gui_document_list(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<Vec<GuiDocumentEntry>, String> {
    gui_designer::list_documents(&service, &project_id).map(|result| result.entries)
}

#[tauri::command]
pub fn gui_document_open(
    service: State<'_, ProjectService>,
    project_id: String,
    dev_relative_path: String,
    draft_id: Option<String>,
) -> Result<GuiDocumentEnvelope, String> {
    gui_designer::open_document(
        &service,
        &project_id,
        &dev_relative_path,
        draft_id.as_deref(),
    )
}

#[tauri::command]
pub fn gui_document_reparse(
    service: State<'_, ProjectService>,
    project_id: String,
    request: GuiReparseRequest,
) -> Result<GuiDocumentEnvelope, String> {
    gui_designer::reparse_document(&service, &project_id, request)
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
pub fn gui_asset_read(
    service: State<'_, ProjectService>,
    project_id: String,
    logical_path: String,
) -> Result<GuiAsset, String> {
    gui_designer::read_asset(&service, &project_id, &logical_path)
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
