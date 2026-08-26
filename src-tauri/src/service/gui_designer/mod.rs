//! GUI Designer 的项目边界、文件读取与 Draft 编排。
//!
//! 本模块只允许访问当前激活项目的 `客户端/dev`，不会使用 Workspace，且不会
//! 执行 Lua。所有正式写入都必须经过现有 Draft、人工确认与 Snapshot 链路。

use crate::service::project::{DraftConfirmation, ProjectService};
use mir3_domain::{patch_supported_text_bytes, DraftBinaryChangeInput, DraftPreview, Snapshot};
use mir3_ui::{
    generate_template, parse_document, DiagnosticSeverity, Mir3UiDocument, Mir3UiViewport,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const MAX_ASSET_BYTES: u64 = 16 * 1024 * 1024;
const MAX_GUI_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const MOBILE_WIDTH: u32 = 1136;
const MOBILE_HEIGHT: u32 = 640;
const DEFAULT_PC_WIDTH: u32 = 1024;
const DEFAULT_PC_HEIGHT: u32 = 768;
const DEV_TREE_PAGE_SIZE: usize = 500;
const DEV_METADATA_DOCUMENT_VERSION: &str = "2026-08-24";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiDesignerStatus {
    pub project_id: String,
    pub dev_root: String,
    pub available: bool,
    pub gui_export_available: bool,
    pub resource_available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GuiDocumentKind {
    Editable,
    Readonly,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GuiPlatform {
    Mobile,
    Pc,
    Shared,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiDocumentEntry {
    pub path: String,
    pub kind: GuiDocumentKind,
    pub platform: GuiPlatform,
    pub peer_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiDocumentList {
    pub project_id: String,
    pub entries: Vec<GuiDocumentEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiDocumentEnvelope {
    pub dev_relative_path: String,
    pub source: String,
    pub document: Mir3UiDocument,
    pub sha256: Option<String>,
    pub encoding: String,
    pub newline: String,
    pub draft_id: Option<String>,
    pub revision: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiReparseRequest {
    pub dev_relative_path: String,
    pub working_source: String,
    pub expected_sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GuiTemplateTarget {
    Mobile,
    Pc,
    Both,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiTemplateRequest {
    #[serde(alias = "relativePath")]
    pub path: String,
    #[serde(alias = "targets")]
    pub platform: GuiTemplateTarget,
    pub pc_resolution: Option<Mir3UiViewport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiTemplateResponse {
    pub documents: Vec<GuiDocumentEnvelope>,
}

#[derive(Debug, Clone)]
pub struct GuiAssetContent {
    pub logical_path: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiAssetMeta {
    pub logical_path: String,
    pub mime_type: String,
    pub byte_length: u64,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GuiDevEntryType {
    Directory,
    File,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GuiDevPolicy {
    Editable,
    Readonly,
    Asset,
    Info,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiDevTreeEntry {
    pub path: String,
    pub name: String,
    pub entry_type: GuiDevEntryType,
    pub policy: GuiDevPolicy,
    pub hidden: bool,
    pub size: u64,
    pub has_children: bool,
    pub description_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiDevTreePage {
    pub parent_path: String,
    pub entries: Vec<GuiDevTreeEntry>,
    pub next_cursor: Option<String>,
    pub metadata_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiReadonlyDocument {
    pub dev_relative_path: String,
    pub source: String,
    pub sha256: String,
    pub encoding: String,
    pub newline: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiDraftChangeSet {
    pub files: Vec<GuiDraftFileChange>,
    pub draft_id: Option<String>,
    #[serde(default)]
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiDraftFileChange {
    pub dev_relative_path: String,
    #[serde(alias = "workingSource")]
    pub source: String,
    #[serde(alias = "baseSha256")]
    pub expected_sha256: Option<String>,
    pub is_new: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiDraftPrepareResult {
    pub draft_id: String,
    pub revision: i64,
    pub preview: DraftPreview,
}

struct GuiProjectContext {
    project_id: String,
    project_root: PathBuf,
    dev_root: PathBuf,
    project_relative_dev: PathBuf,
}

pub fn status(
    project_service: &ProjectService,
    project_id: &str,
) -> Result<GuiDesignerStatus, String> {
    let project = ensure_active_project(project_service, project_id)?;
    let dev = PathBuf::from(&project.client_root).join("dev");
    let dev_root = dev.to_string_lossy().into_owned();
    if !dev.is_dir() {
        return Ok(GuiDesignerStatus {
            project_id: project_id.to_string(),
            dev_root,
            available: false,
            gui_export_available: false,
            resource_available: false,
            reason: Some("GUI_DEV_MISSING: 当前项目缺少客户端/dev".to_string()),
        });
    }
    let context = active_context(project_service, project_id)?;
    Ok(GuiDesignerStatus {
        project_id: project_id.to_string(),
        dev_root: context.dev_root.to_string_lossy().into_owned(),
        available: true,
        gui_export_available: context.dev_root.join("GUIExport").is_dir(),
        resource_available: context.dev_root.join("res").is_dir(),
        reason: None,
    })
}

pub fn list_documents(
    project_service: &ProjectService,
    project_id: &str,
) -> Result<GuiDocumentList, String> {
    let context = active_context(project_service, project_id)?;
    let mut editable_paths = Vec::new();
    let mut readonly_paths = Vec::new();
    collect_lua_files(
        &context.dev_root,
        &context.dev_root.join("GUIExport"),
        &mut editable_paths,
    )?;
    collect_lua_files(
        &context.dev_root,
        &context.dev_root.join("GUILayout"),
        &mut readonly_paths,
    )?;

    let editable_lookup: HashMap<String, String> = editable_paths
        .iter()
        .map(|path| (path.to_ascii_lowercase(), path.clone()))
        .collect();
    let mut entries = Vec::with_capacity(editable_paths.len() + readonly_paths.len());
    for path in editable_paths {
        let pc = is_pc_path(&path);
        let peer_candidate = if pc {
            mobile_peer_path(&path)
        } else {
            pc_peer_path(&path)
        };
        let peer_path = editable_lookup
            .get(&peer_candidate.to_ascii_lowercase())
            .cloned();
        let platform = match (pc, peer_path.is_some()) {
            (true, _) => GuiPlatform::Pc,
            (false, true) => GuiPlatform::Mobile,
            (false, false) => GuiPlatform::Shared,
        };
        entries.push(GuiDocumentEntry {
            path,
            kind: GuiDocumentKind::Editable,
            platform,
            peer_path,
        });
    }
    entries.extend(readonly_paths.into_iter().map(|path| GuiDocumentEntry {
        path,
        kind: GuiDocumentKind::Readonly,
        platform: GuiPlatform::Shared,
        peer_path: None,
    }));
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(GuiDocumentList {
        project_id: context.project_id,
        entries,
    })
}

pub fn open_document(
    project_service: &ProjectService,
    project_id: &str,
    dev_relative_path: &str,
    draft_id: Option<&str>,
) -> Result<GuiDocumentEnvelope, String> {
    let context = active_context(project_service, project_id)?;
    let path = validate_gui_export_path(dev_relative_path)?;
    let target = existing_file(&context.dev_root, &path, "GUI_DOCUMENT")?;
    if !target.is_file() {
        return Err("GUI_DOCUMENT_NOT_FOUND: GUIExport Lua 文件不存在".to_string());
    }
    let project_relative = project_relative_path(&context, &path)?;
    let opened = project_service
        .store()
        .safe_text_open(project_id, &project_relative, draft_id)?;
    ensure_source_size(&opened.content)?;
    let newline = opened.newline.clone().unwrap_or_else(|| "\n".to_string());
    let document = parse_document(
        &opened.content,
        &path,
        &opened.sha256,
        &opened.encoding,
        &newline,
    )?;
    Ok(GuiDocumentEnvelope {
        dev_relative_path: path,
        source: opened.content,
        document,
        sha256: Some(opened.sha256),
        encoding: opened.encoding,
        newline,
        draft_id: opened.draft_id,
        revision: opened.revision,
    })
}

pub fn reparse_document(
    project_service: &ProjectService,
    project_id: &str,
    request: GuiReparseRequest,
) -> Result<GuiDocumentEnvelope, String> {
    ensure_source_size(&request.working_source)?;
    let context = active_context(project_service, project_id)?;
    let path = validate_gui_export_path(&request.dev_relative_path)?;
    let (sha256, encoding, newline) = match optional_existing_file(&context.dev_root, &path)? {
        Some(_) => {
            let project_relative = project_relative_path(&context, &path)?;
            let opened =
                project_service
                    .store()
                    .safe_text_open(project_id, &project_relative, None)?;
            if request
                .expected_sha256
                .as_ref()
                .is_some_and(|expected| expected != &opened.sha256)
            {
                return Err("GUI_SOURCE_CONFLICT: 源文件已被外部修改".to_string());
            }
            (
                Some(opened.sha256),
                opened.encoding,
                opened.newline.unwrap_or_else(|| "\n".to_string()),
            )
        }
        None => {
            ensure_new_target(&context, &path)?;
            (None, "UTF-8".to_string(), "\n".to_string())
        }
    };
    let working_hash = hash_bytes(request.working_source.as_bytes());
    let document = parse_document(
        &request.working_source,
        &path,
        &working_hash,
        &encoding,
        &newline,
    )?;
    Ok(GuiDocumentEnvelope {
        dev_relative_path: path,
        source: request.working_source,
        document,
        sha256,
        encoding,
        newline,
        draft_id: None,
        revision: 0,
    })
}

pub fn create_template(
    project_service: &ProjectService,
    project_id: &str,
    request: GuiTemplateRequest,
) -> Result<GuiTemplateResponse, String> {
    let context = active_context(project_service, project_id)?;
    let base_path = normalize_template_path(&request.path)?;
    let paths = match request.platform {
        GuiTemplateTarget::Mobile => vec![(base_path, GuiPlatform::Mobile)],
        GuiTemplateTarget::Pc => vec![(pc_peer_path(&base_path), GuiPlatform::Pc)],
        GuiTemplateTarget::Both => vec![
            (base_path.clone(), GuiPlatform::Mobile),
            (pc_peer_path(&base_path), GuiPlatform::Pc),
        ],
    };
    let mut seen = HashSet::new();
    let mut documents = Vec::with_capacity(paths.len());
    for (path, platform) in paths {
        if !seen.insert(path.to_ascii_lowercase()) {
            return Err("GUI_TEMPLATE_PATH_CONFLICT: 双端文件名发生冲突".to_string());
        }
        ensure_new_target(&context, &path)?;
        let source = generate_template("\n");
        let sha = hash_bytes(source.as_bytes());
        let mut document = parse_document(&source, &path, &sha, "UTF-8", "\n")?;
        document.viewport = match platform {
            GuiPlatform::Mobile => Mir3UiViewport {
                width: MOBILE_WIDTH,
                height: MOBILE_HEIGHT,
            },
            GuiPlatform::Pc => request.pc_resolution.unwrap_or(Mir3UiViewport {
                width: DEFAULT_PC_WIDTH,
                height: DEFAULT_PC_HEIGHT,
            }),
            GuiPlatform::Shared => document.viewport,
        };
        documents.push(GuiDocumentEnvelope {
            dev_relative_path: path,
            source,
            document,
            sha256: None,
            encoding: "UTF-8".to_string(),
            newline: "\n".to_string(),
            draft_id: None,
            revision: 0,
        });
    }
    Ok(GuiTemplateResponse { documents })
}

pub fn list_dev_tree(
    project_service: &ProjectService,
    project_id: &str,
    parent_path: &str,
    cursor: Option<&str>,
) -> Result<GuiDevTreePage, String> {
    let context = active_context(project_service, project_id)?;
    let parent = validate_tree_parent(parent_path)?;
    let directory = existing_directory(&context.dev_root, &parent, "GUI_DEV_TREE")?;
    let offset = parse_tree_cursor(cursor)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(&directory)
        .map_err(|e| format!("GUI_DEV_TREE_READ_FAILED: {}: {e}", directory.display()))?
    {
        let entry = entry.map_err(|e| format!("GUI_DEV_TREE_READ_FAILED: {e}"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|e| format!("GUI_DEV_TREE_METADATA_FAILED: {e}"))?;
        if is_link_or_reparse(&metadata) {
            continue;
        }
        let file_type = if metadata.is_dir() {
            GuiDevEntryType::Directory
        } else if metadata.is_file() {
            GuiDevEntryType::File
        } else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if parent.is_empty() {
            name.clone()
        } else {
            format!("{parent}/{name}")
        };
        let has_children = metadata.is_dir() && directory_has_children(&entry.path())?;
        entries.push(GuiDevTreeEntry {
            policy: dev_entry_policy(&path, file_type),
            description_id: dev_description_id(&path, file_type),
            hidden: name.starts_with('.'),
            size: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
            has_children,
            path,
            name,
            entry_type: file_type,
        });
    }
    entries.sort_by(|left, right| {
        let left_rank = u8::from(left.entry_type == GuiDevEntryType::File);
        let right_rank = u8::from(right.entry_type == GuiDevEntryType::File);
        left_rank
            .cmp(&right_rank)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });
    if offset > entries.len() {
        return Err("GUI_DEV_TREE_CURSOR_INVALID: cursor 超出目录范围".to_string());
    }
    let end = offset.saturating_add(DEV_TREE_PAGE_SIZE).min(entries.len());
    let next_cursor = (end < entries.len()).then(|| end.to_string());
    Ok(GuiDevTreePage {
        parent_path: parent,
        entries: entries[offset..end].to_vec(),
        next_cursor,
        metadata_version: DEV_METADATA_DOCUMENT_VERSION.to_string(),
    })
}

pub fn open_readonly_document(
    project_service: &ProjectService,
    project_id: &str,
    dev_relative_path: &str,
) -> Result<GuiReadonlyDocument, String> {
    let context = active_context(project_service, project_id)?;
    let path = validate_guilayout_path(dev_relative_path)?;
    existing_file(&context.dev_root, &path, "GUI_READONLY_DOCUMENT")?;
    let project_relative = project_relative_path(&context, &path)?;
    let opened = project_service
        .store()
        .safe_text_open(project_id, &project_relative, None)?;
    ensure_source_size(&opened.content)?;
    Ok(GuiReadonlyDocument {
        dev_relative_path: path,
        source: opened.content,
        sha256: opened.sha256,
        encoding: opened.encoding,
        newline: opened.newline.unwrap_or_else(|| "\n".to_string()),
        read_only: true,
    })
}

pub fn read_asset_content(
    project_service: &ProjectService,
    project_id: &str,
    logical_path: &str,
) -> Result<GuiAssetContent, String> {
    let context = active_context(project_service, project_id)?;
    let relative = normalize_asset_path(logical_path)?;
    let target = existing_file(&context.dev_root, &relative, "GUI_ASSET")?;
    let metadata = fs::metadata(&target)
        .map_err(|e| format!("GUI_ASSET_METADATA_FAILED: {}: {e}", target.display()))?;
    if metadata.len() > MAX_ASSET_BYTES {
        return Err("GUI_ASSET_TOO_LARGE: 素材不能超过 16 MiB".to_string());
    }
    let mut bytes = Vec::with_capacity(metadata.len().min(MAX_ASSET_BYTES) as usize);
    File::open(&target)
        .and_then(|file| {
            file.take(MAX_ASSET_BYTES + 1)
                .read_to_end(&mut bytes)
                .map(|_| ())
        })
        .map_err(|e| format!("GUI_ASSET_READ_FAILED: {}: {e}", target.display()))?;
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err("GUI_ASSET_TOO_LARGE: 素材不能超过 16 MiB".to_string());
    }
    let mime_type = validate_asset_container(&target, &bytes)?;
    Ok(GuiAssetContent {
        logical_path: relative,
        mime_type: mime_type.to_string(),
        sha256: hash_bytes(&bytes),
        bytes,
    })
}

pub fn read_asset_meta(
    project_service: &ProjectService,
    project_id: &str,
    logical_path: &str,
) -> Result<GuiAssetMeta, String> {
    let content = read_asset_content(project_service, project_id, logical_path)?;
    let (width, height) = image_dimensions(&content.bytes, &content.mime_type)?;
    Ok(GuiAssetMeta {
        logical_path: content.logical_path,
        mime_type: content.mime_type,
        byte_length: content.bytes.len() as u64,
        sha256: content.sha256,
        width,
        height,
    })
}

pub fn prepare_draft(
    project_service: &ProjectService,
    project_id: &str,
    request: GuiDraftChangeSet,
) -> Result<GuiDraftPrepareResult, String> {
    let context = active_context(project_service, project_id)?;
    if request.files.is_empty() {
        return Err("GUI_DRAFT_FILES_EMPTY: 至少需要一个 GUI 文件变更".to_string());
    }
    let mut unique_paths = HashSet::new();
    let mut validated = Vec::with_capacity(request.files.len());
    for file in &request.files {
        ensure_source_size(&file.source)?;
        let path = validate_gui_export_path(&file.dev_relative_path)?;
        if !unique_paths.insert(path.to_ascii_lowercase()) {
            return Err("GUI_DRAFT_CASE_CONFLICT: 同批变更包含重复路径".to_string());
        }
        let existing = optional_existing_file(&context.dev_root, &path)?;
        if file
            .is_new
            .is_some_and(|is_new| is_new != existing.is_none())
        {
            return Err("GUI_DRAFT_FILE_STATE_CONFLICT: isNew 与当前文件状态不一致".to_string());
        }
        let source_bytes = existing
            .as_ref()
            .map(fs::read)
            .transpose()
            .map_err(|e| format!("GUI_DOCUMENT_READ_FAILED: {e}"))?;
        let source_hash = source_bytes.as_deref().map(hash_bytes);
        if source_hash != file.expected_sha256 {
            return Err("GUI_SOURCE_CONFLICT: 源文件已被外部修改或路径状态已变化".to_string());
        }
        if existing.is_none() {
            ensure_new_target(&context, &path)?;
        }
        let project_relative = project_relative_path(&context, &path)?;
        let (encoding, newline) = if existing.is_some() {
            let opened =
                project_service
                    .store()
                    .safe_text_open(project_id, &project_relative, None)?;
            (
                opened.encoding,
                opened.newline.unwrap_or_else(|| "\n".to_string()),
            )
        } else {
            ("UTF-8".to_string(), "\n".to_string())
        };
        let parse_sha = source_hash
            .clone()
            .unwrap_or_else(|| hash_bytes(file.source.as_bytes()));
        let document = parse_document(&file.source, &path, &parse_sha, &encoding, &newline)?;
        if document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
        {
            return Err(format!(
                "GUI_SOURCE_INVALID: {path} 包含 Lua 语法错误，不能生成 Draft"
            ));
        }
        validated.push((project_relative, source_bytes, source_hash, newline));
    }

    let draft = match request.draft_id.as_deref() {
        Some(id) => project_service.store().get_draft(project_id, id)?,
        None => {
            let draft = project_service.store().open_draft(
                project_id,
                &format!("GUI 编辑 {} 个文件", request.files.len()),
            )?;
            project_service.store().bind_draft_domain(
                project_id,
                &draft.id,
                "__studio_gui__",
                env!("CARGO_PKG_VERSION"),
                None,
            )?;
            draft
        }
    };
    if draft.revision != request.expected_revision {
        return Err(format!(
            "DRAFT_REVISION_CONFLICT: expected {}, current {}",
            request.expected_revision, draft.revision
        ));
    }

    let mut changes = Vec::with_capacity(validated.len());
    for (index, (project_relative, source_bytes, source_hash, newline)) in
        validated.into_iter().enumerate()
    {
        let file = &request.files[index];
        let content = if let Some(source_bytes) = source_bytes {
            let current_bytes = project_service
                .store()
                .draft_change_bytes(project_id, &draft.id, &project_relative)?
                .unwrap_or(source_bytes);
            let opened = project_service.store().safe_text_open(
                project_id,
                &project_relative,
                Some(&draft.id),
            )?;
            patch_supported_text_bytes(
                &current_bytes,
                &opened.content,
                &file.source,
                Some(&newline),
            )?
        } else {
            file.source.as_bytes().to_vec()
        };
        changes.push(DraftBinaryChangeInput {
            path: project_relative,
            content,
            expected_sha256: source_hash,
        });
    }
    let preview = project_service.store().patch_draft_bytes(
        project_id,
        &draft.id,
        draft.revision,
        &changes,
    )?;
    validate_gui_preview(&context, &preview)?;
    Ok(GuiDraftPrepareResult {
        draft_id: draft.id,
        revision: preview.draft.revision,
        preview,
    })
}

fn ensure_source_size(source: &str) -> Result<(), String> {
    if source.len() > MAX_GUI_SOURCE_BYTES {
        return Err("GUI_SOURCE_TOO_LARGE: GUI Lua 源码不能超过 8 MiB".to_string());
    }
    Ok(())
}

pub fn confirm_draft(
    project_service: &ProjectService,
    project_id: &str,
    draft_id: &str,
) -> Result<DraftConfirmation, String> {
    let context = active_context(project_service, project_id)?;
    let preview = project_service
        .store()
        .preview_draft(project_id, draft_id)?;
    validate_gui_preview(&context, &preview)?;
    project_service.create_confirmation(project_id, draft_id)
}

pub fn apply_draft(
    project_service: &ProjectService,
    project_id: &str,
    draft_id: &str,
    confirmation_token: &str,
) -> Result<Snapshot, String> {
    let context = active_context(project_service, project_id)?;
    let preview = project_service
        .store()
        .preview_draft(project_id, draft_id)?;
    validate_gui_preview(&context, &preview)?;
    let confirmation =
        project_service.consume_confirmation(project_id, draft_id, confirmation_token)?;
    project_service.store().apply_draft(
        project_id,
        draft_id,
        confirmation.revision,
        &confirmation.diff_hash,
    )
}

fn ensure_active_project(
    project_service: &ProjectService,
    project_id: &str,
) -> Result<mir3_domain::Mir3Project, String> {
    let active = project_service
        .store()
        .active_project()?
        .ok_or_else(|| "GUI_PROJECT_NOT_ACTIVE: 请先激活一个 996 项目".to_string())?;
    if active.id != project_id {
        return Err("GUI_PROJECT_NOT_ACTIVE: 请求项目不是当前激活项目".to_string());
    }
    Ok(active)
}

fn active_context(
    project_service: &ProjectService,
    project_id: &str,
) -> Result<GuiProjectContext, String> {
    let project = ensure_active_project(project_service, project_id)?;
    let project_root =
        fs::canonicalize(&project.root).map_err(|e| format!("GUI_PROJECT_PATH_INVALID: {e}"))?;
    let client_root = fs::canonicalize(&project.client_root)
        .map_err(|e| format!("GUI_CLIENT_PATH_INVALID: {e}"))?;
    if !client_root.starts_with(&project_root) {
        return Err("GUI_CLIENT_PATH_OUTSIDE: 客户端目录超出项目根".to_string());
    }
    let dev_candidate = client_root.join("dev");
    reject_symlink(&dev_candidate, "GUI_DEV_SYMLINK")?;
    let dev_root = fs::canonicalize(&dev_candidate)
        .map_err(|e| format!("GUI_DEV_MISSING: {}: {e}", dev_candidate.display()))?;
    if !dev_root.is_dir() || !dev_root.starts_with(&client_root) {
        return Err("GUI_DEV_PATH_OUTSIDE: 客户端/dev 路径无效".to_string());
    }
    let project_relative_dev = dev_root
        .strip_prefix(&project_root)
        .map_err(|_| "GUI_DEV_PATH_OUTSIDE: 客户端/dev 超出项目根".to_string())?
        .to_path_buf();
    Ok(GuiProjectContext {
        project_id: project.id,
        project_root,
        dev_root,
        project_relative_dev,
    })
}

fn validate_gui_export_path(value: &str) -> Result<String, String> {
    let normalized = validate_dev_relative(value, "GUI_DOCUMENT_PATH_INVALID")?;
    let path = Path::new(&normalized);
    if path.components().next().and_then(component_text) != Some("GUIExport")
        || !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("lua"))
    {
        return Err("GUI_DOCUMENT_PATH_INVALID: 仅允许编辑 GUIExport 下的 Lua 文件".to_string());
    }
    Ok(normalized)
}

fn validate_guilayout_path(value: &str) -> Result<String, String> {
    let normalized = validate_dev_relative(value, "GUI_READONLY_DOCUMENT_PATH_INVALID")?;
    let path = Path::new(&normalized);
    if path.components().next().and_then(component_text) != Some("GUILayout")
        || !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("lua"))
    {
        return Err(
            "GUI_READONLY_DOCUMENT_PATH_INVALID: 仅允许读取 GUILayout 下的 Lua 文件".to_string(),
        );
    }
    Ok(normalized)
}

fn validate_tree_parent(value: &str) -> Result<String, String> {
    if value.trim().is_empty() || value == "." {
        Ok(String::new())
    } else {
        validate_dev_relative(value, "GUI_DEV_TREE_PATH_INVALID")
    }
}

fn parse_tree_cursor(cursor: Option<&str>) -> Result<usize, String> {
    match cursor {
        None => Ok(0),
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| "GUI_DEV_TREE_CURSOR_INVALID: cursor 必须是非负整数".to_string()),
    }
}

fn dev_entry_policy(path: &str, entry_type: GuiDevEntryType) -> GuiDevPolicy {
    if entry_type == GuiDevEntryType::File
        && path.starts_with("GUIExport/")
        && has_extension(path, &["lua"])
    {
        GuiDevPolicy::Editable
    } else if path == "GUILayout" || path.starts_with("GUILayout/") {
        GuiDevPolicy::Readonly
    } else if entry_type == GuiDevEntryType::File
        && path.starts_with("res/")
        && has_extension(path, &["png", "jpg", "jpeg"])
    {
        GuiDevPolicy::Asset
    } else {
        GuiDevPolicy::Info
    }
}

fn dev_description_id(path: &str, entry_type: GuiDevEntryType) -> String {
    let components: Vec<String> = Path::new(path)
        .components()
        .filter_map(component_text)
        .map(str::to_ascii_lowercase)
        .collect();
    let first = components.first().map(String::as_str).unwrap_or("");
    let second = components.get(1).map(String::as_str).unwrap_or("");
    let third = components.get(2).map(String::as_str).unwrap_or("");
    let description = match (first, second, third) {
        ("scripts", "game_config", _) => "game_config",
        ("scripts", "ssr", _) => "ssr",
        ("scripts", _, _) => "scripts",
        ("game_config", _, _) => "game_config",
        ("ssr", _, _) => "ssr",
        ("res", "custom", _) => "res_custom",
        ("res", "item", _) => "res_item",
        ("res", "item_ground", _) => "res_item_ground",
        ("res", "player_show", _) => "res_player_show",
        ("res", "private", _) => "res_private",
        ("res", "official", "announce") => "res_official_announce",
        ("res", "official", "bag_ui") => "res_official_bag_ui",
        ("res", "official", "chat") => "res_official_chat",
        ("res", "official", "damage_num") => "res_official_damage_num",
        ("res", "official", "dark") => "res_official_dark",
        ("res", "official", "loading") => "res_official_loading",
        ("res", "official", "login") => "res_official_login",
        ("res", "official", "mail") => "res_official_mail",
        ("res", "official", "main") => "res_official_main",
        ("res", "official", "minimap") => "res_official_minimap",
        ("res", "official", "item_tips") => "res_official_item_tips",
        ("res", "official", "player_main_layer_ui") => "res_official_player_main",
        ("res", "official", "player_model") => "res_official_player_model",
        ("res", "official", "player_skill_layer_ui") => "res_official_player_skill",
        ("res", "official", "skill") => "res_official_skill",
        ("res", "official", "splash") => "res_official_splash",
        ("res", "official", "trade") => "res_official_trade",
        ("res", "official", _) => "res_official",
        ("res", "public", _) => "res_public",
        ("res", "skill_icon", _) => "res_skill_icon",
        ("res", "skill_icon_c", _) => "res_skill_icon_c",
        ("res", _, _) if components.len() > 1 && entry_type == GuiDevEntryType::Directory => {
            "res_subdirectory"
        }
        ("res", _, _) => "res",
        ("anim", "effect", _) => "anim_effect",
        ("anim", "hair", _) => "anim_hair",
        ("anim", "monster", _) => "anim_monster",
        ("anim", "npc", _) => "anim_npc",
        ("anim", "player", _) => "anim_player",
        ("anim", "weapon", _) => "anim_weapon",
        ("anim", _, _) => "anim",
        ("scene", "map", _) => "scene_map",
        ("scene", "objects", _) => "scene_objects",
        ("scene", "smtiles", _) => "scene_smtiles",
        ("scene", "tiles", _) => "scene_tiles",
        ("scene", "uiminimap", _) => "scene_uiminimap",
        ("scene", _, _) => "scene",
        ("data_config", _, _) => "data_config",
        ("guiexport", _, _) => "GUIExport",
        ("guilayout", _, _) => "GUILayout",
        ("guidata", _, _) => "GUIData",
        _ => "custom",
    };
    description.to_string()
}

fn has_extension(path: &str, allowed: &[&str]) -> bool {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| allowed.iter().any(|item| value.eq_ignore_ascii_case(item)))
}

fn normalize_template_path(value: &str) -> Result<String, String> {
    let value = if Path::new(value).extension().is_none() {
        format!("{value}.lua")
    } else {
        value.to_string()
    };
    let value = if Path::new(&value)
        .components()
        .next()
        .and_then(component_text)
        == Some("GUIExport")
    {
        value
    } else {
        format!("GUIExport/{value}")
    };
    validate_gui_export_path(&value)
}

fn normalize_asset_path(value: &str) -> Result<String, String> {
    let normalized = validate_dev_relative(value, "GUI_ASSET_PATH_INVALID")?;
    let relative = if Path::new(&normalized)
        .components()
        .next()
        .and_then(component_text)
        == Some("res")
    {
        normalized
    } else {
        format!("res/{normalized}")
    };
    let extension = Path::new(&relative)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(extension.as_deref(), Some("png" | "jpg" | "jpeg")) {
        return Err("GUI_ASSET_TYPE_UNSUPPORTED: 仅支持 PNG/JPG".to_string());
    }
    Ok(relative)
}

fn validate_asset_container(path: &Path, bytes: &[u8]) -> Result<&'static str, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "GUI_ASSET_TYPE_UNSUPPORTED: 仅支持 PNG/JPG".to_string())?;
    match extension.as_str() {
        "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => Ok("image/png"),
        "jpg" | "jpeg" if bytes.starts_with(b"\xff\xd8\xff") => Ok("image/jpeg"),
        "png" | "jpg" | "jpeg" => {
            Err("GUI_ASSET_CONTAINER_INVALID: 素材内容与 PNG/JPG 扩展名不符".to_string())
        }
        _ => Err("GUI_ASSET_TYPE_UNSUPPORTED: 仅支持 PNG/JPG".to_string()),
    }
}

fn image_dimensions(bytes: &[u8], mime_type: &str) -> Result<(u32, u32), String> {
    let dimensions = match mime_type {
        "image/png" => png_dimensions(bytes),
        "image/jpeg" => jpeg_dimensions(bytes),
        _ => None,
    };
    dimensions
        .filter(|(width, height)| *width > 0 && *height > 0)
        .ok_or_else(|| "GUI_ASSET_DIMENSIONS_INVALID: 无法读取图片尺寸".to_string())
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.get(12..16) != Some(b"IHDR")
    {
        return None;
    }
    Some((
        u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?),
        u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?),
    ))
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(b"\xff\xd8\xff") {
        return None;
    }
    let mut cursor = 2usize;
    while cursor < bytes.len() {
        while bytes.get(cursor) == Some(&0xff) {
            cursor += 1;
        }
        let marker = *bytes.get(cursor)?;
        cursor += 1;
        if marker == 0xd9 || marker == 0xda {
            break;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let length = u16::from_be_bytes(bytes.get(cursor..cursor + 2)?.try_into().ok()?) as usize;
        if length < 2 || cursor.checked_add(length)? > bytes.len() {
            return None;
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
        ) {
            if length < 7 {
                return None;
            }
            let height = u16::from_be_bytes(bytes.get(cursor + 3..cursor + 5)?.try_into().ok()?);
            let width = u16::from_be_bytes(bytes.get(cursor + 5..cursor + 7)?.try_into().ok()?);
            return Some((u32::from(width), u32::from(height)));
        }
        cursor += length;
    }
    None
}

fn validate_dev_relative(value: &str, prefix: &str) -> Result<String, String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || value.chars().any(char::is_control)
    {
        return Err(format!("{prefix}: 路径必须是 dev 下的安全相对路径"));
    }
    for component in path.components().filter_map(component_text) {
        if is_windows_reserved(component) {
            return Err(format!("{prefix}: 路径包含 Windows 保留名"));
        }
    }
    Ok(path
        .components()
        .filter_map(component_text)
        .collect::<Vec<_>>()
        .join("/"))
}

fn component_text(component: Component<'_>) -> Option<&str> {
    match component {
        Component::Normal(value) => value.to_str(),
        _ => None,
    }
}

fn is_windows_reserved(component: &str) -> bool {
    let stem = component
        .trim_end_matches([' ', '.'])
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .and_then(|value| value.parse::<u8>().ok())
            .is_some_and(|value| (1..=9).contains(&value))
}

fn reject_symlink(path: &Path, prefix: &str) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("{prefix}: {}: {e}", path.display()))?;
    if is_link_or_reparse(&metadata) {
        Err(format!("{prefix}: 不允许符号链接或 reparse point"))
    } else {
        Ok(())
    }
}

fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    let is_reparse_point = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let is_reparse_point = false;
    metadata.file_type().is_symlink() || is_reparse_point
}

fn existing_file(root: &Path, relative: &str, prefix: &str) -> Result<PathBuf, String> {
    let target = root.join(relative);
    let mut cursor = root.to_path_buf();
    for component in Path::new(relative).components() {
        let name =
            component_text(component).ok_or_else(|| format!("{prefix}_PATH_INVALID: 路径无效"))?;
        cursor.push(name);
        reject_symlink(&cursor, &format!("{prefix}_SYMLINK"))?;
    }
    let canonical = fs::canonicalize(&target)
        .map_err(|e| format!("{prefix}_NOT_FOUND: {}: {e}", target.display()))?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        return Err(format!("{prefix}_PATH_OUTSIDE: 文件超出允许目录"));
    }
    Ok(canonical)
}

fn existing_directory(root: &Path, relative: &str, prefix: &str) -> Result<PathBuf, String> {
    if relative.is_empty() {
        return Ok(root.to_path_buf());
    }
    let target = root.join(relative);
    let mut cursor = root.to_path_buf();
    for component in Path::new(relative).components() {
        let name =
            component_text(component).ok_or_else(|| format!("{prefix}_PATH_INVALID: 路径无效"))?;
        cursor.push(name);
        reject_symlink(&cursor, &format!("{prefix}_SYMLINK"))?;
    }
    let canonical = fs::canonicalize(&target)
        .map_err(|e| format!("{prefix}_NOT_FOUND: {}: {e}", target.display()))?;
    if !canonical.starts_with(root) || !canonical.is_dir() {
        return Err(format!("{prefix}_PATH_OUTSIDE: 目录超出允许范围或不是目录"));
    }
    Ok(canonical)
}

fn directory_has_children(directory: &Path) -> Result<bool, String> {
    for entry in fs::read_dir(directory)
        .map_err(|e| format!("GUI_DEV_TREE_READ_FAILED: {}: {e}", directory.display()))?
    {
        let entry = entry.map_err(|e| format!("GUI_DEV_TREE_READ_FAILED: {e}"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|e| format!("GUI_DEV_TREE_METADATA_FAILED: {e}"))?;
        if !is_link_or_reparse(&metadata) && (metadata.is_dir() || metadata.is_file()) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn optional_existing_file(root: &Path, relative: &str) -> Result<Option<PathBuf>, String> {
    let target = root.join(relative);
    if target.exists() {
        existing_file(root, relative, "GUI_DOCUMENT").map(Some)
    } else {
        Ok(None)
    }
}

fn ensure_new_target(context: &GuiProjectContext, relative: &str) -> Result<(), String> {
    let target = context.dev_root.join(relative);
    if target.exists() {
        return Err("GUI_DOCUMENT_EXISTS: 目标文件已存在".to_string());
    }
    let parent = target
        .parent()
        .ok_or_else(|| "GUI_DOCUMENT_PATH_INVALID: 目标缺少父目录".to_string())?;
    let mut existing = parent;
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or_else(|| "GUI_DOCUMENT_PATH_INVALID: 找不到安全父目录".to_string())?;
    }
    let canonical =
        fs::canonicalize(existing).map_err(|e| format!("GUI_DOCUMENT_PATH_INVALID: {e}"))?;
    let export_root = fs::canonicalize(context.dev_root.join("GUIExport"))
        .map_err(|e| format!("GUI_EXPORT_MISSING: {e}"))?;
    if !canonical.starts_with(&export_root) {
        return Err("GUI_DOCUMENT_PATH_OUTSIDE: 新文件必须位于 GUIExport".to_string());
    }
    let relative_existing = existing
        .strip_prefix(&context.dev_root)
        .map_err(|_| "GUI_DOCUMENT_PATH_OUTSIDE: 父目录超出 dev".to_string())?;
    let mut cursor = context.dev_root.clone();
    for component in relative_existing.components() {
        cursor.push(component.as_os_str());
        reject_symlink(&cursor, "GUI_DOCUMENT_SYMLINK")?;
    }
    ensure_no_case_conflict(&export_root, &target)?;
    Ok(())
}

fn ensure_no_case_conflict(export_root: &Path, target: &Path) -> Result<(), String> {
    let expected = target.to_string_lossy().to_ascii_lowercase();
    let mut files = Vec::new();
    collect_all_paths(export_root, &mut files)?;
    if files
        .iter()
        .any(|path| path.to_string_lossy().to_ascii_lowercase() == expected)
    {
        Err("GUI_DOCUMENT_CASE_CONFLICT: 存在仅大小写不同的路径".to_string())
    } else {
        Ok(())
    }
}

fn collect_all_paths(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|e| format!("GUI_LIST_FAILED: {e}"))? {
        let entry = entry.map_err(|e| format!("GUI_LIST_FAILED: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("GUI_LIST_FAILED: {e}"))?;
        if file_type.is_symlink() {
            continue;
        }
        output.push(entry.path());
        if file_type.is_dir() {
            collect_all_paths(&entry.path(), output)?;
        }
    }
    Ok(())
}

fn collect_lua_files(dev_root: &Path, root: &Path, output: &mut Vec<String>) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    reject_symlink(root, "GUI_LIST_SYMLINK")?;
    let mut entries = fs::read_dir(root)
        .map_err(|e| format!("GUI_LIST_FAILED: {}: {e}", root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("GUI_LIST_FAILED: {e}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry
            .file_type()
            .map_err(|e| format!("GUI_LIST_FAILED: {e}"))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_lua_files(dev_root, &entry.path(), output)?;
        } else if file_type.is_file()
            && entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("lua"))
        {
            let relative = entry
                .path()
                .strip_prefix(dev_root)
                .map_err(|_| "GUI_LIST_PATH_OUTSIDE: 文件超出 dev".to_string())?
                .components()
                .filter_map(component_text)
                .collect::<Vec<_>>()
                .join("/");
            output.push(relative);
        }
    }
    Ok(())
}

fn project_relative_path(
    context: &GuiProjectContext,
    dev_relative: &str,
) -> Result<String, String> {
    let path = context.project_relative_dev.join(dev_relative);
    if context
        .project_root
        .join(&path)
        .starts_with(&context.project_root)
    {
        Ok(path
            .components()
            .filter_map(component_text)
            .collect::<Vec<_>>()
            .join("/"))
    } else {
        Err("GUI_DOCUMENT_PATH_OUTSIDE: 文件超出项目".to_string())
    }
}

fn validate_gui_preview(context: &GuiProjectContext, preview: &DraftPreview) -> Result<(), String> {
    let allowed = context.project_relative_dev.join("GUIExport");
    let mut unique_paths = HashSet::new();
    for change in &preview.changes {
        if change.deleted {
            return Err("GUI_DRAFT_DELETE_UNSUPPORTED: V0.1 不允许删除 GUI 文件".to_string());
        }
        let path = Path::new(&change.path);
        if !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("lua"))
            || !path.starts_with(&allowed)
        {
            return Err("GUI_DRAFT_PATH_OUTSIDE: Draft 包含非 GUIExport Lua 文件".to_string());
        }
        let relative = path
            .strip_prefix(&context.project_relative_dev)
            .map_err(|_| "GUI_DRAFT_PATH_OUTSIDE: Draft 超出 dev".to_string())?;
        validate_gui_export_path(&relative.to_string_lossy())?;
        if !unique_paths.insert(change.path.to_ascii_lowercase()) {
            return Err("GUI_DRAFT_CASE_CONFLICT: Draft 包含仅大小写不同的重复路径".to_string());
        }
        if context.project_root.join(path).exists() {
            existing_file(&context.dev_root, &relative.to_string_lossy(), "GUI_DRAFT")?;
        } else {
            ensure_new_target(context, &relative.to_string_lossy())?;
        }
    }
    Ok(())
}

fn is_pc_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with("_win32.lua")
}

fn pc_peer_path(path: &str) -> String {
    if is_pc_path(path) {
        return path.to_string();
    }
    path.strip_suffix(".lua")
        .map(|value| format!("{value}_win32.lua"))
        .unwrap_or_else(|| format!("{path}_win32.lua"))
}

fn mobile_peer_path(path: &str) -> String {
    if is_pc_path(path) {
        let keep = path.len() - "_win32.lua".len();
        format!("{}.lua", &path[..keep])
    } else {
        path.to_string()
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn fixture_service() -> (PathBuf, ProjectService, String) {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "mir3-gui-service-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        let project = base.join("project");
        fs::create_dir_all(project.join("客户端/dev/GUIExport/demo")).unwrap();
        fs::create_dir_all(project.join("客户端/dev/GUIData")).unwrap();
        fs::create_dir_all(project.join("客户端/dev/GUILayout")).unwrap();
        fs::create_dir_all(project.join("客户端/dev/res/icons")).unwrap();
        fs::create_dir_all(project.join("引擎")).unwrap();
        let template = generate_template("\r\n");
        fs::write(
            project.join("客户端/dev/GUIExport/demo/main.lua"),
            &template,
        )
        .unwrap();
        fs::write(
            project.join("客户端/dev/GUIExport/demo/main_win32.lua"),
            &template,
        )
        .unwrap();
        fs::write(project.join("客户端/dev/GUILayout/demo.lua"), "return {}\n").unwrap();
        fs::write(project.join("客户端/dev/GUIData/secret.lua"), "return {}\n").unwrap();
        fs::write(
            project.join("客户端/dev/res/icons/close.png"),
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x20\x00\x00\x00\x10",
        )
        .unwrap();
        fs::write(
            project.join("客户端/dev/res/icons/photo.jpg"),
            b"\xff\xd8\xff\xc0\x00\x11\x08\x00\x10\x00\x20\x03\x01\x11\x00\x02\x11\x00\x03\x11\x00",
        )
        .unwrap();
        fs::write(project.join("客户端/dev/.hidden"), b"hidden").unwrap();
        fs::create_dir_all(project.join("客户端/dev/scripts")).unwrap();
        fs::create_dir_all(project.join("客户端/dev/scripts/game_config")).unwrap();
        fs::create_dir_all(project.join("客户端/dev/scripts/ssr")).unwrap();
        fs::create_dir_all(project.join("客户端/dev/MyCustom")).unwrap();
        let service = ProjectService::new(base.join("studio-data")).unwrap();
        let imported = service.store().import_project(&project).unwrap();
        service.store().activate_project(&imported.id).unwrap();
        (base, service, imported.id)
    }

    #[test]
    fn gui_paths_are_strictly_scoped() {
        assert!(validate_gui_export_path("GUIExport/auction/main.lua").is_ok());
        assert!(validate_gui_export_path("GUILayout/main.lua").is_err());
        assert!(validate_gui_export_path("GUIExport/../GUILayout/main.lua").is_err());
        assert!(normalize_asset_path("icons/a.png").is_ok());
        assert!(normalize_asset_path("../a.png").is_err());
    }

    #[test]
    fn platform_peer_names_are_exact() {
        assert_eq!(
            pc_peer_path("GUIExport/main_widgets.lua"),
            "GUIExport/main_widgets_win32.lua"
        );
        assert_eq!(
            mobile_peer_path("GUIExport/main_widgets_win32.lua"),
            "GUIExport/main_widgets.lua"
        );
    }

    #[test]
    fn reserved_windows_names_are_rejected_everywhere() {
        assert!(validate_gui_export_path("GUIExport/CON.lua").is_err());
        assert!(normalize_template_path("nested/LPT1.lua").is_err());
    }

    #[test]
    fn real_files_flow_through_list_asset_draft_apply_and_restore() {
        let (base, service, project_id) = fixture_service();
        let listed = list_documents(&service, &project_id).unwrap();
        assert_eq!(listed.entries.len(), 3);
        assert!(listed
            .entries
            .iter()
            .all(|entry| !entry.path.starts_with("GUIData/")));
        assert!(listed.entries.iter().any(|entry| {
            entry.path == "GUIExport/demo/main.lua"
                && entry.platform == GuiPlatform::Mobile
                && entry.peer_path.as_deref() == Some("GUIExport/demo/main_win32.lua")
        }));

        let asset = read_asset_content(&service, &project_id, "icons/close.png").unwrap();
        assert_eq!(asset.mime_type, "image/png");
        let opened = open_document(&service, &project_id, "GUIExport/demo/main.lua", None).unwrap();
        let working = opened
            .source
            .replace("return ui", "-- changed\r\nreturn ui");
        let prepared = prepare_draft(
            &service,
            &project_id,
            GuiDraftChangeSet {
                files: vec![GuiDraftFileChange {
                    dev_relative_path: opened.dev_relative_path,
                    source: working,
                    expected_sha256: opened.sha256,
                    is_new: Some(false),
                }],
                draft_id: None,
                expected_revision: 0,
            },
        )
        .unwrap();
        let target = base.join("project/客户端/dev/GUIExport/demo/main.lua");
        assert!(!fs::read_to_string(&target).unwrap().contains("changed"));
        let confirmation = confirm_draft(&service, &project_id, &prepared.draft_id).unwrap();
        let snapshot = apply_draft(
            &service,
            &project_id,
            &prepared.draft_id,
            &confirmation.confirmation_token,
        )
        .unwrap();
        assert!(fs::read_to_string(&target).unwrap().contains("changed"));
        service
            .store()
            .restore_snapshot(&project_id, &snapshot.id)
            .unwrap();
        assert!(!fs::read_to_string(&target).unwrap().contains("changed"));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn new_template_remains_external_until_confirmed_apply() {
        let (base, service, project_id) = fixture_service();
        let generated = create_template(
            &service,
            &project_id,
            GuiTemplateRequest {
                path: "custom/new_page".to_string(),
                platform: GuiTemplateTarget::Both,
                pc_resolution: None,
            },
        )
        .unwrap();
        assert_eq!(generated.documents.len(), 2);
        let targets: Vec<PathBuf> = generated
            .documents
            .iter()
            .map(|document| {
                base.join("project/客户端/dev")
                    .join(&document.dev_relative_path)
            })
            .collect();
        assert!(targets.iter().all(|target| !target.exists()));
        let prepared = prepare_draft(
            &service,
            &project_id,
            GuiDraftChangeSet {
                files: generated
                    .documents
                    .into_iter()
                    .map(|document| GuiDraftFileChange {
                        dev_relative_path: document.dev_relative_path,
                        source: document.source,
                        expected_sha256: None,
                        is_new: Some(true),
                    })
                    .collect(),
                draft_id: None,
                expected_revision: 0,
            },
        )
        .unwrap();
        assert_eq!(prepared.preview.changes.len(), 2);
        assert!(targets.iter().all(|target| !target.exists()));
        let confirmation = confirm_draft(&service, &project_id, &prepared.draft_id).unwrap();
        let snapshot = apply_draft(
            &service,
            &project_id,
            &prepared.draft_id,
            &confirmation.confirmation_token,
        )
        .unwrap();
        assert!(targets.iter().all(|target| target.is_file()));
        service
            .store()
            .restore_snapshot(&project_id, &snapshot.id)
            .unwrap();
        assert!(targets.iter().all(|target| !target.exists()));
        fs::remove_dir_all(base).ok();
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_gui_document_and_asset_are_rejected() {
        use std::os::unix::fs::symlink;

        let (base, service, project_id) = fixture_service();
        let outside = base.join("outside.lua");
        fs::write(&outside, generate_template("\n")).unwrap();
        symlink(&outside, base.join("project/客户端/dev/GUIExport/link.lua")).unwrap();
        assert!(
            open_document(&service, &project_id, "GUIExport/link.lua", None)
                .unwrap_err()
                .starts_with("GUI_DOCUMENT_SYMLINK")
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn dev_tree_is_one_level_sorted_classified_and_paginated() {
        let (base, service, project_id) = fixture_service();
        let root = list_dev_tree(&service, &project_id, "", None).unwrap();
        let first_file = root
            .entries
            .iter()
            .position(|entry| entry.entry_type == GuiDevEntryType::File)
            .unwrap();
        assert!(root.entries[..first_file]
            .iter()
            .all(|entry| entry.entry_type == GuiDevEntryType::Directory));
        assert!(root.entries[..first_file]
            .windows(2)
            .all(|pair| { pair[0].name.to_lowercase() <= pair[1].name.to_lowercase() }));
        assert!(root.entries.iter().any(|entry| {
            entry.path == "GUILayout"
                && entry.policy == GuiDevPolicy::Readonly
                && entry.description_id == "GUILayout"
        }));
        assert!(root
            .entries
            .iter()
            .any(|entry| { entry.path == "MyCustom" && entry.description_id == "custom" }));
        assert!(root
            .entries
            .iter()
            .any(|entry| entry.path == ".hidden" && entry.hidden));
        let scripts = list_dev_tree(&service, &project_id, "scripts", None).unwrap();
        assert!(scripts.entries.iter().any(|entry| {
            entry.path == "scripts/game_config" && entry.description_id == "game_config"
        }));
        assert!(scripts
            .entries
            .iter()
            .any(|entry| { entry.path == "scripts/ssr" && entry.description_id == "ssr" }));

        let export = list_dev_tree(&service, &project_id, "GUIExport/demo", None).unwrap();
        assert!(export
            .entries
            .iter()
            .all(|entry| entry.policy == GuiDevPolicy::Editable));
        let assets = list_dev_tree(&service, &project_id, "res/icons", None).unwrap();
        assert_eq!(assets.entries[0].policy, GuiDevPolicy::Asset);

        let paging = base.join("project/客户端/dev/paging");
        fs::create_dir_all(&paging).unwrap();
        for index in 0..503 {
            fs::write(paging.join(format!("file-{index:04}.txt")), b"x").unwrap();
        }
        let first = list_dev_tree(&service, &project_id, "paging", None).unwrap();
        assert_eq!(first.entries.len(), DEV_TREE_PAGE_SIZE);
        assert_eq!(first.next_cursor.as_deref(), Some("500"));
        let second = list_dev_tree(
            &service,
            &project_id,
            "paging",
            first.next_cursor.as_deref(),
        )
        .unwrap();
        assert_eq!(second.entries.len(), 3);
        assert!(second.next_cursor.is_none());
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn official_dev_directory_descriptions_are_specific() {
        assert_eq!(
            dev_description_id("res/official/bag_ui", GuiDevEntryType::Directory),
            "res_official_bag_ui"
        );
        assert_eq!(
            dev_description_id("res/item_ground", GuiDevEntryType::Directory),
            "res_item_ground"
        );
        assert_eq!(
            dev_description_id("anim/monster", GuiDevEntryType::Directory),
            "anim_monster"
        );
        assert_eq!(
            dev_description_id("scene/smtiles", GuiDevEntryType::Directory),
            "scene_smtiles"
        );
    }

    #[test]
    fn oversized_gui_source_is_rejected_before_parse() {
        let source = "x".repeat(MAX_GUI_SOURCE_BYTES + 1);
        assert!(ensure_source_size(&source)
            .unwrap_err()
            .starts_with("GUI_SOURCE_TOO_LARGE"));
    }

    #[test]
    fn tree_paths_and_readonly_documents_fail_closed() {
        let (base, service, project_id) = fixture_service();
        assert!(list_dev_tree(&service, &project_id, "../引擎", None).is_err());
        assert!(list_dev_tree(&service, &project_id, "/tmp", None).is_err());
        let opened = open_readonly_document(&service, &project_id, "GUILayout/demo.lua").unwrap();
        assert!(opened.read_only);
        assert!(open_readonly_document(&service, &project_id, "GUIExport/demo/main.lua").is_err());
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn asset_metadata_and_binary_limits_are_enforced() {
        let (base, service, project_id) = fixture_service();
        let meta = read_asset_meta(&service, &project_id, "res/icons/close.png").unwrap();
        assert_eq!((meta.width, meta.height), (32, 16));
        assert_eq!(meta.byte_length, 24);
        let content = read_asset_content(&service, &project_id, "icons/close.png").unwrap();
        assert_eq!(content.bytes.len() as u64, meta.byte_length);
        assert_eq!(content.sha256, meta.sha256);
        let jpeg = read_asset_meta(&service, &project_id, "icons/photo.jpg").unwrap();
        assert_eq!((jpeg.width, jpeg.height), (32, 16));
        assert_eq!(jpeg.mime_type, "image/jpeg");

        let oversized = base.join("project/客户端/dev/res/icons/oversized.png");
        let file = fs::File::create(&oversized).unwrap();
        file.set_len(MAX_ASSET_BYTES + 1).unwrap();
        assert!(
            read_asset_content(&service, &project_id, "icons/oversized.png")
                .unwrap_err()
                .starts_with("GUI_ASSET_TOO_LARGE")
        );
        assert!(read_asset_content(&service, &project_id, "../GUIData/secret.lua").is_err());
        fs::remove_dir_all(base).ok();
    }

    #[cfg(unix)]
    #[test]
    fn dev_tree_rejects_symlinked_parent() {
        use std::os::unix::fs::symlink;

        let (base, service, project_id) = fixture_service();
        symlink(
            base.join("project/引擎"),
            base.join("project/客户端/dev/outside-link"),
        )
        .unwrap();
        assert!(list_dev_tree(&service, &project_id, "outside-link", None)
            .unwrap_err()
            .starts_with("GUI_DEV_TREE_SYMLINK"));
        fs::remove_dir_all(base).ok();
    }
}
