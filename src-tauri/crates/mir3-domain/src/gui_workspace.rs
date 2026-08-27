//! GUI AI 私有工作副本。
//!
//! Studio 与 MCP 通过应用数据目录共享同一份可恢复工作副本。这里不读取或写入
//! 996 项目文件；正式保存仍由桌面端的保存节点流程负责。

use crate::DomainStore;
use fs2::FileExt;
use mir3_ui::{
    insert_core_node, insert_node_behavior, parse_document, replace_bound_property,
    CoreBehaviorType, CoreNodeType, DiagnosticSeverity, InsertCoreNodeRequest, Mir3UiDiagnostic,
    Mir3UiDocument, Mir3UiPropertyValue,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const GUI_WORKSPACE_SCHEMA_VERSION: u32 = 3;
const GUI_WORKSPACE_DIRECTORY: &str = "gui-ai-workspace";
const MAX_GUI_WORKSPACE_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const GUI_WORKSPACE_TOKEN_BYTES: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiWorkspaceSyncRequest {
    pub path: String,
    pub working_revision: i64,
    pub base_sha256: Option<String>,
    pub selected_node_id: Option<String>,
    pub dirty: bool,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiWorkspaceSnapshot {
    pub schema_version: u32,
    pub project_id: String,
    pub workspace_id: String,
    pub path: String,
    pub working_revision: i64,
    pub base_sha256: Option<String>,
    pub selected_node_id: Option<String>,
    pub dirty: bool,
    pub source: String,
    pub valid: bool,
    pub diagnostics: Vec<Mir3UiDiagnostic>,
    pub document: Mir3UiDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiWorkspaceSyncResult {
    pub workspace_id: String,
    pub workspace_token: String,
    pub path: String,
    pub source: String,
    pub base_sha256: Option<String>,
    pub working_revision: i64,
    pub valid: bool,
    pub diagnostics: Vec<Mir3UiDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GuiWorkspaceAuthorization {
    schema_version: u32,
    project_id: String,
    path: String,
    workspace_id: String,
    token_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GuiWorkspaceOperation {
    SetPosition {
        node_id: String,
        x: f64,
        y: f64,
    },
    SetProperty {
        node_id: String,
        property: String,
        value: Value,
    },
    AddNode {
        parent_node_id: String,
        node_type: CoreNodeType,
        name: String,
        x: f64,
        y: f64,
    },
    AddBehavior {
        node_id: String,
        behavior: CoreBehaviorType,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiWorkspaceOperateRequest {
    pub workspace_token: String,
    pub path: String,
    pub expected_revision: i64,
    pub operation: GuiWorkspaceOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiWorkspaceOperateResult {
    pub workspace_id: String,
    pub path: String,
    pub base_sha256: Option<String>,
    pub working_revision: i64,
    pub source: String,
    pub diagnostics: Vec<Mir3UiDiagnostic>,
    pub valid: bool,
}

impl DomainStore {
    /// 用 Studio 内存缓冲区覆盖私有工作副本；不会触碰项目源文件。
    pub fn sync_gui_workspace(
        &self,
        project_id: &str,
        request: &GuiWorkspaceSyncRequest,
    ) -> Result<GuiWorkspaceSyncResult, String> {
        self.ensure_writable()?;
        validate_gui_relative_path(&request.path)?;
        validate_revision(request.working_revision)?;
        validate_source(&request.source)?;
        self.get_project(project_id)?;
        let directory = self.gui_workspace_directory(project_id)?;
        fs::create_dir_all(&directory)
            .map_err(|error| format!("GUI_WORKSPACE_CREATE_FAILED: {error}"))?;
        let _lock = lock_workspace(&directory, &request.path)?;
        let document = parse_workspace_document(
            &request.source,
            &request.path,
            request.base_sha256.as_deref().unwrap_or_default(),
        )?;
        if request
            .selected_node_id
            .as_ref()
            .is_some_and(|node_id| !document.nodes.iter().any(|node| &node.id == node_id))
        {
            return Err(
                "GUI_WORKSPACE_SELECTED_NODE_INVALID: selected node is not in the document"
                    .to_string(),
            );
        }
        let workspace_id = format!("gui-{}", &workspace_key(&request.path)[..24]);
        let diagnostics = document.diagnostics.clone();
        let valid = !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error);
        let snapshot = GuiWorkspaceSnapshot {
            schema_version: GUI_WORKSPACE_SCHEMA_VERSION,
            project_id: project_id.to_string(),
            workspace_id: workspace_id.clone(),
            path: request.path.clone(),
            working_revision: request.working_revision,
            base_sha256: request.base_sha256.clone(),
            selected_node_id: request.selected_node_id.clone(),
            dirty: request.dirty,
            source: request.source.clone(),
            valid,
            diagnostics: diagnostics.clone(),
            document,
        };
        persist_snapshot(&directory, &snapshot)?;
        let workspace_token = generate_workspace_token()?;
        persist_authorization(
            &directory,
            &GuiWorkspaceAuthorization {
                schema_version: GUI_WORKSPACE_SCHEMA_VERSION,
                project_id: project_id.to_string(),
                path: request.path.clone(),
                workspace_id: workspace_id.clone(),
                token_hash: hash_token(&workspace_token),
            },
        )?;
        Ok(GuiWorkspaceSyncResult {
            workspace_id,
            workspace_token,
            path: snapshot.path,
            source: snapshot.source,
            base_sha256: snapshot.base_sha256,
            working_revision: snapshot.working_revision,
            valid,
            diagnostics,
        })
    }

    /// 读取 Studio 最近同步的私有工作副本。
    pub fn get_gui_workspace(
        &self,
        project_id: &str,
        path: &str,
    ) -> Result<GuiWorkspaceSnapshot, String> {
        validate_gui_relative_path(path)?;
        self.get_project(project_id)?;
        let directory = self.gui_workspace_directory(project_id)?;
        let _lock = lock_workspace(&directory, path)?;
        read_snapshot(&directory, project_id, path)
    }

    /// 在私有工作副本执行受限语义操作；调用方必须携带精确工作版本。
    pub fn operate_gui_workspace(
        &self,
        project_id: &str,
        request: &GuiWorkspaceOperateRequest,
    ) -> Result<GuiWorkspaceOperateResult, String> {
        self.ensure_writable()?;
        validate_gui_write_path(&request.path)?;
        validate_revision(request.expected_revision)?;
        self.get_project(project_id)?;
        let directory = self.gui_workspace_directory(project_id)?;
        let _lock = lock_workspace(&directory, &request.path)?;
        authorize_workspace_locked(
            &directory,
            project_id,
            &request.path,
            &request.workspace_token,
        )?;
        let mut snapshot = read_snapshot(&directory, project_id, &request.path)?;
        if snapshot.working_revision != request.expected_revision {
            return Err(format!(
                "GUI_WORKSPACE_REVISION_CONFLICT: expected {}, got {}",
                request.expected_revision, snapshot.working_revision
            ));
        }
        snapshot.source = apply_operation(&snapshot, &request.operation)?;
        validate_source(&snapshot.source)?;
        snapshot.working_revision = snapshot
            .working_revision
            .checked_add(1)
            .ok_or_else(|| "GUI_WORKSPACE_REVISION_OVERFLOW".to_string())?;
        snapshot.dirty = true;
        snapshot.document = parse_workspace_document(
            &snapshot.source,
            &snapshot.path,
            snapshot.base_sha256.as_deref().unwrap_or_default(),
        )?;
        let diagnostics = snapshot.document.diagnostics.clone();
        let valid = !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error);
        snapshot.diagnostics = diagnostics.clone();
        snapshot.valid = valid;
        persist_snapshot(&directory, &snapshot)?;
        Ok(GuiWorkspaceOperateResult {
            workspace_id: snapshot.workspace_id,
            path: snapshot.path,
            base_sha256: snapshot.base_sha256,
            working_revision: snapshot.working_revision,
            source: snapshot.source,
            diagnostics,
            valid,
        })
    }

    /// 校验 GUI AI 工作区令牌；普通 MCP 会话不能借用领域作用域访问 GUI 工具。
    pub fn authorize_gui_workspace(
        &self,
        project_id: &str,
        path: &str,
        workspace_token: &str,
    ) -> Result<(), String> {
        validate_gui_relative_path(path)?;
        self.get_project(project_id)?;
        let directory = self.gui_workspace_directory(project_id)?;
        let _lock = lock_workspace(&directory, path)?;
        authorize_workspace_locked(&directory, project_id, path, workspace_token)
    }

    fn gui_workspace_directory(&self, project_id: &str) -> Result<PathBuf, String> {
        Ok(self.project_dir(project_id)?.join(GUI_WORKSPACE_DIRECTORY))
    }
}

fn apply_operation(
    snapshot: &GuiWorkspaceSnapshot,
    operation: &GuiWorkspaceOperation,
) -> Result<String, String> {
    match operation {
        GuiWorkspaceOperation::SetPosition { node_id, x, y } => {
            if !x.is_finite() || !y.is_finite() {
                return Err("GUI_WORKSPACE_NUMBER_INVALID: position must be finite".to_string());
            }
            let source = replace_bound_property(
                &snapshot.source,
                &snapshot.document,
                node_id,
                "x",
                &format_number(*x),
            )?;
            let document = parse_workspace_document(
                &source,
                &snapshot.path,
                snapshot.base_sha256.as_deref().unwrap_or_default(),
            )?;
            replace_bound_property(&source, &document, node_id, "y", &format_number(*y))
        }
        GuiWorkspaceOperation::SetProperty {
            node_id,
            property,
            value,
        } => {
            let node = snapshot
                .document
                .nodes
                .iter()
                .find(|node| &node.id == node_id)
                .ok_or_else(|| format!("GUI_NODE_NOT_FOUND: {node_id}"))?;
            let replacement = property_token(node, property, value)?;
            replace_bound_property(
                &snapshot.source,
                &snapshot.document,
                node_id,
                property,
                &replacement,
            )
        }
        GuiWorkspaceOperation::AddNode {
            parent_node_id,
            node_type,
            name,
            x,
            y,
        } => {
            if !x.is_finite() || !y.is_finite() {
                return Err("GUI_WORKSPACE_NUMBER_INVALID: position must be finite".to_string());
            }
            insert_core_node(
                &snapshot.source,
                &snapshot.document,
                parent_node_id,
                &InsertCoreNodeRequest {
                    node_type: *node_type,
                    name: name.clone(),
                    x: *x,
                    y: *y,
                },
            )
        }
        GuiWorkspaceOperation::AddBehavior { node_id, behavior } => {
            insert_node_behavior(&snapshot.source, &snapshot.document, node_id, *behavior)
        }
    }
}

fn property_token(
    node: &mir3_ui::Mir3UiNode,
    property: &str,
    value: &Value,
) -> Result<String, String> {
    const NUMBERS: &[&str] = &[
        "x",
        "y",
        "width",
        "height",
        "anchorX",
        "anchorY",
        "fontSize",
        "opacity",
        "tag",
        "scaleX",
        "scaleY",
        "rotation",
        "skewX",
        "skewY",
        "scale9Left",
        "scale9Bottom",
        "scale9Right",
        "scale9Top",
        "direction",
        "gravity",
        "itemsMargin",
        "innerWidth",
        "innerHeight",
    ];
    const BOOLEANS: &[&str] = &[
        "visible",
        "ignoreContentAdaptWithSize",
        "clippingEnabled",
        "scale9Enabled",
    ];
    const STRINGS: &[&str] = &[
        "text",
        "image",
        "pressedImage",
        "disabledImage",
        "color",
        "name",
    ];
    if NUMBERS.contains(&property) {
        let number = value
            .as_f64()
            .filter(|number| number.is_finite())
            .ok_or_else(|| format!("GUI_PROPERTY_VALUE_INVALID: {property} requires a number"))?;
        return Ok(format_number(number));
    }
    if BOOLEANS.contains(&property) {
        return value
            .as_bool()
            .map(|value| if value { "true" } else { "false" }.to_string())
            .ok_or_else(|| format!("GUI_PROPERTY_VALUE_INVALID: {property} requires a boolean"));
    }
    if STRINGS.contains(&property) || node.asset_slots.contains_key(property) {
        return value
            .as_str()
            .map(quote_lua_string)
            .ok_or_else(|| format!("GUI_PROPERTY_VALUE_INVALID: {property} requires a string"));
    }
    match node.properties.get(property).map(|bound| &bound.value) {
        Some(Mir3UiPropertyValue::Boolean(_)) => value
            .as_bool()
            .map(|value| if value { "true" } else { "false" }.to_string())
            .ok_or_else(|| format!("GUI_PROPERTY_VALUE_INVALID: {property} requires a boolean")),
        Some(Mir3UiPropertyValue::Number(_)) => value
            .as_f64()
            .filter(|number| number.is_finite())
            .map(format_number)
            .ok_or_else(|| format!("GUI_PROPERTY_VALUE_INVALID: {property} requires a number")),
        Some(Mir3UiPropertyValue::String(_)) => value
            .as_str()
            .map(quote_lua_string)
            .ok_or_else(|| format!("GUI_PROPERTY_VALUE_INVALID: {property} requires a string")),
        Some(Mir3UiPropertyValue::Nil) if value.is_null() => Ok("nil".to_string()),
        Some(Mir3UiPropertyValue::RawLiteral { .. }) => Err(format!(
            "GUI_PROPERTY_RAW_LITERAL_FORBIDDEN: {property} cannot be changed by AI"
        )),
        _ => Err(format!("GUI_PROPERTY_UNSUPPORTED: {property}")),
    }
}

fn parse_workspace_document(
    source: &str,
    path: &str,
    base_sha256: &str,
) -> Result<Mir3UiDocument, String> {
    parse_document(source, path, base_sha256, "utf-8", detect_newline(source))
}

fn validate_gui_relative_path(path: &str) -> Result<(), String> {
    let candidate = Path::new(path);
    let valid_root = matches!(
        candidate.components().next(),
        Some(Component::Normal(root)) if root == "GUIExport" || root == "GUILayout"
    );
    let valid_components = candidate
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    if !valid_root || !valid_components || !path.to_ascii_lowercase().ends_with(".lua") {
        return Err(
            "GUI_WORKSPACE_PATH_INVALID: expected GUIExport/ or GUILayout/ Lua path".to_string(),
        );
    }
    Ok(())
}

fn validate_gui_write_path(path: &str) -> Result<(), String> {
    validate_gui_relative_path(path)?;
    if !matches!(Path::new(path).components().next(), Some(Component::Normal(root)) if root == "GUIExport")
    {
        return Err(
            "GUI_WORKSPACE_WRITE_PATH_DENIED: AI changes are limited to GUIExport".to_string(),
        );
    }
    Ok(())
}

fn validate_revision(revision: i64) -> Result<(), String> {
    if revision < 0 {
        return Err("GUI_WORKSPACE_REVISION_INVALID: revision must be non-negative".to_string());
    }
    Ok(())
}

fn validate_source(source: &str) -> Result<(), String> {
    if source.as_bytes().len() > MAX_GUI_WORKSPACE_SOURCE_BYTES {
        return Err(format!(
            "GUI_WORKSPACE_SOURCE_TOO_LARGE: maximum is {MAX_GUI_WORKSPACE_SOURCE_BYTES} bytes"
        ));
    }
    Ok(())
}

fn workspace_key(path: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(path.as_bytes());
    format!("{:x}", digest.finalize())
}

fn snapshot_path(directory: &Path, path: &str) -> PathBuf {
    directory.join(format!("{}.json", workspace_key(path)))
}

fn authorization_path(directory: &Path, path: &str) -> PathBuf {
    directory.join(format!("{}.auth.json", workspace_key(path)))
}

fn generate_workspace_token() -> Result<String, String> {
    let mut bytes = [0_u8; GUI_WORKSPACE_TOKEN_BYTES];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("GUI_WORKSPACE_TOKEN_GENERATE_FAILED: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn hash_token(token: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(token.as_bytes());
    format!("{:x}", digest.finalize())
}

fn authorize_workspace_locked(
    directory: &Path,
    project_id: &str,
    path: &str,
    workspace_token: &str,
) -> Result<(), String> {
    if workspace_token.len() < 32 {
        return Err(
            "GUI_WORKSPACE_TOKEN_REQUIRED: a valid workspace token is required".to_string(),
        );
    }
    let bytes = fs::read(authorization_path(directory, path))
        .map_err(|_| "GUI_WORKSPACE_TOKEN_INVALID: workspace token is unavailable".to_string())?;
    let authorization: GuiWorkspaceAuthorization = serde_json::from_slice(&bytes)
        .map_err(|error| format!("GUI_WORKSPACE_AUTH_INVALID: {error}"))?;
    if authorization.schema_version != GUI_WORKSPACE_SCHEMA_VERSION
        || authorization.project_id != project_id
        || authorization.path != path
        || authorization.token_hash != hash_token(workspace_token)
    {
        return Err("GUI_WORKSPACE_TOKEN_INVALID: token does not match the workspace".to_string());
    }
    Ok(())
}

fn lock_workspace(directory: &Path, path: &str) -> Result<File, String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("GUI_WORKSPACE_CREATE_FAILED: {error}"))?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(directory.join(format!("{}.lock", workspace_key(path))))
        .map_err(|error| format!("GUI_WORKSPACE_LOCK_FAILED: {error}"))?;
    file.lock_exclusive()
        .map_err(|error| format!("GUI_WORKSPACE_LOCK_FAILED: {error}"))?;
    Ok(file)
}

fn read_snapshot(
    directory: &Path,
    project_id: &str,
    path: &str,
) -> Result<GuiWorkspaceSnapshot, String> {
    let bytes = fs::read(snapshot_path(directory, path))
        .map_err(|error| format!("GUI_WORKSPACE_NOT_FOUND: {error}"))?;
    let snapshot: GuiWorkspaceSnapshot = serde_json::from_slice(&bytes)
        .map_err(|error| format!("GUI_WORKSPACE_INVALID: {error}"))?;
    if snapshot.schema_version != GUI_WORKSPACE_SCHEMA_VERSION
        || snapshot.project_id != project_id
        || snapshot.path != path
    {
        return Err(
            "GUI_WORKSPACE_IDENTITY_MISMATCH: workspace belongs to another context".to_string(),
        );
    }
    Ok(snapshot)
}

fn persist_snapshot(directory: &Path, snapshot: &GuiWorkspaceSnapshot) -> Result<(), String> {
    let target = snapshot_path(directory, &snapshot.path);
    let pending = directory.join(format!(
        ".{}.{}.pending",
        workspace_key(&snapshot.path),
        std::process::id()
    ));
    let bytes = serde_json::to_vec(snapshot)
        .map_err(|error| format!("GUI_WORKSPACE_SERIALIZE_FAILED: {error}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&pending)
        .map_err(|error| format!("GUI_WORKSPACE_WRITE_FAILED: {error}"))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("GUI_WORKSPACE_WRITE_FAILED: {error}"))?;
    drop(file);
    if target.exists() {
        fs::remove_file(&target)
            .map_err(|error| format!("GUI_WORKSPACE_REPLACE_FAILED: {error}"))?;
    }
    fs::rename(&pending, &target).map_err(|error| {
        let _ = fs::remove_file(&pending);
        format!("GUI_WORKSPACE_REPLACE_FAILED: {error}")
    })
}

fn persist_authorization(
    directory: &Path,
    authorization: &GuiWorkspaceAuthorization,
) -> Result<(), String> {
    let target = authorization_path(directory, &authorization.path);
    let bytes = serde_json::to_vec(authorization)
        .map_err(|error| format!("GUI_WORKSPACE_AUTH_SERIALIZE_FAILED: {error}"))?;
    fs::write(target, bytes).map_err(|error| format!("GUI_WORKSPACE_AUTH_WRITE_FAILED: {error}"))
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn quote_lua_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

fn detect_newline(source: &str) -> &'static str {
    if source.contains("\r\n") {
        "\r\n"
    } else if source.contains('\r') {
        "\r"
    } else {
        "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mir3Project;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static FIXTURE_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

    fn fixture() -> (DomainStore, Mir3Project, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "mir3-gui-workspace-{}-{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("客户端/dev/GUIExport")).unwrap();
        fs::create_dir_all(root.join("引擎")).unwrap();
        let data = root.join("data");
        let store = DomainStore::new_trusted_fixture(&data).unwrap();
        let project = store.import_project(&root).unwrap();
        (store, project, root)
    }

    #[test]
    fn operate_updates_only_private_workspace_and_checks_revision() {
        let (store, project, root) = fixture();
        let source = mir3_ui::generate_template("\n");
        let initial = store
            .sync_gui_workspace(
                &project.id,
                &GuiWorkspaceSyncRequest {
                    path: "GUIExport/Test.lua".to_string(),
                    working_revision: 2,
                    base_sha256: Some("base-sha".to_string()),
                    selected_node_id: None,
                    dirty: false,
                    source,
                },
            )
            .unwrap();
        let snapshot = store.get_gui_workspace(&project.id, &initial.path).unwrap();
        let node_id = snapshot.document.roots[0].clone();
        let result = store
            .operate_gui_workspace(
                &project.id,
                &GuiWorkspaceOperateRequest {
                    workspace_token: initial.workspace_token.clone(),
                    path: initial.path.clone(),
                    expected_revision: 2,
                    operation: GuiWorkspaceOperation::SetPosition {
                        node_id,
                        x: 15.0,
                        y: 25.0,
                    },
                },
            )
            .unwrap();
        assert_eq!(result.working_revision, 3);
        assert!(result.source.contains("15, 25"));
        assert!(!root.join("客户端/dev/GUIExport/Test.lua").exists());
        let error = store
            .operate_gui_workspace(
                &project.id,
                &GuiWorkspaceOperateRequest {
                    workspace_token: initial.workspace_token,
                    path: initial.path,
                    expected_revision: 2,
                    operation: GuiWorkspaceOperation::SetProperty {
                        node_id: "missing".to_string(),
                        property: "text".to_string(),
                        value: Value::String("no".to_string()),
                    },
                },
            )
            .unwrap_err();
        assert!(error.starts_with("GUI_WORKSPACE_REVISION_CONFLICT"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn operate_adds_core_node_and_behavior_in_private_workspace() {
        let (store, project, root) = fixture();
        let initial = store
            .sync_gui_workspace(
                &project.id,
                &GuiWorkspaceSyncRequest {
                    path: "GUIExport/Test.lua".to_string(),
                    working_revision: 0,
                    base_sha256: Some("base-sha".to_string()),
                    selected_node_id: None,
                    dirty: false,
                    source: mir3_ui::generate_template("\n"),
                },
            )
            .unwrap();
        let root_node = store
            .get_gui_workspace(&project.id, &initial.path)
            .unwrap()
            .document
            .roots[0]
            .clone();
        let added = store
            .operate_gui_workspace(
                &project.id,
                &GuiWorkspaceOperateRequest {
                    workspace_token: initial.workspace_token.clone(),
                    path: initial.path.clone(),
                    expected_revision: 0,
                    operation: GuiWorkspaceOperation::AddNode {
                        parent_node_id: root_node,
                        node_type: CoreNodeType::Text,
                        name: "Title".to_string(),
                        x: 12.0,
                        y: 34.0,
                    },
                },
            )
            .unwrap();
        assert!(added
            .source
            .contains("GUI:Text_Create(Scene, \"Title\", 12, 34"));
        let title_id = store
            .get_gui_workspace(&project.id, &initial.path)
            .unwrap()
            .document
            .nodes
            .into_iter()
            .find(|node| node.lua_variable == "Title")
            .unwrap()
            .id;
        let behavior = store
            .operate_gui_workspace(
                &project.id,
                &GuiWorkspaceOperateRequest {
                    workspace_token: initial.workspace_token,
                    path: initial.path,
                    expected_revision: 1,
                    operation: GuiWorkspaceOperation::AddBehavior {
                        node_id: title_id,
                        behavior: CoreBehaviorType::Timeline,
                    },
                },
            )
            .unwrap();
        assert!(behavior
            .source
            .contains("GUI:Timeline_FadeIn(Title, 0.3, nil)"));
        assert!(!root.join("客户端/dev/GUIExport/Test.lua").exists());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn workspace_token_is_required_and_gui_layout_is_read_only() {
        let (store, project, root) = fixture();
        let initial = store
            .sync_gui_workspace(
                &project.id,
                &GuiWorkspaceSyncRequest {
                    path: "GUILayout/Test.lua".to_string(),
                    working_revision: 0,
                    base_sha256: Some("base".to_string()),
                    selected_node_id: None,
                    dirty: false,
                    source: mir3_ui::generate_template("\n"),
                },
            )
            .unwrap();
        assert!(store
            .authorize_gui_workspace(&project.id, &initial.path, "invalid")
            .unwrap_err()
            .starts_with("GUI_WORKSPACE_TOKEN_REQUIRED"));
        let node_id = store
            .get_gui_workspace(&project.id, &initial.path)
            .unwrap()
            .document
            .roots[0]
            .clone();
        let error = store
            .operate_gui_workspace(
                &project.id,
                &GuiWorkspaceOperateRequest {
                    workspace_token: initial.workspace_token,
                    path: initial.path,
                    expected_revision: 0,
                    operation: GuiWorkspaceOperation::SetPosition {
                        node_id,
                        x: 1.0,
                        y: 2.0,
                    },
                },
            )
            .unwrap_err();
        assert!(error.starts_with("GUI_WORKSPACE_WRITE_PATH_DENIED"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rejects_paths_outside_gui_roots() {
        let (store, project, root) = fixture();
        let error = store
            .sync_gui_workspace(
                &project.id,
                &GuiWorkspaceSyncRequest {
                    path: "../GUIExport/Test.lua".to_string(),
                    working_revision: 0,
                    base_sha256: Some("base".to_string()),
                    selected_node_id: None,
                    dirty: false,
                    source: "return {}".to_string(),
                },
            )
            .unwrap_err();
        assert!(error.starts_with("GUI_WORKSPACE_PATH_INVALID"));
        fs::remove_dir_all(root).ok();
    }
}
