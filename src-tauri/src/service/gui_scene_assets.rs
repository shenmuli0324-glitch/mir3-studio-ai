//! GUI 场景素材的受控只读解析。
//!
//! 本模块把客户端 DEV 素材与已下载的模块缓存暴露为不含绝对路径的逻辑 ID。
//! 它不执行客户端程序，也不会让 Lua Runtime 获得文件系统能力。

use crate::service::project::ProjectService;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use texture2ddecoder::{
    decode_eacr, decode_eacr_signed, decode_eacrg, decode_eacrg_signed, decode_etc1,
    decode_etc2_rgb, decode_etc2_rgba1, decode_etc2_rgba8,
};

const MAX_SCENE_ASSET_SOURCE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_SCENE_ASSET_DECODED_BYTES: usize = 64 * 1024 * 1024;
const MAX_ATLAS_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_MAP_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
const MAP_HEADER_BYTES: usize = 28;
const MAP_TILE_RECORD_BYTES: usize = 3;
const MAP_CELL_RECORD_BYTES: usize = 14;
const MAX_WORLD_CHUNK_EDGE: u32 = 128;
const SUPPORTED_MAP_IDS: [&str; 4] = ["01", "1", "d021", "d032"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneAssetCatalog {
    pub project_id: String,
    pub dev_available: bool,
    pub stab_available: bool,
    pub modules: Vec<SceneAssetModule>,
    pub accepted_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneAssetModule {
    pub id: String,
    pub asset_root_id: String,
    pub has_res: bool,
    pub has_anim: bool,
    pub has_scene: bool,
}

#[derive(Debug, Clone)]
pub struct SceneAssetBinary {
    pub asset_id: String,
    pub mime_type: String,
    pub texture_format: Option<String>,
    pub transcoded: bool,
    pub diagnostic: Option<String>,
    pub bytes: Vec<u8>,
    pub source_byte_length: u64,
    pub source_sha256: String,
    pub content_sha256: String,
    pub gzip_wrapped: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneAssetMeta {
    pub asset_id: String,
    pub mime_type: String,
    pub texture_format: Option<String>,
    pub browser_renderable: bool,
    pub transcoded: bool,
    pub diagnostic: Option<String>,
    pub source_byte_length: u64,
    pub content_byte_length: u64,
    pub source_sha256: String,
    pub content_sha256: String,
    pub gzip_wrapped: bool,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneAtlasManifest {
    pub asset_id: String,
    pub texture_asset_id: String,
    pub texture_width: Option<u32>,
    pub texture_height: Option<u32>,
    pub frames: Vec<SceneAtlasFrame>,
    pub source_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneAtlasFrame {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub source_width: i32,
    pub source_height: i32,
    pub rotated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneEffectResolution {
    pub effect_id: u32,
    pub available: bool,
    pub source: Option<String>,
    pub atlas_asset_ids: Vec<String>,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneMapCapabilities {
    pub adapter: String,
    pub map_ids: Vec<SceneMapCapability>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneMapCapability {
    pub map_id: String,
    pub available: bool,
    pub asset_id: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<String>,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneWorldChunkRequest {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneWorldChunk {
    pub map_id: String,
    pub map_asset_id: String,
    pub map_width: u32,
    pub map_height: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub tile_record_bytes: u32,
    pub cell_record_bytes: u32,
    pub tiles: Vec<SceneWorldTile>,
    pub cells: Vec<SceneWorldCell>,
    pub source_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneWorldTile {
    pub x: u32,
    pub y: u32,
    pub file_index: u8,
    pub image_index: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneWorldCell {
    pub x: u32,
    pub y: u32,
    pub flags: u8,
    pub raw: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SceneWorldManifestStatus {
    Supported,
    Partial,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SceneWorldLayer {
    Ground,
    Bottom,
    Top,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneWorldManifest {
    pub status: SceneWorldManifestStatus,
    pub chunk: SceneWorldChunk,
    pub frames: Vec<SceneWorldFrameReference>,
    pub atlas_asset_ids: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneWorldFrameReference {
    pub layer: SceneWorldLayer,
    pub x: u32,
    pub y: u32,
    pub file_index: u8,
    pub image_index: u16,
    pub frame_index: u16,
    pub atlas_index: Option<u32>,
    pub atlas_asset_id: Option<String>,
    pub available: bool,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneLoginPresetManifest {
    pub scene_id: String,
    pub background_asset_id: String,
    pub background_available: bool,
    pub effects: Vec<SceneEffectResolution>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
struct SceneAtlasConfig {
    formats: BTreeMap<u16, String>,
    splits: BTreeMap<u16, u32>,
}

#[derive(Debug, Clone)]
struct PkmInfo {
    format: u16,
    format_name: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone)]
struct PreparedSceneImage {
    bytes: Vec<u8>,
    mime_type: &'static str,
    texture_format: Option<String>,
    transcoded: bool,
    diagnostic: Option<String>,
}

#[derive(Debug, Clone)]
struct SceneProjectContext {
    project_id: String,
    dev_res_root: Option<PathBuf>,
    cache_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
enum SceneAssetSource {
    Dev,
    Stab,
    Module(String),
}

#[derive(Debug, Clone)]
struct ParsedAssetId {
    source: SceneAssetSource,
    relative: String,
}

#[derive(Debug, Clone)]
struct ResolvedAsset {
    parsed: ParsedAssetId,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct ParsedMap<'a> {
    bytes: &'a [u8],
    width: u32,
    height: u32,
    tile_offset: usize,
    cell_offset: usize,
    format: String,
}

#[derive(Debug, Clone)]
enum PlistValue {
    Dict(BTreeMap<String, PlistValue>),
    Array,
    String(String),
    Integer(i64),
    Real,
    Bool(bool),
}

#[derive(Debug, Clone)]
enum PlistToken {
    DictStart,
    DictEnd,
    ArrayStart,
    ArrayEnd,
    Key(String),
    String(String),
    Integer(i64),
    Real,
    Bool(bool),
}

pub fn scene_asset_catalog(
    project_service: &ProjectService,
    project_id: &str,
) -> Result<SceneAssetCatalog, String> {
    let context = scene_project_context(project_service, project_id)?;
    let modules = list_modules(&context)?;
    Ok(SceneAssetCatalog {
        project_id: context.project_id,
        dev_available: context.dev_res_root.is_some(),
        stab_available: context
            .cache_root
            .as_ref()
            .is_some_and(|root| root.join("stab").is_dir()),
        modules,
        accepted_asset_ids: vec![
            "dev://res/<path>".to_string(),
            "cache://stab/<res|anim|scene>/<path>".to_string(),
            "cache://module/<id>/<res|anim|scene>/<path>".to_string(),
        ],
    })
}

pub fn read_scene_asset(
    project_service: &ProjectService,
    project_id: &str,
    asset_id: &str,
) -> Result<SceneAssetBinary, String> {
    let context = scene_project_context(project_service, project_id)?;
    let resolved = resolve_asset(&context, asset_id, &["png", "jpg", "jpeg"])?;
    let source = read_limited_file(
        &resolved.path,
        MAX_SCENE_ASSET_SOURCE_BYTES,
        "GUI_SCENE_ASSET",
    )?;
    let source_byte_length = source.len() as u64;
    let source_sha256 = hash_bytes(&source);
    let (decoded, gzip_wrapped) = decode_gzip_if_needed(&source)?;
    let prepared = prepare_scene_image(&resolved.path, decoded)?;
    Ok(SceneAssetBinary {
        asset_id: asset_id.to_string(),
        mime_type: prepared.mime_type.to_string(),
        texture_format: prepared.texture_format,
        transcoded: prepared.transcoded,
        diagnostic: prepared.diagnostic,
        content_sha256: hash_bytes(&prepared.bytes),
        source_sha256,
        source_byte_length,
        bytes: prepared.bytes,
        gzip_wrapped,
    })
}

pub fn scene_asset_meta(
    project_service: &ProjectService,
    project_id: &str,
    asset_id: &str,
) -> Result<SceneAssetMeta, String> {
    let content = read_scene_asset(project_service, project_id, asset_id)?;
    let (width, height) = image_dimensions(&content.bytes, &content.mime_type)?;
    let browser_renderable = content.mime_type != "image/x-pkm";
    Ok(SceneAssetMeta {
        asset_id: content.asset_id,
        mime_type: content.mime_type,
        texture_format: content.texture_format,
        browser_renderable,
        transcoded: content.transcoded,
        diagnostic: content.diagnostic,
        source_byte_length: content.source_byte_length,
        content_byte_length: content.bytes.len() as u64,
        source_sha256: content.source_sha256,
        content_sha256: content.content_sha256,
        gzip_wrapped: content.gzip_wrapped,
        width,
        height,
    })
}

pub fn scene_atlas_manifest(
    project_service: &ProjectService,
    project_id: &str,
    asset_id: &str,
) -> Result<SceneAtlasManifest, String> {
    let context = scene_project_context(project_service, project_id)?;
    let resolved = resolve_asset(&context, asset_id, &["plist"])?;
    let source = read_limited_file(&resolved.path, MAX_ATLAS_SOURCE_BYTES, "GUI_SCENE_ATLAS")?;
    let root = parse_plist(&source)?;
    let root_dict = plist_dict(&root, "GUI_SCENE_ATLAS_ROOT_INVALID")?;
    let frames_dict = root_dict
        .get("frames")
        .ok_or_else(|| "GUI_SCENE_ATLAS_FRAMES_MISSING: plist 缺少 frames".to_string())
        .and_then(|value| plist_dict(value, "GUI_SCENE_ATLAS_FRAMES_INVALID"))?;
    let metadata = root_dict.get("metadata").and_then(plist_dict_optional);
    let texture_name = metadata
        .and_then(|values| {
            values
                .get("realTextureFileName")
                .or_else(|| values.get("textureFileName"))
        })
        .and_then(plist_string_optional)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            resolved
                .path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(|value| format!("{value}.png"))
                .unwrap_or_else(|| "atlas.png".to_string())
        });
    let texture_asset_id = sibling_asset_id(&resolved.parsed, &texture_name)?;
    let texture_size = metadata
        .and_then(|values| values.get("size"))
        .and_then(plist_string_optional)
        .and_then(|value| {
            parse_numbers(value)
                .get(0..2)
                .map(|items| (items[0], items[1]))
        });
    let mut frames = Vec::with_capacity(frames_dict.len());
    for (name, value) in frames_dict {
        let values = plist_dict(value, "GUI_SCENE_ATLAS_FRAME_INVALID")?;
        let rectangle = plist_field_numbers(values, "frame", 4)?;
        let offset = plist_field_numbers_default(values, "offset", &[0, 0]);
        let source_size = plist_field_numbers_default(
            values,
            "sourceSize",
            &[rectangle[2].abs(), rectangle[3].abs()],
        );
        frames.push(SceneAtlasFrame {
            name: name.clone(),
            x: rectangle[0],
            y: rectangle[1],
            width: rectangle[2],
            height: rectangle[3],
            offset_x: offset[0],
            offset_y: offset[1],
            source_width: source_size[0],
            source_height: source_size[1],
            rotated: values
                .get("rotated")
                .and_then(plist_bool_optional)
                .unwrap_or(false),
        });
    }
    Ok(SceneAtlasManifest {
        asset_id: asset_id.to_string(),
        texture_asset_id,
        texture_width: texture_size.and_then(|value| u32::try_from(value.0).ok()),
        texture_height: texture_size.and_then(|value| u32::try_from(value.1).ok()),
        frames,
        source_sha256: hash_bytes(&source),
    })
}

pub fn resolve_scene_effect(
    project_service: &ProjectService,
    project_id: &str,
    effect_id: u32,
    preferred_module: Option<&str>,
) -> Result<SceneEffectResolution, String> {
    let context = scene_project_context(project_service, project_id)?;
    let modules = effect_search_modules(&context, preferred_module)?;
    let prefix = format!("sfx_{effect_id}_");
    for (source, root, source_name) in modules {
        let directory = root.join("anim/effect");
        if !directory.is_dir() {
            continue;
        }
        ensure_no_symlink_components(&root, &directory)?;
        let mut names = Vec::new();
        let entries = fs::read_dir(&directory)
            .map_err(|e| format!("GUI_SCENE_EFFECT_LIST_FAILED: {}: {e}", directory.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("GUI_SCENE_EFFECT_LIST_FAILED: {e}"))?;
            let file_type = entry
                .file_type()
                .map_err(|e| format!("GUI_SCENE_EFFECT_METADATA_FAILED: {e}"))?;
            if !file_type.is_file() || file_type.is_symlink() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let lower = name.to_ascii_lowercase();
            if lower.starts_with(&prefix)
                && lower.ends_with(".plist")
                && !lower.ends_with("-1.plist")
            {
                names.push(name);
            }
        }
        names.sort_by(|left, right| natural_name_key(left).cmp(&natural_name_key(right)));
        if !names.is_empty() {
            let atlas_asset_ids = names
                .iter()
                .map(|name| effect_asset_id(&source, name))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(SceneEffectResolution {
                effect_id,
                available: true,
                source: Some(source_name),
                atlas_asset_ids,
                diagnostic: None,
            });
        }
    }
    Ok(SceneEffectResolution {
        effect_id,
        available: false,
        source: None,
        atlas_asset_ids: Vec::new(),
        diagnostic: Some("GUI_SCENE_EFFECT_MISSING: 本地缓存中没有该 Effect 图集".to_string()),
    })
}

pub fn scene_map_capabilities(
    project_service: &ProjectService,
    project_id: &str,
) -> Result<SceneMapCapabilities, String> {
    let context = scene_project_context(project_service, project_id)?;
    let mut map_ids = Vec::with_capacity(SUPPORTED_MAP_IDS.len());
    for map_id in SUPPORTED_MAP_IDS {
        map_ids.push(map_capability(&context, map_id));
    }
    Ok(SceneMapCapabilities {
        adapter: "aragom-3.1-minimal".to_string(),
        map_ids,
    })
}

pub fn read_world_chunk(
    project_service: &ProjectService,
    project_id: &str,
    map_id: &str,
    request: SceneWorldChunkRequest,
) -> Result<SceneWorldChunk, String> {
    validate_map_id(map_id)?;
    validate_world_chunk_request(request)?;
    let context = scene_project_context(project_service, project_id)?;
    let (map_asset_id, map_path) = resolve_map(&context, map_id)?;
    let bytes = read_limited_file(&map_path, MAX_MAP_SOURCE_BYTES, "GUI_SCENE_MAP")?;
    let parsed = parse_map(&bytes)?;
    let end_x = request
        .x
        .checked_add(request.width)
        .ok_or_else(|| "GUI_SCENE_MAP_CHUNK_OUTSIDE: X 范围溢出".to_string())?;
    let end_y = request
        .y
        .checked_add(request.height)
        .ok_or_else(|| "GUI_SCENE_MAP_CHUNK_OUTSIDE: Y 范围溢出".to_string())?;
    if end_x > parsed.width || end_y > parsed.height {
        return Err("GUI_SCENE_MAP_CHUNK_OUTSIDE: 请求区域超出地图边界".to_string());
    }
    let mut tiles = Vec::new();
    let tile_start_x = request.x - request.x % 2;
    let tile_start_y = request.y - request.y % 2;
    for y in (tile_start_y..end_y).step_by(2) {
        for x in (tile_start_x..end_x).step_by(2) {
            let tile_index = ((y / 2) * (parsed.width / 2) + x / 2) as usize;
            let offset = parsed.tile_offset + tile_index * MAP_TILE_RECORD_BYTES;
            let record = &parsed.bytes[offset..offset + MAP_TILE_RECORD_BYTES];
            tiles.push(SceneWorldTile {
                x,
                y,
                file_index: record[0],
                image_index: u16::from_le_bytes([record[1], record[2]]),
            });
        }
    }
    let mut cells = Vec::with_capacity((request.width * request.height) as usize);
    for y in request.y..end_y {
        for x in request.x..end_x {
            let cell_index = (y * parsed.width + x) as usize;
            let offset = parsed.cell_offset + cell_index * MAP_CELL_RECORD_BYTES;
            let raw = parsed.bytes[offset..offset + MAP_CELL_RECORD_BYTES].to_vec();
            cells.push(SceneWorldCell {
                x,
                y,
                flags: raw[0],
                raw,
            });
        }
    }
    Ok(SceneWorldChunk {
        map_id: map_id.to_string(),
        map_asset_id,
        map_width: parsed.width,
        map_height: parsed.height,
        x: request.x,
        y: request.y,
        width: request.width,
        height: request.height,
        tile_record_bytes: MAP_TILE_RECORD_BYTES as u32,
        cell_record_bytes: MAP_CELL_RECORD_BYTES as u32,
        tiles,
        cells,
        source_sha256: hash_bytes(&bytes),
    })
}

pub fn scene_world_manifest(
    project_service: &ProjectService,
    project_id: &str,
    map_id: &str,
    request: SceneWorldChunkRequest,
) -> Result<SceneWorldManifest, String> {
    let context = scene_project_context(project_service, project_id)?;
    let chunk = read_world_chunk(project_service, project_id, map_id, request)?;
    let atlas_config = match load_scene_atlas_config(&context) {
        Ok(value) => value,
        Err(error) => {
            return Ok(SceneWorldManifest {
                status: SceneWorldManifestStatus::Unsupported,
                chunk,
                frames: Vec::new(),
                atlas_asset_ids: Vec::new(),
                diagnostics: vec![error],
            })
        }
    };
    let preferred_module = module_id_from_asset_id(&chunk.map_asset_id);
    let mut frames = Vec::new();
    for tile in &chunk.tiles {
        if tile.file_index == u8::MAX || tile.image_index == u16::MAX {
            continue;
        }
        frames.push(resolve_world_frame(
            &context,
            &atlas_config,
            preferred_module.as_deref(),
            SceneWorldLayer::Ground,
            tile.x,
            tile.y,
            tile.file_index,
            tile.image_index,
        ));
    }
    for cell in &chunk.cells {
        if cell.raw.len() != MAP_CELL_RECORD_BYTES {
            continue;
        }
        let bottom_image = u16::from_le_bytes([cell.raw[5], cell.raw[6]]);
        if bottom_image != u16::MAX && cell.raw[4] != u8::MAX {
            frames.push(resolve_world_frame(
                &context,
                &atlas_config,
                preferred_module.as_deref(),
                SceneWorldLayer::Bottom,
                cell.x,
                cell.y,
                cell.raw[4],
                bottom_image,
            ));
        }
        let top_image = u16::from_le_bytes([cell.raw[7], cell.raw[8]]);
        if top_image != u16::MAX && cell.raw[3] != u8::MAX {
            frames.push(resolve_world_frame(
                &context,
                &atlas_config,
                preferred_module.as_deref(),
                SceneWorldLayer::Top,
                cell.x,
                cell.y,
                cell.raw[3],
                top_image,
            ));
        }
    }
    let mut atlas_asset_ids = frames
        .iter()
        .filter_map(|frame| frame.atlas_asset_id.clone())
        .collect::<Vec<_>>();
    atlas_asset_ids.sort();
    atlas_asset_ids.dedup();
    let mut diagnostics = frames
        .iter()
        .filter_map(|frame| frame.diagnostic.clone())
        .collect::<Vec<_>>();
    diagnostics.sort();
    diagnostics.dedup();
    let available = frames.iter().filter(|frame| frame.available).count();
    let status = if frames.is_empty() || available == 0 {
        SceneWorldManifestStatus::Unsupported
    } else if available == frames.len() {
        SceneWorldManifestStatus::Supported
    } else {
        SceneWorldManifestStatus::Partial
    };
    Ok(SceneWorldManifest {
        status,
        chunk,
        frames,
        atlas_asset_ids,
        diagnostics,
    })
}

pub fn scene_login_presets(
    project_service: &ProjectService,
    project_id: &str,
) -> Result<Vec<SceneLoginPresetManifest>, String> {
    let presets = [
        (
            "character-create",
            "dev://res/private/login/create_bg.jpg",
            &[3061u32, 3062u32][..],
        ),
        (
            "character-select",
            "dev://res/private/login/bg_cjzy_02.jpg",
            &[3005u32, 3006u32][..],
        ),
    ];
    let mut result = Vec::with_capacity(presets.len());
    for (scene_id, background_asset_id, effect_ids) in presets {
        let background_result = scene_asset_meta(project_service, project_id, background_asset_id);
        let mut diagnostics = Vec::new();
        let background_available = match background_result {
            Ok(_) => true,
            Err(error) => {
                diagnostics.push(error);
                false
            }
        };
        let mut effects = Vec::with_capacity(effect_ids.len());
        for effect_id in effect_ids {
            let effect = resolve_scene_effect(project_service, project_id, *effect_id, None)?;
            if let Some(diagnostic) = &effect.diagnostic {
                diagnostics.push(diagnostic.clone());
            }
            effects.push(effect);
        }
        diagnostics.sort();
        diagnostics.dedup();
        result.push(SceneLoginPresetManifest {
            scene_id: scene_id.to_string(),
            background_asset_id: background_asset_id.to_string(),
            background_available,
            effects,
            diagnostics,
        });
    }
    Ok(result)
}

fn scene_project_context(
    project_service: &ProjectService,
    project_id: &str,
) -> Result<SceneProjectContext, String> {
    let active = project_service
        .store()
        .active_project()?
        .ok_or_else(|| "GUI_SCENE_PROJECT_NOT_ACTIVE: 请先激活一个 996 项目".to_string())?;
    if active.id != project_id {
        return Err("GUI_SCENE_PROJECT_NOT_ACTIVE: 请求项目不是当前激活项目".to_string());
    }
    let project_root = fs::canonicalize(&active.root)
        .map_err(|e| format!("GUI_SCENE_PROJECT_PATH_INVALID: {e}"))?;
    let client_candidate = PathBuf::from(&active.client_root);
    reject_symlink(&client_candidate, "GUI_SCENE_CLIENT_SYMLINK")?;
    let client_root = fs::canonicalize(&client_candidate)
        .map_err(|e| format!("GUI_SCENE_CLIENT_PATH_INVALID: {e}"))?;
    if !client_root.is_dir() || !client_root.starts_with(&project_root) {
        return Err("GUI_SCENE_CLIENT_PATH_OUTSIDE: 客户端目录超出项目根".to_string());
    }
    let dev_res_root = optional_safe_root(&client_root, &client_root.join("dev/res"))?;
    let cache_root = optional_safe_root(&client_root, &client_root.join("cache/mod_chuanqi3"))?;
    Ok(SceneProjectContext {
        project_id: active.id,
        dev_res_root,
        cache_root,
    })
}

fn optional_safe_root(parent: &Path, candidate: &Path) -> Result<Option<PathBuf>, String> {
    if !candidate.exists() {
        return Ok(None);
    }
    reject_symlink(candidate, "GUI_SCENE_ROOT_SYMLINK")?;
    let canonical = fs::canonicalize(candidate)
        .map_err(|e| format!("GUI_SCENE_ROOT_INVALID: {}: {e}", candidate.display()))?;
    if !canonical.is_dir() || !canonical.starts_with(parent) {
        return Err("GUI_SCENE_ROOT_OUTSIDE: 场景素材根目录越界".to_string());
    }
    Ok(Some(canonical))
}

fn list_modules(context: &SceneProjectContext) -> Result<Vec<SceneAssetModule>, String> {
    let Some(cache_root) = &context.cache_root else {
        return Ok(Vec::new());
    };
    let mut modules = Vec::new();
    let entries = fs::read_dir(cache_root).map_err(|e| {
        format!(
            "GUI_SCENE_MODULE_LIST_FAILED: {}: {e}",
            cache_root.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("GUI_SCENE_MODULE_LIST_FAILED: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("GUI_SCENE_MODULE_METADATA_FAILED: {e}"))?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let Some(id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !id.starts_with("mod_") || !valid_module_id(&id) {
            continue;
        }
        let root = fs::canonicalize(entry.path())
            .map_err(|e| format!("GUI_SCENE_MODULE_PATH_INVALID: {id}: {e}"))?;
        if !root.starts_with(cache_root) {
            return Err("GUI_SCENE_MODULE_PATH_OUTSIDE: 模块缓存目录越界".to_string());
        }
        modules.push(SceneAssetModule {
            asset_root_id: format!("cache://module/{id}"),
            has_res: root.join("res").is_dir(),
            has_anim: root.join("anim").is_dir(),
            has_scene: root.join("scene").is_dir(),
            id,
        });
    }
    modules.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(modules)
}

fn resolve_asset(
    context: &SceneProjectContext,
    asset_id: &str,
    allowed_extensions: &[&str],
) -> Result<ResolvedAsset, String> {
    let parsed = parse_asset_id(asset_id)?;
    let extension = Path::new(&parsed.relative)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "GUI_SCENE_ASSET_TYPE_UNSUPPORTED: 素材缺少扩展名".to_string())?;
    if !allowed_extensions.contains(&extension.as_str()) {
        return Err(format!(
            "GUI_SCENE_ASSET_TYPE_UNSUPPORTED: 不支持 {extension} 素材"
        ));
    }
    let root = source_root(context, &parsed.source)?;
    let target = root.join(&parsed.relative);
    ensure_no_symlink_components(&root, &target)?;
    let canonical = fs::canonicalize(&target)
        .map_err(|e| format!("GUI_SCENE_ASSET_NOT_FOUND: {}: {e}", parsed.relative))?;
    if !canonical.is_file() || !canonical.starts_with(&root) {
        return Err("GUI_SCENE_ASSET_PATH_OUTSIDE: 场景素材路径越界".to_string());
    }
    Ok(ResolvedAsset {
        parsed,
        path: canonical,
    })
}

fn parse_asset_id(asset_id: &str) -> Result<ParsedAssetId, String> {
    if let Some(relative) = asset_id.strip_prefix("dev://res/") {
        let suffix = validate_relative_path(relative)?;
        return Ok(ParsedAssetId {
            source: SceneAssetSource::Dev,
            relative: suffix,
        });
    }
    if let Some(relative) = asset_id.strip_prefix("cache://stab/") {
        let suffix = validate_cache_relative(relative)?;
        return Ok(ParsedAssetId {
            source: SceneAssetSource::Stab,
            relative: suffix,
        });
    }
    if let Some(value) = asset_id.strip_prefix("cache://module/") {
        let (module_id, relative) = value
            .split_once('/')
            .ok_or_else(|| "GUI_SCENE_ASSET_ID_INVALID: module ID 缺少素材相对路径".to_string())?;
        if !module_id.starts_with("mod_") || !valid_module_id(module_id) {
            return Err("GUI_SCENE_ASSET_MODULE_INVALID: module ID 无效".to_string());
        }
        return Ok(ParsedAssetId {
            source: SceneAssetSource::Module(module_id.to_string()),
            relative: validate_cache_relative(relative)?,
        });
    }
    Err("GUI_SCENE_ASSET_ID_INVALID: 不支持的场景素材 ID".to_string())
}

fn source_root(
    context: &SceneProjectContext,
    source: &SceneAssetSource,
) -> Result<PathBuf, String> {
    match source {
        SceneAssetSource::Dev => context
            .dev_res_root
            .clone()
            .ok_or_else(|| "GUI_SCENE_DEV_RES_MISSING: 客户端/dev/res 不存在".to_string()),
        SceneAssetSource::Stab => {
            let cache = context
                .cache_root
                .as_ref()
                .ok_or_else(|| "GUI_SCENE_CACHE_MISSING: 客户端场景缓存不存在".to_string())?;
            safe_child_root(cache, "stab")
        }
        SceneAssetSource::Module(module_id) => {
            let cache = context
                .cache_root
                .as_ref()
                .ok_or_else(|| "GUI_SCENE_CACHE_MISSING: 客户端场景缓存不存在".to_string())?;
            safe_child_root(cache, module_id)
        }
    }
}

fn safe_child_root(parent: &Path, name: &str) -> Result<PathBuf, String> {
    let candidate = parent.join(name);
    reject_symlink(&candidate, "GUI_SCENE_SOURCE_SYMLINK")?;
    let canonical = fs::canonicalize(&candidate)
        .map_err(|e| format!("GUI_SCENE_SOURCE_MISSING: {name}: {e}"))?;
    if !canonical.is_dir() || !canonical.starts_with(parent) {
        return Err("GUI_SCENE_SOURCE_OUTSIDE: 场景素材来源越界".to_string());
    }
    Ok(canonical)
}

fn validate_relative_path(value: &str) -> Result<String, String> {
    if value.trim().is_empty() || value.contains('\0') || value.contains('\\') {
        return Err("GUI_SCENE_ASSET_PATH_INVALID: 素材相对路径无效".to_string());
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            !matches!(component, Component::Normal(_))
                || component
                    .as_os_str()
                    .to_str()
                    .is_none_or(|part| part.is_empty() || part == "." || part == "..")
        })
    {
        return Err("GUI_SCENE_ASSET_PATH_INVALID: 素材相对路径无效".to_string());
    }
    Ok(value.replace('\\', "/"))
}

fn validate_cache_relative(value: &str) -> Result<String, String> {
    let relative = validate_relative_path(value)?;
    let first = Path::new(&relative)
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str());
    if !matches!(first, Some("res" | "anim" | "scene")) {
        return Err("GUI_SCENE_ASSET_PATH_INVALID: cache 只允许 res、anim、scene".to_string());
    }
    Ok(relative)
}

fn valid_module_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'_' | b'-'))
}

fn ensure_no_symlink_components(root: &Path, target: &Path) -> Result<(), String> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| "GUI_SCENE_ASSET_PATH_OUTSIDE: 素材路径越界".to_string())?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err("GUI_SCENE_ASSET_PATH_INVALID: 素材路径包含非法段".to_string());
        };
        current.push(part);
        if current.exists() {
            reject_symlink(&current, "GUI_SCENE_ASSET_SYMLINK")?;
        }
    }
    Ok(())
}

fn reject_symlink(path: &Path, code: &str) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("{code}: {}: {e}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("{code}: 不允许符号链接或重解析点"));
    }
    Ok(())
}

fn read_limited_file(path: &Path, limit: u64, code: &str) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path)
        .map_err(|e| format!("{code}_METADATA_FAILED: {}: {e}", path.display()))?;
    if metadata.len() > limit {
        return Err(format!(
            "{code}_TOO_LARGE: 文件超过 {} MiB",
            limit / 1024 / 1024
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("{code}_READ_FAILED: {}: {e}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{code}_TOO_LARGE: 文件读取后超过限制"));
    }
    Ok(bytes)
}

fn decode_gzip_if_needed(source: &[u8]) -> Result<(Vec<u8>, bool), String> {
    if !source.starts_with(&[0x1f, 0x8b]) {
        return Ok((source.to_vec(), false));
    }
    let mut output = Vec::new();
    GzDecoder::new(source)
        .take(MAX_SCENE_ASSET_DECODED_BYTES as u64 + 1)
        .read_to_end(&mut output)
        .map_err(|e| format!("GUI_SCENE_ASSET_GZIP_INVALID: {e}"))?;
    if output.len() > MAX_SCENE_ASSET_DECODED_BYTES {
        return Err("GUI_SCENE_ASSET_DECODED_TOO_LARGE: 解压后素材超过 64 MiB".to_string());
    }
    Ok((output, true))
}

fn prepare_scene_image(path: &Path, bytes: Vec<u8>) -> Result<PreparedSceneImage, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match extension.as_str() {
        "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => Ok(PreparedSceneImage {
            bytes,
            mime_type: "image/png",
            texture_format: None,
            transcoded: false,
            diagnostic: None,
        }),
        "png" if bytes.starts_with(b"PKM 20") => prepare_pkm_image(bytes),
        "jpg" | "jpeg" if bytes.starts_with(&[0xff, 0xd8, 0xff]) => Ok(PreparedSceneImage {
            bytes,
            mime_type: "image/jpeg",
            texture_format: None,
            transcoded: false,
            diagnostic: None,
        }),
        "png" => Err("GUI_SCENE_ASSET_CONTAINER_INVALID: PNG 文件头无效".to_string()),
        "jpg" | "jpeg" => Err("GUI_SCENE_ASSET_CONTAINER_INVALID: JPEG 文件头无效".to_string()),
        _ => Err("GUI_SCENE_ASSET_TYPE_UNSUPPORTED: 仅支持 PNG/JPG".to_string()),
    }
}

fn prepare_pkm_image(bytes: Vec<u8>) -> Result<PreparedSceneImage, String> {
    let info = parse_pkm_info(&bytes)?;
    match transcode_pkm_to_png(&bytes, &info) {
        Ok(png) => Ok(PreparedSceneImage {
            bytes: png,
            mime_type: "image/png",
            texture_format: Some(info.format_name),
            transcoded: true,
            diagnostic: None,
        }),
        Err(error) => Ok(PreparedSceneImage {
            bytes,
            mime_type: "image/x-pkm",
            texture_format: Some(info.format_name),
            transcoded: false,
            diagnostic: Some(error),
        }),
    }
}

fn transcode_pkm_to_png(bytes: &[u8], info: &PkmInfo) -> Result<Vec<u8>, String> {
    let pixel_count = (info.width as usize)
        .checked_mul(info.height as usize)
        .ok_or_else(|| "GUI_SCENE_ASSET_PKM_SIZE_INVALID: 像素数量溢出".to_string())?;
    let rgba_bytes = pixel_count
        .checked_mul(4)
        .ok_or_else(|| "GUI_SCENE_ASSET_PKM_SIZE_INVALID: RGBA 大小溢出".to_string())?;
    if rgba_bytes > MAX_SCENE_ASSET_DECODED_BYTES {
        return Err("GUI_SCENE_ASSET_PKM_DECODE_TOO_LARGE: RGBA 超过 64 MiB".to_string());
    }
    let payload = bytes
        .get(16..)
        .ok_or_else(|| "GUI_SCENE_ASSET_PKM_INVALID: PKM 缺少纹理数据".to_string())?;
    let mut pixels = vec![0u32; pixel_count];
    let decode_result = match info.format {
        0 => decode_etc1(
            payload,
            info.width as usize,
            info.height as usize,
            &mut pixels,
        ),
        1 => decode_etc2_rgb(
            payload,
            info.width as usize,
            info.height as usize,
            &mut pixels,
        ),
        3 => decode_etc2_rgba8(
            payload,
            info.width as usize,
            info.height as usize,
            &mut pixels,
        ),
        4 => decode_etc2_rgba1(
            payload,
            info.width as usize,
            info.height as usize,
            &mut pixels,
        ),
        5 => decode_eacr(
            payload,
            info.width as usize,
            info.height as usize,
            &mut pixels,
        ),
        6 => decode_eacr_signed(
            payload,
            info.width as usize,
            info.height as usize,
            &mut pixels,
        ),
        7 => decode_eacrg(
            payload,
            info.width as usize,
            info.height as usize,
            &mut pixels,
        ),
        8 => decode_eacrg_signed(
            payload,
            info.width as usize,
            info.height as usize,
            &mut pixels,
        ),
        value => {
            return Err(format!(
                "GUI_SCENE_ASSET_PKM_DECODE_UNSUPPORTED: 不支持 PKM format {value}"
            ))
        }
    };
    decode_result.map_err(|e| format!("GUI_SCENE_ASSET_PKM_DECODE_FAILED: {e}"))?;
    let mut rgba = Vec::with_capacity(rgba_bytes);
    for pixel in pixels {
        let bgra = pixel.to_le_bytes();
        rgba.extend_from_slice(&[bgra[2], bgra[1], bgra[0], bgra[3]]);
    }
    let image = image::RgbaImage::from_raw(info.width, info.height, rgba)
        .ok_or_else(|| "GUI_SCENE_ASSET_PKM_IMAGE_INVALID: RGBA 缓冲区无效".to_string())?;
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|e| format!("GUI_SCENE_ASSET_PKM_PNG_FAILED: {e}"))?;
    let output = output.into_inner();
    if output.len() > MAX_SCENE_ASSET_DECODED_BYTES {
        return Err("GUI_SCENE_ASSET_PKM_PNG_TOO_LARGE: PNG 超过 64 MiB".to_string());
    }
    Ok(output)
}

fn image_dimensions(bytes: &[u8], mime_type: &str) -> Result<(u32, u32), String> {
    if mime_type == "image/png" && bytes.len() >= 24 {
        return Ok((
            u32::from_be_bytes(bytes[16..20].try_into().unwrap_or([0; 4])),
            u32::from_be_bytes(bytes[20..24].try_into().unwrap_or([0; 4])),
        ));
    }
    if mime_type == "image/jpeg" {
        return jpeg_dimensions(bytes);
    }
    if mime_type == "image/x-pkm" {
        let info = parse_pkm_info(bytes)?;
        return Ok((info.width, info.height));
    }
    Err("GUI_SCENE_ASSET_DIMENSIONS_UNSUPPORTED: 无法读取素材尺寸".to_string())
}

fn parse_pkm_info(bytes: &[u8]) -> Result<PkmInfo, String> {
    if bytes.len() < 16 || !bytes.starts_with(b"PKM 20") {
        return Err("GUI_SCENE_ASSET_PKM_INVALID: PKM 2.0 文件头无效".to_string());
    }
    let format = u16::from_be_bytes([bytes[6], bytes[7]]);
    let encoded_width = u16::from_be_bytes([bytes[8], bytes[9]]) as u32;
    let encoded_height = u16::from_be_bytes([bytes[10], bytes[11]]) as u32;
    let width = u16::from_be_bytes([bytes[12], bytes[13]]) as u32;
    let height = u16::from_be_bytes([bytes[14], bytes[15]]) as u32;
    if width == 0
        || height == 0
        || encoded_width < width
        || encoded_height < height
        || encoded_width % 4 != 0
        || encoded_height % 4 != 0
    {
        return Err("GUI_SCENE_ASSET_PKM_INVALID: PKM 纹理尺寸无效".to_string());
    }
    let format_name = match format {
        0 => "etc1-rgb",
        1 => "etc2-rgb",
        3 => "etc2-rgba",
        4 => "etc2-rgba1",
        5 => "eac-r11",
        6 => "eac-signed-r11",
        7 => "eac-rg11",
        8 => "eac-signed-rg11",
        _ => "unknown",
    };
    Ok(PkmInfo {
        format,
        format_name: if format_name == "unknown" {
            format!("pkm-format-{format}")
        } else {
            format_name.to_string()
        },
        width,
        height,
    })
}

fn jpeg_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    let mut offset = 2usize;
    while offset + 9 < bytes.len() {
        if bytes[offset] != 0xff {
            offset += 1;
            continue;
        }
        let marker = bytes[offset + 1];
        offset += 2;
        if matches!(marker, 0xd8 | 0xd9) || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        if offset + 2 > bytes.len() {
            break;
        }
        let length = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
        if length < 2 || offset + length > bytes.len() {
            break;
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) && length >= 7
        {
            let height = u16::from_be_bytes([bytes[offset + 3], bytes[offset + 4]]) as u32;
            let width = u16::from_be_bytes([bytes[offset + 5], bytes[offset + 6]]) as u32;
            return Ok((width, height));
        }
        offset += length;
    }
    Err("GUI_SCENE_ASSET_DIMENSIONS_INVALID: JPEG 缺少尺寸信息".to_string())
}

fn parse_plist(bytes: &[u8]) -> Result<PlistValue, String> {
    let source = std::str::from_utf8(bytes)
        .map_err(|e| format!("GUI_SCENE_ATLAS_ENCODING_INVALID: {e}"))?
        .trim_start_matches('\u{feff}');
    let tokens = tokenize_plist(source)?;
    let Some(start) = tokens
        .iter()
        .position(|token| matches!(token, PlistToken::DictStart))
    else {
        return Err("GUI_SCENE_ATLAS_ROOT_INVALID: plist 缺少根 dict".to_string());
    };
    let mut index = start;
    let value = parse_plist_value(&tokens, &mut index)?;
    Ok(value)
}

fn tokenize_plist(source: &str) -> Result<Vec<PlistToken>, String> {
    let mut tokens = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative_start) = source[cursor..].find('<') {
        let start = cursor + relative_start;
        let Some(relative_end) = source[start..].find('>') else {
            return Err("GUI_SCENE_ATLAS_XML_INVALID: 标签未闭合".to_string());
        };
        let end = start + relative_end;
        let tag = source[start + 1..end].trim();
        cursor = end + 1;
        match tag {
            "dict" => tokens.push(PlistToken::DictStart),
            "/dict" => tokens.push(PlistToken::DictEnd),
            "array" => tokens.push(PlistToken::ArrayStart),
            "/array" => tokens.push(PlistToken::ArrayEnd),
            "true/" | "true /" => tokens.push(PlistToken::Bool(true)),
            "false/" | "false /" => tokens.push(PlistToken::Bool(false)),
            "key" | "string" | "integer" | "real" => {
                let closing = format!("</{tag}>");
                let Some(relative_close) = source[cursor..].find(&closing) else {
                    return Err(format!("GUI_SCENE_ATLAS_XML_INVALID: {tag} 未闭合"));
                };
                let close = cursor + relative_close;
                let text = unescape_xml(&source[cursor..close]);
                cursor = close + closing.len();
                match tag {
                    "key" => tokens.push(PlistToken::Key(text)),
                    "string" => tokens.push(PlistToken::String(text)),
                    "integer" => tokens.push(PlistToken::Integer(
                        text.trim()
                            .parse()
                            .map_err(|e| format!("GUI_SCENE_ATLAS_INTEGER_INVALID: {e}"))?,
                    )),
                    "real" => {
                        text.trim()
                            .parse::<f64>()
                            .map_err(|e| format!("GUI_SCENE_ATLAS_REAL_INVALID: {e}"))?;
                        tokens.push(PlistToken::Real);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(tokens)
}

fn parse_plist_value(tokens: &[PlistToken], index: &mut usize) -> Result<PlistValue, String> {
    let token = tokens
        .get(*index)
        .ok_or_else(|| "GUI_SCENE_ATLAS_VALUE_MISSING: plist 值缺失".to_string())?;
    *index += 1;
    match token {
        PlistToken::DictStart => {
            let mut values = BTreeMap::new();
            loop {
                match tokens.get(*index) {
                    Some(PlistToken::DictEnd) => {
                        *index += 1;
                        break;
                    }
                    Some(PlistToken::Key(key)) => {
                        let key = key.clone();
                        *index += 1;
                        let value = parse_plist_value(tokens, index)?;
                        values.insert(key, value);
                    }
                    _ => return Err("GUI_SCENE_ATLAS_DICT_INVALID: dict 中缺少 key".to_string()),
                }
            }
            Ok(PlistValue::Dict(values))
        }
        PlistToken::ArrayStart => {
            loop {
                if matches!(tokens.get(*index), Some(PlistToken::ArrayEnd)) {
                    *index += 1;
                    break;
                }
                parse_plist_value(tokens, index)?;
            }
            Ok(PlistValue::Array)
        }
        PlistToken::String(value) => Ok(PlistValue::String(value.clone())),
        PlistToken::Integer(value) => Ok(PlistValue::Integer(*value)),
        PlistToken::Real => Ok(PlistValue::Real),
        PlistToken::Bool(value) => Ok(PlistValue::Bool(*value)),
        _ => Err("GUI_SCENE_ATLAS_VALUE_INVALID: plist 值类型无效".to_string()),
    }
}

fn plist_dict<'a>(
    value: &'a PlistValue,
    code: &str,
) -> Result<&'a BTreeMap<String, PlistValue>, String> {
    match value {
        PlistValue::Dict(values) => Ok(values),
        _ => Err(format!("{code}: 预期 dict")),
    }
}

fn plist_dict_optional(value: &PlistValue) -> Option<&BTreeMap<String, PlistValue>> {
    match value {
        PlistValue::Dict(values) => Some(values),
        _ => None,
    }
}

fn plist_string_optional(value: &PlistValue) -> Option<&str> {
    match value {
        PlistValue::String(value) => Some(value),
        _ => None,
    }
}

fn plist_bool_optional(value: &PlistValue) -> Option<bool> {
    match value {
        PlistValue::Bool(value) => Some(*value),
        PlistValue::Integer(value) => Some(*value != 0),
        PlistValue::String(value) if value.eq_ignore_ascii_case("true") => Some(true),
        PlistValue::String(value) if value.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    }
}

fn plist_field_numbers(
    values: &BTreeMap<String, PlistValue>,
    key: &str,
    minimum: usize,
) -> Result<Vec<i32>, String> {
    let numbers = values
        .get(key)
        .and_then(plist_string_optional)
        .map(parse_numbers)
        .unwrap_or_default();
    if numbers.len() < minimum {
        return Err(format!("GUI_SCENE_ATLAS_FRAME_INVALID: {key} 数值不足"));
    }
    Ok(numbers)
}

fn plist_field_numbers_default(
    values: &BTreeMap<String, PlistValue>,
    key: &str,
    default: &[i32],
) -> Vec<i32> {
    values
        .get(key)
        .and_then(plist_string_optional)
        .map(parse_numbers)
        .filter(|values| values.len() >= default.len())
        .unwrap_or_else(|| default.to_vec())
}

fn parse_numbers(value: &str) -> Vec<i32> {
    value
        .split(|character: char| !(character.is_ascii_digit() || character == '-'))
        .filter(|part| !part.is_empty() && *part != "-")
        .filter_map(|part| part.parse::<i32>().ok())
        .collect()
}

fn unescape_xml(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn sibling_asset_id(parsed: &ParsedAssetId, file_name: &str) -> Result<String, String> {
    let safe_name = validate_relative_path(file_name)?;
    if Path::new(&safe_name).components().count() != 1 {
        return Err("GUI_SCENE_ATLAS_TEXTURE_INVALID: 图集纹理必须是同目录文件".to_string());
    }
    let parent = Path::new(&parsed.relative)
        .parent()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let relative = if parent.is_empty() {
        safe_name
    } else {
        format!("{parent}/{safe_name}")
    };
    asset_id_for_source(&parsed.source, &relative)
}

fn asset_id_for_source(source: &SceneAssetSource, relative: &str) -> Result<String, String> {
    match source {
        SceneAssetSource::Dev => Ok(format!("dev://res/{relative}")),
        SceneAssetSource::Stab => Ok(format!("cache://stab/{relative}")),
        SceneAssetSource::Module(id) if valid_module_id(id) => {
            Ok(format!("cache://module/{id}/{relative}"))
        }
        SceneAssetSource::Module(_) => {
            Err("GUI_SCENE_ASSET_MODULE_INVALID: module ID 无效".to_string())
        }
    }
}

fn effect_search_modules(
    context: &SceneProjectContext,
    preferred_module: Option<&str>,
) -> Result<Vec<(SceneAssetSource, PathBuf, String)>, String> {
    let mut result = Vec::new();
    if let Some(module_id) = preferred_module {
        if !module_id.starts_with("mod_") || !valid_module_id(module_id) {
            return Err("GUI_SCENE_EFFECT_MODULE_INVALID: 首选 module ID 无效".to_string());
        }
        let source = SceneAssetSource::Module(module_id.to_string());
        let root = source_root(context, &source)?;
        result.push((source, root, format!("module:{module_id}")));
    }
    for module in list_modules(context)? {
        if preferred_module == Some(module.id.as_str()) {
            continue;
        }
        let source = SceneAssetSource::Module(module.id.clone());
        let root = source_root(context, &source)?;
        result.push((source, root, format!("module:{}", module.id)));
    }
    if let Ok(root) = source_root(context, &SceneAssetSource::Stab) {
        result.push((SceneAssetSource::Stab, root, "stab".to_string()));
    }
    Ok(result)
}

fn effect_asset_id(source: &SceneAssetSource, name: &str) -> Result<String, String> {
    asset_id_for_source(source, &format!("anim/effect/{name}"))
}

fn load_scene_atlas_config(context: &SceneProjectContext) -> Result<SceneAtlasConfig, String> {
    let stab_root = source_root(context, &SceneAssetSource::Stab)?;
    let format_path = stab_root.join("data_config/sceneAtlasFormatConfigs_996.txt");
    let split_path = stab_root.join("data_config/sceneAtlasSplitConfigs_996.txt");
    ensure_no_symlink_components(&stab_root, &format_path)?;
    ensure_no_symlink_components(&stab_root, &split_path)?;
    let format_bytes = read_limited_file(&format_path, 1024 * 1024, "GUI_SCENE_ATLAS_CONFIG")?;
    let split_bytes = read_limited_file(&split_path, 1024 * 1024, "GUI_SCENE_ATLAS_CONFIG")?;
    let raw_formats: BTreeMap<String, String> = serde_json::from_slice(&format_bytes)
        .map_err(|e| format!("GUI_SCENE_ATLAS_FORMAT_CONFIG_INVALID: {e}"))?;
    let raw_splits: BTreeMap<String, u32> = serde_json::from_slice(&split_bytes)
        .map_err(|e| format!("GUI_SCENE_ATLAS_SPLIT_CONFIG_INVALID: {e}"))?;
    let mut formats = BTreeMap::new();
    for (key, value) in raw_formats {
        let file_index = key
            .parse::<u16>()
            .map_err(|e| format!("GUI_SCENE_ATLAS_FORMAT_INDEX_INVALID: {key}: {e}"))?;
        if !value.starts_with("scene/") || value.matches("%d").count() != 1 {
            return Err(format!(
                "GUI_SCENE_ATLAS_FORMAT_PATH_INVALID: fileIndex {file_index}"
            ));
        }
        formats.insert(file_index, value);
    }
    let mut splits = BTreeMap::new();
    for (key, value) in raw_splits {
        let file_index = key
            .parse::<u16>()
            .map_err(|e| format!("GUI_SCENE_ATLAS_SPLIT_INDEX_INVALID: {key}: {e}"))?;
        if value == 0 {
            return Err(format!(
                "GUI_SCENE_ATLAS_SPLIT_VALUE_INVALID: fileIndex {file_index}"
            ));
        }
        splits.insert(file_index, value);
    }
    Ok(SceneAtlasConfig { formats, splits })
}

fn resolve_world_frame(
    context: &SceneProjectContext,
    config: &SceneAtlasConfig,
    preferred_module: Option<&str>,
    layer: SceneWorldLayer,
    x: u32,
    y: u32,
    file_index: u8,
    image_index: u16,
) -> SceneWorldFrameReference {
    let Some(format) = config.formats.get(&(file_index as u16)) else {
        return missing_world_frame(
            layer,
            x,
            y,
            file_index,
            image_index,
            None,
            format!("GUI_SCENE_ATLAS_FORMAT_MISSING: fileIndex {file_index}"),
        );
    };
    let Some(split) = config.splits.get(&(file_index as u16)).copied() else {
        return missing_world_frame(
            layer,
            x,
            y,
            file_index,
            image_index,
            None,
            format!("GUI_SCENE_ATLAS_SPLIT_MISSING: fileIndex {file_index}"),
        );
    };
    let atlas_index = image_index as u32 / split;
    let relative = format!("{}.plist", format.replace("%d", &atlas_index.to_string()));
    if let Err(error) = validate_cache_relative(&relative) {
        return missing_world_frame(
            layer,
            x,
            y,
            file_index,
            image_index,
            Some(atlas_index),
            error,
        );
    }
    match resolve_atlas_asset_id(context, preferred_module, &relative) {
        Ok(asset_id) => SceneWorldFrameReference {
            layer,
            x,
            y,
            file_index,
            image_index,
            frame_index: image_index,
            atlas_index: Some(atlas_index),
            atlas_asset_id: Some(asset_id),
            available: true,
            diagnostic: None,
        },
        Err(error) => missing_world_frame(
            layer,
            x,
            y,
            file_index,
            image_index,
            Some(atlas_index),
            error,
        ),
    }
}

fn missing_world_frame(
    layer: SceneWorldLayer,
    x: u32,
    y: u32,
    file_index: u8,
    image_index: u16,
    atlas_index: Option<u32>,
    diagnostic: String,
) -> SceneWorldFrameReference {
    SceneWorldFrameReference {
        layer,
        x,
        y,
        file_index,
        image_index,
        frame_index: image_index,
        atlas_index,
        atlas_asset_id: None,
        available: false,
        diagnostic: Some(diagnostic),
    }
}

fn resolve_atlas_asset_id(
    context: &SceneProjectContext,
    preferred_module: Option<&str>,
    relative: &str,
) -> Result<String, String> {
    let mut candidates = Vec::new();
    if let Some(module_id) = preferred_module {
        candidates.push(format!("cache://module/{module_id}/{relative}"));
    }
    for module in list_modules(context)? {
        if preferred_module == Some(module.id.as_str()) {
            continue;
        }
        candidates.push(format!("cache://module/{}/{relative}", module.id));
    }
    candidates.push(format!("cache://stab/{relative}"));
    for asset_id in &candidates {
        if resolve_asset(context, asset_id, &["plist"]).is_ok() {
            return Ok(asset_id.clone());
        }
    }
    Err(format!(
        "GUI_SCENE_ATLAS_ASSET_MISSING: 本地缓存缺少 {relative}"
    ))
}

fn module_id_from_asset_id(asset_id: &str) -> Option<String> {
    asset_id
        .strip_prefix("cache://module/")
        .and_then(|value| value.split_once('/'))
        .map(|(module_id, _)| module_id.to_string())
}

fn natural_name_key(value: &str) -> (u32, String) {
    let sequence = value
        .trim_end_matches(".plist")
        .rsplit('_')
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(u32::MAX);
    (sequence, value.to_string())
}

fn map_capability(context: &SceneProjectContext, map_id: &str) -> SceneMapCapability {
    match resolve_map(context, map_id).and_then(|(asset_id, path)| {
        let bytes = read_limited_file(&path, MAX_MAP_SOURCE_BYTES, "GUI_SCENE_MAP")?;
        let parsed = parse_map(&bytes)?;
        Ok((asset_id, parsed.width, parsed.height, parsed.format))
    }) {
        Ok((asset_id, width, height, format)) => SceneMapCapability {
            map_id: map_id.to_string(),
            available: true,
            asset_id: Some(asset_id),
            width: Some(width),
            height: Some(height),
            format: Some(format),
            diagnostic: None,
        },
        Err(error) => SceneMapCapability {
            map_id: map_id.to_string(),
            available: false,
            asset_id: None,
            width: None,
            height: None,
            format: None,
            diagnostic: Some(error),
        },
    }
}

fn resolve_map(context: &SceneProjectContext, map_id: &str) -> Result<(String, PathBuf), String> {
    validate_map_id(map_id)?;
    for module in list_modules(context)? {
        if !module.has_scene {
            continue;
        }
        let asset_id = format!("cache://module/{}/scene/map/{map_id}.map", module.id);
        if let Ok(resolved) = resolve_asset(context, &asset_id, &["map"]) {
            return Ok((asset_id, resolved.path));
        }
    }
    let stab_asset_id = format!("cache://stab/scene/map/{map_id}.map");
    if let Ok(resolved) = resolve_asset(context, &stab_asset_id, &["map"]) {
        return Ok((stab_asset_id, resolved.path));
    }
    Err(format!(
        "GUI_SCENE_MAP_MISSING: 本地缓存中没有地图 {map_id}"
    ))
}

fn validate_map_id(map_id: &str) -> Result<(), String> {
    if !SUPPORTED_MAP_IDS.contains(&map_id) {
        return Err("GUI_SCENE_MAP_UNSUPPORTED: 当前只支持 01、1、d021、d032".to_string());
    }
    Ok(())
}

fn parse_map(bytes: &[u8]) -> Result<ParsedMap<'_>, String> {
    if bytes.len() < MAP_HEADER_BYTES {
        return Err("GUI_SCENE_MAP_HEADER_INVALID: 地图头不足 28 字节".to_string());
    }
    let width = u16::from_le_bytes([bytes[22], bytes[23]]) as u32;
    let height = u16::from_le_bytes([bytes[24], bytes[25]]) as u32;
    if width == 0 || height == 0 || width % 2 != 0 || height % 2 != 0 {
        return Err("GUI_SCENE_MAP_DIMENSIONS_INVALID: 地图尺寸无效".to_string());
    }
    let tile_count = (width as usize / 2)
        .checked_mul(height as usize / 2)
        .ok_or_else(|| "GUI_SCENE_MAP_SIZE_INVALID: 地图 Tile 数量溢出".to_string())?;
    let tile_bytes = tile_count
        .checked_mul(MAP_TILE_RECORD_BYTES)
        .ok_or_else(|| "GUI_SCENE_MAP_SIZE_INVALID: 地图 Tile 数据溢出".to_string())?;
    let cell_count = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| "GUI_SCENE_MAP_SIZE_INVALID: 地图 Cell 数量溢出".to_string())?;
    let cell_bytes = cell_count
        .checked_mul(MAP_CELL_RECORD_BYTES)
        .ok_or_else(|| "GUI_SCENE_MAP_SIZE_INVALID: 地图 Cell 数据溢出".to_string())?;
    let cell_offset = MAP_HEADER_BYTES
        .checked_add(tile_bytes)
        .ok_or_else(|| "GUI_SCENE_MAP_SIZE_INVALID: 地图数据偏移溢出".to_string())?;
    let expected = cell_offset
        .checked_add(cell_bytes)
        .ok_or_else(|| "GUI_SCENE_MAP_SIZE_INVALID: 地图文件大小溢出".to_string())?;
    if bytes.len() != expected {
        return Err(format!(
            "GUI_SCENE_MAP_LAYOUT_UNSUPPORTED: 预期 {expected} 字节，实际 {} 字节",
            bytes.len()
        ));
    }
    let format = if bytes.starts_with(b"Aragom")
        && bytes[..MAP_HEADER_BYTES].windows(3).any(|v| v == b"3.1")
    {
        "aragom-3.1".to_string()
    } else {
        "996-map-3.1-compatible".to_string()
    };
    Ok(ParsedMap {
        bytes,
        width,
        height,
        tile_offset: MAP_HEADER_BYTES,
        cell_offset,
        format,
    })
}

fn validate_world_chunk_request(request: SceneWorldChunkRequest) -> Result<(), String> {
    if request.width == 0 || request.height == 0 {
        return Err("GUI_SCENE_MAP_CHUNK_INVALID: 区域尺寸必须大于 0".to_string());
    }
    if request.width > MAX_WORLD_CHUNK_EDGE || request.height > MAX_WORLD_CHUNK_EDGE {
        return Err("GUI_SCENE_MAP_CHUNK_TOO_LARGE: 单次区域不能超过 128×128".to_string());
    }
    Ok(())
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestProject {
        service: ProjectService,
        id: String,
        root: PathBuf,
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn test_project() -> TestProject {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("mir3-scene-assets-{nonce}"));
        fs::create_dir_all(root.join("project/客户端/dev/res/login")).unwrap();
        fs::create_dir_all(root.join("project/客户端/cache/mod_chuanqi3/mod_demo/anim/effect"))
            .unwrap();
        fs::create_dir_all(root.join("project/客户端/cache/mod_chuanqi3/mod_demo/scene/map"))
            .unwrap();
        fs::create_dir_all(root.join("project/客户端/cache/mod_chuanqi3/stab/res")).unwrap();
        fs::create_dir_all(root.join("project/客户端/cache/mod_chuanqi3/stab/data_config"))
            .unwrap();
        fs::create_dir_all(root.join("project/引擎")).unwrap();
        let service = ProjectService::new(root.join("data")).unwrap();
        let project = service
            .store()
            .import_project(&root.join("project"))
            .unwrap();
        TestProject {
            service,
            id: project.id,
            root,
        }
    }

    fn tiny_png() -> Vec<u8> {
        vec![
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0,
            0, 0, 2, 0, 0, 0, 3,
        ]
    }

    fn tiny_pkm() -> Vec<u8> {
        let mut bytes = b"PKM 20".to_vec();
        bytes.extend_from_slice(&3u16.to_be_bytes());
        bytes.extend_from_slice(&4u16.to_be_bytes());
        bytes.extend_from_slice(&4u16.to_be_bytes());
        bytes.extend_from_slice(&4u16.to_be_bytes());
        bytes.extend_from_slice(&4u16.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 16]);
        bytes
    }

    fn tiny_unknown_pkm() -> Vec<u8> {
        let mut bytes = tiny_pkm();
        bytes[6..8].copy_from_slice(&9u16.to_be_bytes());
        bytes
    }

    fn tiny_jpeg() -> Vec<u8> {
        vec![
            0xff, 0xd8, 0xff, 0xc0, 0x00, 0x07, 0x08, 0x00, 0x03, 0x00, 0x02, 0xff, 0xd9,
        ]
    }

    fn write_atlas_config(project: &TestProject) {
        let root = project
            .root
            .join("project/客户端/cache/mod_chuanqi3/stab/data_config");
        fs::write(
            root.join("sceneAtlasFormatConfigs_996.txt"),
            r#"{"1":"scene/Tiles30c/%d"}"#,
        )
        .unwrap();
        fs::write(root.join("sceneAtlasSplitConfigs_996.txt"), r#"{"1":500}"#).unwrap();
    }

    fn atlas_source() -> &'static str {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>frames</key><dict>
<key>frame_0.png</key><dict>
<key>frame</key><string>{{1,2},{30,40}}</string>
<key>offset</key><string>{-3,4}</string>
<key>rotated</key><true/>
<key>sourceSize</key><string>{50,60}</string>
</dict></dict>
<key>metadata</key><dict>
<key>realTextureFileName</key><string>sfx_3061_0.png</string>
<key>size</key><string>{256,512}</string>
</dict></dict></plist>"#
    }

    fn synthetic_map(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; MAP_HEADER_BYTES];
        bytes[..6].copy_from_slice(b"Aragom");
        bytes[16..19].copy_from_slice(b"3.1");
        bytes[22..24].copy_from_slice(&width.to_le_bytes());
        bytes[24..26].copy_from_slice(&height.to_le_bytes());
        for index in 0..(width as usize / 2) * (height as usize / 2) {
            bytes.push((index + 1) as u8);
            bytes.extend_from_slice(&((index + 100) as u16).to_le_bytes());
        }
        for index in 0..width as usize * height as usize {
            let mut record = [0u8; MAP_CELL_RECORD_BYTES];
            record[0] = index as u8;
            record[1] = 0xaa;
            record[5..9].fill(0xff);
            bytes.extend_from_slice(&record);
        }
        bytes
    }

    #[test]
    fn catalog_exposes_only_mod_prefixed_directories() {
        let project = test_project();
        fs::create_dir_all(
            project
                .root
                .join("project/客户端/cache/mod_chuanqi3/private_data"),
        )
        .unwrap();
        let catalog = scene_asset_catalog(&project.service, &project.id).unwrap();
        assert!(catalog.dev_available);
        assert!(catalog.stab_available);
        assert_eq!(catalog.modules.len(), 1);
        assert_eq!(catalog.modules[0].id, "mod_demo");
    }

    #[test]
    fn gzip_wrapped_png_is_decoded_and_measured() {
        let project = test_project();
        let source = tiny_png();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&source).unwrap();
        let compressed = encoder.finish().unwrap();
        fs::write(
            project
                .root
                .join("project/客户端/cache/mod_chuanqi3/mod_demo/anim/effect/sfx_3061_0.png"),
            compressed,
        )
        .unwrap();
        let asset_id = "cache://module/mod_demo/anim/effect/sfx_3061_0.png";
        let content = read_scene_asset(&project.service, &project.id, asset_id).unwrap();
        assert!(content.gzip_wrapped);
        assert_eq!(content.bytes, source);
        let meta = scene_asset_meta(&project.service, &project.id, asset_id).unwrap();
        assert_eq!((meta.width, meta.height), (2, 3));
    }

    #[test]
    fn gzip_wrapped_pkm_texture_is_transcoded_to_png() {
        let project = test_project();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&tiny_pkm()).unwrap();
        let compressed = encoder.finish().unwrap();
        fs::write(
            project
                .root
                .join("project/客户端/cache/mod_chuanqi3/mod_demo/anim/effect/sfx_3005_0.png"),
            compressed,
        )
        .unwrap();
        let meta = scene_asset_meta(
            &project.service,
            &project.id,
            "cache://module/mod_demo/anim/effect/sfx_3005_0.png",
        )
        .unwrap();
        assert_eq!(meta.mime_type, "image/png");
        assert_eq!(meta.texture_format.as_deref(), Some("etc2-rgba"));
        assert_eq!((meta.width, meta.height), (4, 4));
        assert!(meta.browser_renderable);
        assert!(meta.transcoded);
        assert!(meta.diagnostic.is_none());
    }

    #[test]
    fn unsupported_pkm_format_is_preserved_with_diagnostic() {
        let project = test_project();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&tiny_unknown_pkm()).unwrap();
        let compressed = encoder.finish().unwrap();
        fs::write(
            project
                .root
                .join("project/客户端/cache/mod_chuanqi3/mod_demo/anim/effect/sfx_3999_0.png"),
            compressed,
        )
        .unwrap();
        let meta = scene_asset_meta(
            &project.service,
            &project.id,
            "cache://module/mod_demo/anim/effect/sfx_3999_0.png",
        )
        .unwrap();
        assert_eq!(meta.mime_type, "image/x-pkm");
        assert!(!meta.browser_renderable);
        assert!(!meta.transcoded);
        assert!(meta.diagnostic.is_some());
    }

    #[test]
    fn atlas_manifest_keeps_frame_geometry_and_opaque_texture_id() {
        let project = test_project();
        fs::write(
            project
                .root
                .join("project/客户端/cache/mod_chuanqi3/mod_demo/anim/effect/sfx_3061_0.plist"),
            atlas_source(),
        )
        .unwrap();
        let manifest = scene_atlas_manifest(
            &project.service,
            &project.id,
            "cache://module/mod_demo/anim/effect/sfx_3061_0.plist",
        )
        .unwrap();
        assert_eq!(manifest.frames.len(), 1);
        assert_eq!(manifest.frames[0].width, 30);
        assert_eq!(manifest.frames[0].offset_x, -3);
        assert!(manifest.frames[0].rotated);
        assert_eq!(manifest.texture_width, Some(256));
        assert_eq!(
            manifest.texture_asset_id,
            "cache://module/mod_demo/anim/effect/sfx_3061_0.png"
        );
    }

    #[test]
    fn effect_resolution_prefers_canonical_atlas_names() {
        let project = test_project();
        let directory = project
            .root
            .join("project/客户端/cache/mod_chuanqi3/mod_demo/anim/effect");
        fs::write(directory.join("sfx_3061_0.plist"), atlas_source()).unwrap();
        fs::write(directory.join("sfx_3061_0-1.plist"), atlas_source()).unwrap();
        let resolved = resolve_scene_effect(&project.service, &project.id, 3061, None).unwrap();
        assert!(resolved.available);
        assert_eq!(resolved.atlas_asset_ids.len(), 1);
        assert!(resolved.atlas_asset_ids[0].ends_with("sfx_3061_0.plist"));
    }

    #[test]
    fn world_chunk_decodes_minimal_map_records() {
        let project = test_project();
        fs::write(
            project
                .root
                .join("project/客户端/cache/mod_chuanqi3/mod_demo/scene/map/1.map"),
            synthetic_map(4, 4),
        )
        .unwrap();
        let chunk = read_world_chunk(
            &project.service,
            &project.id,
            "1",
            SceneWorldChunkRequest {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
        )
        .unwrap();
        assert_eq!((chunk.map_width, chunk.map_height), (4, 4));
        assert_eq!(chunk.cells.len(), 4);
        assert_eq!(chunk.cells[0].flags, 5);
        assert_eq!(chunk.tiles.len(), 4);
        assert_eq!(chunk.tiles[0].image_index, 100);
    }

    #[test]
    fn world_manifest_resolves_ground_atlas_from_fixed_configs() {
        let project = test_project();
        write_atlas_config(&project);
        let map_root = project
            .root
            .join("project/客户端/cache/mod_chuanqi3/mod_demo/scene");
        fs::write(map_root.join("map/1.map"), synthetic_map(4, 4)).unwrap();
        fs::create_dir_all(map_root.join("Tiles30c")).unwrap();
        fs::write(map_root.join("Tiles30c/0.plist"), atlas_source()).unwrap();
        let manifest = scene_world_manifest(
            &project.service,
            &project.id,
            "1",
            SceneWorldChunkRequest {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
        )
        .unwrap();
        assert_eq!(manifest.status, SceneWorldManifestStatus::Supported);
        assert_eq!(manifest.frames.len(), 1);
        assert_eq!(manifest.frames[0].layer, SceneWorldLayer::Ground);
        assert_eq!(manifest.frames[0].frame_index, 100);
        assert_eq!(
            manifest.frames[0].atlas_asset_id.as_deref(),
            Some("cache://module/mod_demo/scene/Tiles30c/0.plist")
        );
    }

    #[test]
    fn login_presets_bind_fixed_backgrounds_and_effects() {
        let project = test_project();
        let login = project.root.join("project/客户端/dev/res/private/login");
        fs::create_dir_all(&login).unwrap();
        fs::write(login.join("create_bg.jpg"), tiny_jpeg()).unwrap();
        fs::write(login.join("bg_cjzy_02.jpg"), tiny_jpeg()).unwrap();
        let effects = project
            .root
            .join("project/客户端/cache/mod_chuanqi3/mod_demo/anim/effect");
        for effect_id in [3005u32, 3006, 3061, 3062] {
            fs::write(
                effects.join(format!("sfx_{effect_id}_0.plist")),
                atlas_source(),
            )
            .unwrap();
        }
        let presets = scene_login_presets(&project.service, &project.id).unwrap();
        assert_eq!(presets.len(), 2);
        assert!(presets.iter().all(|preset| preset.background_available));
        assert_eq!(presets[0].effects[0].effect_id, 3061);
        assert_eq!(presets[0].effects[1].effect_id, 3062);
        assert!(presets
            .iter()
            .flat_map(|preset| &preset.effects)
            .all(|effect| effect.available));
    }

    #[test]
    fn opaque_ids_reject_parent_and_non_scene_cache_roots() {
        assert!(parse_asset_id("cache://module/mod_demo/../secret.png").is_err());
        assert!(parse_asset_id("cache://module/mod_demo/scripts/secret.lua").is_err());
        assert!(parse_asset_id("/tmp/image.png").is_err());
        assert!(parse_asset_id("cache://module/private/anim/a.png").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_asset_is_rejected() {
        use std::os::unix::fs::symlink;

        let project = test_project();
        let outside = project.root.join("outside.png");
        fs::write(&outside, tiny_png()).unwrap();
        let link = project.root.join("project/客户端/dev/res/login/link.png");
        symlink(outside, link).unwrap();
        let result = read_scene_asset(&project.service, &project.id, "dev://res/login/link.png");
        assert!(result.is_err());
    }

    #[test]
    fn configured_real_project_scene_assets_are_compatible() {
        let Ok(project_root) = std::env::var("MIR3_SCENE_TEST_PROJECT_ROOT") else {
            return;
        };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_root = std::env::temp_dir().join(format!("mir3-scene-real-{nonce}"));
        let service = ProjectService::new(data_root.clone()).unwrap();
        let project = service
            .store()
            .import_project(Path::new(&project_root))
            .unwrap();
        let catalog = scene_asset_catalog(&service, &project.id).unwrap();
        assert!(catalog.modules.iter().any(|module| module.has_scene));
        let presets = scene_login_presets(&service, &project.id).unwrap();
        assert!(presets.iter().all(|preset| preset.background_available));
        let effect = resolve_scene_effect(&service, &project.id, 3061, None).unwrap();
        assert!(effect.available);
        let atlas =
            scene_atlas_manifest(&service, &project.id, &effect.atlas_asset_ids[0]).unwrap();
        assert!(!atlas.frames.is_empty());
        let texture = scene_asset_meta(&service, &project.id, &atlas.texture_asset_id).unwrap();
        assert!(texture.width > 0 && texture.height > 0);
        assert_eq!(texture.mime_type, "image/png");
        assert!(texture.browser_renderable);
        assert!(texture.transcoded);
        let capabilities = scene_map_capabilities(&service, &project.id).unwrap();
        assert!(capabilities.map_ids.iter().all(|map| map.available));
        let manifest = scene_world_manifest(
            &service,
            &project.id,
            "1",
            SceneWorldChunkRequest {
                x: 340,
                y: 140,
                width: 32,
                height: 32,
            },
        )
        .unwrap();
        assert_ne!(manifest.status, SceneWorldManifestStatus::Unsupported);
        let _ = fs::remove_dir_all(data_root);
    }
}
