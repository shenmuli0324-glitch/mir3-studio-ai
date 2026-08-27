use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MIR3_UI_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourcePoint {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start: SourcePoint,
    pub end: SourcePoint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3UiSource {
    pub dev_relative_path: String,
    pub sha256: String,
    pub encoding: String,
    pub newline: String,
    pub byte_length: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BoundValueSource {
    Literal,
    Default,
    Dynamic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoundValue<T> {
    pub value: T,
    pub source: BoundValueSource,
    pub writable: bool,
    pub original_token: Option<String>,
    pub span: Option<SourceSpan>,
}

impl<T> BoundValue<T> {
    pub fn literal(value: T, token: String, span: SourceSpan) -> Self {
        Self {
            value,
            source: BoundValueSource::Literal,
            writable: true,
            original_token: Some(token),
            span: Some(span),
        }
    }

    pub fn dynamic(value: T, token: String, span: SourceSpan) -> Self {
        Self {
            value,
            source: BoundValueSource::Dynamic,
            writable: false,
            original_token: Some(token),
            span: Some(span),
        }
    }

    pub fn default(value: T) -> Self {
        Self {
            value,
            source: BoundValueSource::Default,
            writable: false,
            original_token: None,
            span: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3UiPoint {
    pub x: BoundValue<f64>,
    pub y: BoundValue<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3UiSize {
    pub width: BoundValue<f64>,
    pub height: BoundValue<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Mir3UiNodeType {
    Panel,
    Image,
    Button,
    Text,
    TextAtlas,
    RichText,
    ScrollText,
    Node,
    ItemShow,
    CheckBox,
    TextInput,
    Slider,
    ProgressTimer,
    LoadingBar,
    Effect,
    #[serde(rename = "UIModel")]
    UiModel,
    SpineAnim,
    PageView,
    ListView,
    ScrollView,
    QuickCell,
    MenuItem,
    TableView,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CompatibilityStatus {
    Supported,
    Approximate,
    Dynamic,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3UiCompatibility {
    pub status: CompatibilityStatus,
    pub reason_code: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceBinding {
    pub create_call: SourceSpan,
    pub statement: SourceSpan,
    pub property_spans: BTreeMap<String, SourceSpan>,
    pub insert_byte: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3UiTransform {
    pub scale_x: BoundValue<f64>,
    pub scale_y: BoundValue<f64>,
    pub rotation: BoundValue<f64>,
    pub skew_x: BoundValue<f64>,
    pub skew_y: BoundValue<f64>,
}

impl Default for Mir3UiTransform {
    fn default() -> Self {
        Self {
            scale_x: BoundValue::default(1.0),
            scale_y: BoundValue::default(1.0),
            rotation: BoundValue::default(0.0),
            skew_x: BoundValue::default(0.0),
            skew_y: BoundValue::default(0.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3UiScale9 {
    pub enabled: BoundValue<bool>,
    pub left: BoundValue<f64>,
    pub bottom: BoundValue<f64>,
    pub right: BoundValue<f64>,
    pub top: BoundValue<f64>,
}

impl Default for Mir3UiScale9 {
    fn default() -> Self {
        Self {
            enabled: BoundValue::default(false),
            left: BoundValue::default(0.0),
            bottom: BoundValue::default(0.0),
            right: BoundValue::default(0.0),
            top: BoundValue::default(0.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3UiContainer {
    pub direction: BoundValue<f64>,
    pub gravity: BoundValue<f64>,
    pub items_margin: BoundValue<f64>,
    pub inner_width: BoundValue<f64>,
    pub inner_height: BoundValue<f64>,
}

impl Default for Mir3UiContainer {
    fn default() -> Self {
        Self {
            direction: BoundValue::default(1.0),
            gravity: BoundValue::default(0.0),
            items_margin: BoundValue::default(0.0),
            inner_width: BoundValue::default(0.0),
            inner_height: BoundValue::default(0.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Mir3UiPropertyValue {
    Boolean(bool),
    Number(f64),
    String(String),
    RawLiteral {
        #[serde(rename = "luaLiteral")]
        lua_literal: String,
    },
    Nil,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3UiNode {
    pub id: String,
    pub node_type: Mir3UiNodeType,
    pub parent_id: Option<String>,
    pub children: Vec<String>,
    pub lua_variable: String,
    pub name: BoundValue<String>,
    pub position: Mir3UiPoint,
    pub size: Mir3UiSize,
    pub anchor: Mir3UiPoint,
    pub visible: BoundValue<bool>,
    pub text: BoundValue<String>,
    pub image: BoundValue<String>,
    pub pressed_image: BoundValue<String>,
    pub disabled_image: BoundValue<String>,
    #[serde(default)]
    pub asset_slots: BTreeMap<String, BoundValue<String>>,
    pub font_size: BoundValue<f64>,
    pub color: BoundValue<String>,
    pub opacity: BoundValue<f64>,
    pub tag: BoundValue<f64>,
    #[serde(default)]
    pub transform: Mir3UiTransform,
    #[serde(default = "default_true_bound_value")]
    pub ignore_content_adapt_with_size: BoundValue<bool>,
    #[serde(default = "default_false_bound_value")]
    pub clipping_enabled: BoundValue<bool>,
    #[serde(default)]
    pub scale9: Mir3UiScale9,
    #[serde(default)]
    pub container: Mir3UiContainer,
    #[serde(default)]
    pub properties: BTreeMap<String, BoundValue<Mir3UiPropertyValue>>,
    pub compatibility: Mir3UiCompatibility,
    pub source_binding: SourceBinding,
}

fn default_true_bound_value() -> BoundValue<bool> {
    BoundValue::default(true)
}

fn default_false_bound_value() -> BoundValue<bool> {
    BoundValue::default(false)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3UiAsset {
    pub logical_path: String,
    pub node_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3UiDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub span: Option<SourceSpan>,
    pub node_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3UiViewport {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mir3UiDocument {
    pub schema_version: u32,
    pub source: Mir3UiSource,
    pub viewport: Mir3UiViewport,
    pub roots: Vec<String>,
    pub nodes: Vec<Mir3UiNode>,
    pub assets: Vec<Mir3UiAsset>,
    pub diagnostics: Vec<Mir3UiDiagnostic>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CoreNodeType {
    Panel,
    Image,
    Text,
    Button,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CoreBehaviorType {
    Timeline,
    Action,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InsertCoreNodeRequest {
    pub node_type: CoreNodeType,
    pub name: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceEdit {
    pub span: SourceSpan,
    pub replacement: String,
}
