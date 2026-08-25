//! GUI 场景素材的本地 Tauri 命令。
//!
//! 命令只返回逻辑素材 ID 与解析结果，绝对路径不会跨越 IPC 边界。

use crate::service::gui_scene_assets::{
    self, SceneAssetCatalog, SceneAssetMeta, SceneAtlasManifest, SceneEffectResolution,
    SceneLoginPresetManifest, SceneMapCapabilities, SceneWorldChunk, SceneWorldChunkRequest,
    SceneWorldManifest,
};
use crate::service::project::ProjectService;
use tauri::{ipc::Response, State};

#[tauri::command]
pub async fn gui_scene_asset_catalog(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<SceneAssetCatalog, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_scene_assets::scene_asset_catalog(&service, &project_id)
    })
    .await
    .map_err(|e| format!("GUI_SCENE_ASSET_CATALOG_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_scene_asset_manifest(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<SceneAssetCatalog, String> {
    gui_scene_asset_catalog(service, project_id).await
}

#[tauri::command]
pub async fn gui_scene_asset_meta(
    service: State<'_, ProjectService>,
    project_id: String,
    asset_id: String,
) -> Result<SceneAssetMeta, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_scene_assets::scene_asset_meta(&service, &project_id, &asset_id)
    })
    .await
    .map_err(|e| format!("GUI_SCENE_ASSET_META_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_scene_asset_read(
    service: State<'_, ProjectService>,
    project_id: String,
    asset_id: String,
) -> Result<Response, String> {
    let service = service.inner().clone();
    let content = tauri::async_runtime::spawn_blocking(move || {
        gui_scene_assets::read_scene_asset(&service, &project_id, &asset_id)
    })
    .await
    .map_err(|e| format!("GUI_SCENE_ASSET_READ_TASK_FAILED: {e}"))??;
    Ok(Response::new(content.bytes))
}

#[tauri::command]
pub async fn gui_scene_atlas_manifest(
    service: State<'_, ProjectService>,
    project_id: String,
    asset_id: String,
) -> Result<SceneAtlasManifest, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_scene_assets::scene_atlas_manifest(&service, &project_id, &asset_id)
    })
    .await
    .map_err(|e| format!("GUI_SCENE_ATLAS_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_scene_animation_manifest(
    service: State<'_, ProjectService>,
    project_id: String,
    asset_id: String,
) -> Result<SceneAtlasManifest, String> {
    gui_scene_atlas_manifest(service, project_id, asset_id).await
}

#[tauri::command]
pub async fn gui_scene_effect_resolve(
    service: State<'_, ProjectService>,
    project_id: String,
    effect_id: u32,
    preferred_module: Option<String>,
) -> Result<SceneEffectResolution, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_scene_assets::resolve_scene_effect(
            &service,
            &project_id,
            effect_id,
            preferred_module.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("GUI_SCENE_EFFECT_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_scene_map_capabilities(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<SceneMapCapabilities, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_scene_assets::scene_map_capabilities(&service, &project_id)
    })
    .await
    .map_err(|e| format!("GUI_SCENE_MAP_CAPABILITIES_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_scene_world_chunk(
    service: State<'_, ProjectService>,
    project_id: String,
    map_id: String,
    request: SceneWorldChunkRequest,
) -> Result<SceneWorldChunk, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_scene_assets::read_world_chunk(&service, &project_id, &map_id, request)
    })
    .await
    .map_err(|e| format!("GUI_SCENE_WORLD_CHUNK_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_scene_world_manifest(
    service: State<'_, ProjectService>,
    project_id: String,
    map_id: String,
    request: SceneWorldChunkRequest,
) -> Result<SceneWorldManifest, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_scene_assets::scene_world_manifest(&service, &project_id, &map_id, request)
    })
    .await
    .map_err(|e| format!("GUI_SCENE_WORLD_MANIFEST_TASK_FAILED: {e}"))?
}

#[tauri::command]
pub async fn gui_scene_login_presets(
    service: State<'_, ProjectService>,
    project_id: String,
) -> Result<Vec<SceneLoginPresetManifest>, String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        gui_scene_assets::scene_login_presets(&service, &project_id)
    })
    .await
    .map_err(|e| format!("GUI_SCENE_LOGIN_PRESETS_TASK_FAILED: {e}"))?
}
