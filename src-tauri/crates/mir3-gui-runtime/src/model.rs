use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;
pub const PROTOCOL_NAME: &str = "mir3-gui-runtime";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRequest {
    pub protocol_version: u32,
    pub request_id: String,
    #[serde(flatten)]
    pub operation: RuntimeOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "camelCase")]
pub enum RuntimeOperation {
    Catalog(CatalogRequest),
    Start(StartRequest),
    Event(EventRequest),
    Reload(ReloadRequest),
    Stop(StopRequest),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRequest {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartRequest {
    pub scene_id: String,
    pub layout_path: String,
    #[serde(default)]
    pub modules: BTreeMap<String, String>,
    #[serde(default)]
    pub device: DeviceKind,
    #[serde(default)]
    pub viewport: Viewport,
    #[serde(default)]
    pub data_profile: DataProfileSnapshot,
    #[serde(default)]
    pub limits: Option<RuntimeLimits>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EventRequest {
    pub session_id: String,
    pub name: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReloadRequest {
    pub session_id: String,
    pub layout_path: String,
    #[serde(default)]
    pub modules: BTreeMap<String, String>,
    #[serde(default)]
    pub data_profile: Option<DataProfileSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StopRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeResponse {
    pub protocol_version: u32,
    pub request_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<RuntimeResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RuntimeError>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<RuntimeDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum RuntimeResult {
    Catalog(CatalogResult),
    Scene(SceneResult),
    Stopped(StopResult),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogResult {
    pub protocol_name: String,
    pub protocol_version: u32,
    pub scenes: Vec<SceneCatalogEntry>,
    pub capabilities: RuntimeCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneCatalogEntry {
    pub id: String,
    pub title: String,
    pub description: String,
    pub recommended_device: DeviceKind,
    pub entry_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCapabilities {
    pub virtual_modules: bool,
    pub data_profile_snapshot: bool,
    pub event_dispatch: bool,
    pub filesystem: bool,
    pub network: bool,
    pub lua_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneResult {
    pub session_id: String,
    pub sequence: u64,
    pub scene: RuntimeScene,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<RuntimeDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StopResult {
    pub session_id: String,
    pub stopped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeScene {
    pub id: String,
    pub profile_id: String,
    pub viewport: Viewport,
    pub roots: Vec<String>,
    pub nodes: BTreeMap<String, RuntimeNode>,
    pub diagnostics: Vec<RuntimeDiagnostic>,
    pub provenance: Vec<DataProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeNode {
    pub id: String,
    pub node_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub children: Vec<String>,
    pub transform: RuntimeTransform,
    pub size: RuntimeSize,
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub asset_slots: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<SourceRef>,
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeTransform {
    pub x: f64,
    pub y: f64,
    pub anchor_x: f64,
    pub anchor_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
}

impl Default for RuntimeTransform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            anchor_x: 0.0,
            anchor_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub dev_relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<SourceRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<DataProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DataProvenance {
    pub kind: DataProvenanceKind,
    pub key: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DataProvenanceKind {
    StaticConfig,
    SceneMock,
    RuntimeDerived,
    Missing,
    UserSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProfile {
    pub kind: DeviceKind,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

impl Default for DeviceProfile {
    fn default() -> Self {
        Self {
            kind: DeviceKind::Mobile,
            width: 1136,
            height: 640,
            scale_factor: 1.0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    #[default]
    Mobile,
    Pc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_scale_factor")]
    pub scale_factor: f64,
}

fn default_scale_factor() -> f64 {
    1.0
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 1136,
            height: 640,
            scale_factor: 1.0,
        }
    }
}

impl From<&DeviceProfile> for Viewport {
    fn from(value: &DeviceProfile) -> Self {
        Self {
            width: value.width,
            height: value.height,
            scale_factor: value.scale_factor,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DataProfileSnapshot {
    #[serde(default)]
    pub origin: String,
    #[serde(default)]
    pub profile_id: String,
    #[serde(default)]
    pub virtual_clock: i64,
    #[serde(default)]
    pub tables: BTreeMap<String, Value>,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub meta_values: BTreeMap<String, Value>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub source_hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub redactions: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeLimits {
    pub max_memory_bytes: usize,
    pub max_instructions: u64,
    pub max_nodes: usize,
    pub max_modules: usize,
    pub max_source_bytes: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 64 * 1024 * 1024,
            max_instructions: 5_000_000,
            max_nodes: 10_000,
            max_modules: 512,
            max_source_bytes: 16 * 1024 * 1024,
        }
    }
}

impl RuntimeLimits {
    pub fn sandboxed(self) -> Self {
        Self {
            max_memory_bytes: self.max_memory_bytes.clamp(1024 * 1024, 64 * 1024 * 1024),
            max_instructions: self.max_instructions.clamp(1_000, 5_000_000),
            max_nodes: self.max_nodes.clamp(1, 10_000),
            max_modules: self.max_modules.clamp(1, 512),
            max_source_bytes: self.max_source_bytes.clamp(1024, 16 * 1024 * 1024),
        }
    }
}
