//! GUI Runtime 的 Studio 专用命令。

use crate::service::gui_runtime::{
    self, GuiRuntimeService, RuntimeCapabilities, RuntimeCatalog, RuntimeDataSource,
    RuntimeSceneResponse, RuntimeSceneStartRequest, RuntimeStopResponse,
};
use crate::service::project::ProjectService;
use serde_json::Value;
use std::collections::BTreeMap;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn gui_runtime_capabilities(
    app: AppHandle,
    project_service: State<'_, ProjectService>,
    runtime_service: State<'_, GuiRuntimeService>,
    project_id: String,
) -> Result<RuntimeCapabilities, String> {
    let project_service = project_service.inner().clone();
    let runtime_service = runtime_service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_runtime::capabilities(&app, &project_service, &runtime_service, &project_id)
    })
    .await
    .map_err(|error| format!("GUI_RUNTIME_CAPABILITIES_TASK_FAILED: {error}"))?
}

#[tauri::command]
pub async fn gui_runtime_catalog(
    project_service: State<'_, ProjectService>,
    project_id: String,
) -> Result<RuntimeCatalog, String> {
    let project_service = project_service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_runtime::catalog(&project_service, &project_id)
    })
    .await
    .map_err(|error| format!("GUI_RUNTIME_CATALOG_TASK_FAILED: {error}"))?
}

#[tauri::command]
pub async fn gui_runtime_scene_start(
    app: AppHandle,
    project_service: State<'_, ProjectService>,
    runtime_service: State<'_, GuiRuntimeService>,
    project_id: String,
    request: RuntimeSceneStartRequest,
) -> Result<RuntimeSceneResponse, String> {
    let project_service = project_service.inner().clone();
    let runtime_service = runtime_service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_runtime::start_scene(
            &app,
            &project_service,
            &runtime_service,
            &project_id,
            request,
        )
    })
    .await
    .map_err(|error| format!("GUI_RUNTIME_START_TASK_FAILED: {error}"))?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn gui_runtime_scene_event(
    app: AppHandle,
    project_service: State<'_, ProjectService>,
    runtime_service: State<'_, GuiRuntimeService>,
    project_id: String,
    session_id: String,
    node_id: String,
    event_type: String,
    payload: Value,
    expected_sequence: u64,
) -> Result<RuntimeSceneResponse, String> {
    let project_service = project_service.inner().clone();
    let runtime_service = runtime_service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_runtime::scene_event(
            &app,
            &project_service,
            &runtime_service,
            &project_id,
            &session_id,
            &node_id,
            &event_type,
            payload,
            expected_sequence,
        )
    })
    .await
    .map_err(|error| format!("GUI_RUNTIME_EVENT_TASK_FAILED: {error}"))?
}

#[tauri::command]
pub async fn gui_runtime_scene_reload(
    app: AppHandle,
    project_service: State<'_, ProjectService>,
    runtime_service: State<'_, GuiRuntimeService>,
    project_id: String,
    session_id: String,
    working_sources: BTreeMap<String, String>,
) -> Result<RuntimeSceneResponse, String> {
    let project_service = project_service.inner().clone();
    let runtime_service = runtime_service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_runtime::reload_scene(
            &app,
            &project_service,
            &runtime_service,
            &project_id,
            &session_id,
            working_sources,
        )
    })
    .await
    .map_err(|error| format!("GUI_RUNTIME_RELOAD_TASK_FAILED: {error}"))?
}

#[tauri::command]
pub fn gui_runtime_scene_stop(
    project_service: State<'_, ProjectService>,
    runtime_service: State<'_, GuiRuntimeService>,
    project_id: String,
    session_id: String,
) -> Result<RuntimeStopResponse, String> {
    let active = project_service
        .store()
        .active_project()?
        .ok_or_else(|| "GUI_RUNTIME_PROJECT_REQUIRED: no active project".to_string())?;
    if active.id != project_id {
        return Err("GUI_RUNTIME_PROJECT_MISMATCH: project is not active".to_string());
    }
    runtime_service.stop_session(&project_id, &session_id)?;
    Ok(RuntimeStopResponse { stopped: true })
}

#[tauri::command]
pub async fn gui_runtime_data_source_set(
    app: AppHandle,
    project_service: State<'_, ProjectService>,
    runtime_service: State<'_, GuiRuntimeService>,
    project_id: String,
    mode: RuntimeDataSource,
) -> Result<RuntimeCapabilities, String> {
    let active = project_service
        .store()
        .active_project()?
        .ok_or_else(|| "GUI_RUNTIME_PROJECT_REQUIRED: no active project".to_string())?;
    if active.id != project_id {
        return Err("GUI_RUNTIME_PROJECT_MISMATCH: project is not active".to_string());
    }
    if mode == RuntimeDataSource::ProjectStatic {
        let current = gui_runtime::capabilities(
            &app,
            project_service.inner(),
            runtime_service.inner(),
            &project_id,
        )?;
        if !current.project_static_available {
            return Err(
                "GUI_RUNTIME_STATIC_CONFIG_MISSING: project has no allowed XLS configuration"
                    .to_string(),
            );
        }
    }
    runtime_service.set_data_source(&project_id, mode)?;
    let project_service = project_service.inner().clone();
    let runtime_service = runtime_service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_runtime::capabilities(&app, &project_service, &runtime_service, &project_id)
    })
    .await
    .map_err(|error| format!("GUI_RUNTIME_DATA_SOURCE_TASK_FAILED: {error}"))?
}
