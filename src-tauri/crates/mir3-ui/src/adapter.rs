use crate::Mir3UiNodeType;
use serde::{Deserialize, Serialize};

pub const WIDGET_ADAPTER_REGISTRY_VERSION: u32 = 2;
pub const ASSET_SLOT_MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AdapterPropertyKind {
    Number,
    String,
    Boolean,
    Asset,
    Any,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AdapterAssetKind {
    Image,
    Atlas,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterAssetSetterBinding {
    pub method: &'static str,
    pub argument_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterAssetSlotBinding {
    pub slot: &'static str,
    pub property: &'static str,
    pub kind: AdapterAssetKind,
    pub primary: bool,
    pub setters: &'static [AdapterAssetSetterBinding],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterPropertyBinding {
    pub index: usize,
    pub property: &'static str,
    pub kind: AdapterPropertyKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetAdapter {
    pub create_method: &'static str,
    pub node_type: Mir3UiNodeType,
    pub approximate: bool,
    pub bindings: &'static [AdapterPropertyBinding],
    pub asset_slots: &'static [AdapterAssetSlotBinding],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetAdapterRegistry {
    pub version: u32,
    pub adapters: Vec<WidgetAdapter>,
}

impl WidgetAdapterRegistry {
    pub fn find(&self, create_method: &str) -> Option<&WidgetAdapter> {
        self.adapters
            .iter()
            .find(|adapter| adapter.create_method == create_method)
    }

    pub fn find_by_node_type(&self, node_type: Mir3UiNodeType) -> Option<&WidgetAdapter> {
        self.adapters
            .iter()
            .find(|adapter| adapter.node_type == node_type)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WidgetAdapterManifest {
    pub registry_version: u32,
    pub asset_slot_version: u32,
    pub adapters: Vec<WidgetAdapterManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WidgetAdapterManifestEntry {
    pub create_method: String,
    pub node_type: Mir3UiNodeType,
    pub approximate: bool,
    pub properties: Vec<AdapterPropertyManifestEntry>,
    pub asset_slots: Vec<AdapterAssetSlotManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdapterPropertyManifestEntry {
    pub index: usize,
    pub property: String,
    pub kind: AdapterPropertyKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdapterAssetSlotManifestEntry {
    pub slot: String,
    pub property: String,
    pub kind: AdapterAssetKind,
    pub primary: bool,
    pub setters: Vec<AdapterAssetSetterManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdapterAssetSetterManifestEntry {
    pub method: String,
    pub argument_index: usize,
}

const LAYOUT: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    number(4, "width"),
    number(5, "height"),
    boolean(6, "clippingEnabled"),
];
const XY_IMAGE: &[AdapterPropertyBinding] = &[number(2, "x"), number(3, "y"), asset(4, "image")];
const TEXT: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    number(4, "fontSize"),
    string(5, "color"),
    string(6, "text"),
];
const TEXT_ATLAS: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    string(4, "text"),
    asset(5, "atlasImage"),
    number(6, "itemWidth"),
    number(7, "itemHeight"),
    string(8, "startCharacter"),
];
const CHECK_BOX: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    asset(4, "image"),
    asset(5, "selectedImage"),
];
const TEXT_INPUT: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    number(4, "width"),
    number(5, "height"),
    number(6, "fontSize"),
];
const SLIDER: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    asset(4, "image"),
    asset(5, "progressImage"),
    asset(6, "thumbImage"),
];
const LOADING_BAR: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    asset(4, "progressImage"),
    number(5, "direction"),
];
const CONTAINER: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    number(4, "width"),
    number(5, "height"),
    number(6, "direction"),
];
const EFFECT: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    number(4, "effectType"),
    number(5, "effectId"),
];
const NODE: &[AdapterPropertyBinding] = &[number(2, "x"), number(3, "y")];
const RICH_TEXT: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    string(4, "text"),
    number(5, "width"),
    number(6, "fontSize"),
    string(7, "color"),
    number(8, "verticalSpace"),
    any(9, "hyperlinkCallback"),
    string(10, "defaultFontFace"),
];
const SCROLL_TEXT: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    number(4, "width"),
    number(5, "fontSize"),
    string(6, "color"),
    string(7, "text"),
];
const SET_DATA: &[AdapterPropertyBinding] = &[number(2, "x"), number(3, "y"), any(4, "setData")];
const UI_MODEL: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    number(4, "sex"),
    number(5, "feature"),
    number(6, "scale"),
];
const SPINE_ANIM: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    asset(4, "jsonPath"),
    asset(5, "atlasPath"),
    number(6, "trackIndex"),
    string(7, "animationName"),
    boolean(8, "loop"),
];
const TABLE_VIEW: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    number(4, "width"),
    number(5, "height"),
    number(6, "direction"),
    number(7, "cellWidth"),
    number(8, "cellHeight"),
    number(9, "itemCount"),
];
const QUICK_CELL: &[AdapterPropertyBinding] = &[
    number(2, "x"),
    number(3, "y"),
    number(4, "width"),
    number(5, "height"),
    any(6, "createCell"),
];

const NO_ASSET_SLOTS: &[AdapterAssetSlotBinding] = &[];
const IMAGE_ASSET_SLOTS: &[AdapterAssetSlotBinding] = &[asset_slot(
    "normal",
    "image",
    AdapterAssetKind::Image,
    true,
    &[asset_setter("Image_loadTexture", 1)],
)];
const LAYOUT_ASSET_SLOTS: &[AdapterAssetSlotBinding] = &[asset_slot(
    "background",
    "backgroundImage",
    AdapterAssetKind::Image,
    true,
    &[asset_setter("Layout_setBackGroundImage", 1)],
)];
const BUTTON_ASSET_SLOTS: &[AdapterAssetSlotBinding] = &[
    asset_slot(
        "normal",
        "image",
        AdapterAssetKind::Image,
        true,
        &[asset_setter("Button_loadTextureNormal", 1)],
    ),
    asset_slot(
        "pressed",
        "pressedImage",
        AdapterAssetKind::Image,
        false,
        &[asset_setter("Button_loadTexturePressed", 1)],
    ),
    asset_slot(
        "disabled",
        "disabledImage",
        AdapterAssetKind::Image,
        false,
        &[asset_setter("Button_loadTextureDisabled", 1)],
    ),
];
const TEXT_ATLAS_ASSET_SLOTS: &[AdapterAssetSlotBinding] = &[asset_slot(
    "atlas",
    "atlasImage",
    AdapterAssetKind::Atlas,
    true,
    &[],
)];
const CHECK_BOX_ASSET_SLOTS: &[AdapterAssetSlotBinding] = &[
    asset_slot(
        "normal",
        "image",
        AdapterAssetKind::Image,
        true,
        &[asset_setter("CheckBox_loadTextureBackGround", 1)],
    ),
    asset_slot(
        "selected",
        "selectedImage",
        AdapterAssetKind::Image,
        false,
        &[asset_setter("CheckBox_loadTextureFrontCross", 1)],
    ),
];
const SLIDER_ASSET_SLOTS: &[AdapterAssetSlotBinding] = &[
    asset_slot(
        "background",
        "image",
        AdapterAssetKind::Image,
        true,
        &[asset_setter("Slider_loadBarTexture", 1)],
    ),
    asset_slot(
        "progress",
        "progressImage",
        AdapterAssetKind::Image,
        false,
        &[asset_setter("Slider_loadProgressBarTexture", 1)],
    ),
    asset_slot(
        "thumb",
        "thumbImage",
        AdapterAssetKind::Image,
        false,
        &[
            asset_setter("Slider_loadSlidBallTextureNormal", 1),
            asset_setter("Slider_loadSlidBallTexturePressed", 1),
            asset_setter("Slider_loadSlidBallTextureDisabled", 1),
        ],
    ),
];
const PROGRESS_TIMER_ASSET_SLOTS: &[AdapterAssetSlotBinding] = &[asset_slot(
    "normal",
    "image",
    AdapterAssetKind::Image,
    true,
    &[],
)];
const LOADING_BAR_ASSET_SLOTS: &[AdapterAssetSlotBinding] = &[asset_slot(
    "progress",
    "progressImage",
    AdapterAssetKind::Image,
    true,
    &[asset_setter("LoadingBar_loadTexture", 1)],
)];
const SPINE_ASSET_SLOTS: &[AdapterAssetSlotBinding] = &[
    asset_slot("json", "jsonPath", AdapterAssetKind::Json, true, &[]),
    asset_slot("atlas", "atlasPath", AdapterAssetKind::Atlas, false, &[]),
];
const LIST_VIEW_ASSET_SLOTS: &[AdapterAssetSlotBinding] = &[asset_slot(
    "background",
    "backgroundImage",
    AdapterAssetKind::Image,
    true,
    &[asset_setter("ListView_setBackGroundImage", 1)],
)];
const SCROLL_VIEW_ASSET_SLOTS: &[AdapterAssetSlotBinding] = &[asset_slot(
    "background",
    "backgroundImage",
    AdapterAssetKind::Image,
    true,
    &[asset_setter("ScrollView_setBackGroundImage", 1)],
)];

/// 返回 V0.2 的 996 控件适配器表；调用方可按 version 固定解析行为。
pub fn widget_adapter_registry() -> WidgetAdapterRegistry {
    WidgetAdapterRegistry {
        version: WIDGET_ADAPTER_REGISTRY_VERSION,
        adapters: vec![
            adapter_with_assets(
                "Layout_Create",
                Mir3UiNodeType::Panel,
                false,
                LAYOUT,
                LAYOUT_ASSET_SLOTS,
            ),
            adapter_with_assets(
                "Image_Create",
                Mir3UiNodeType::Image,
                false,
                XY_IMAGE,
                IMAGE_ASSET_SLOTS,
            ),
            adapter_with_assets(
                "Button_Create",
                Mir3UiNodeType::Button,
                false,
                XY_IMAGE,
                BUTTON_ASSET_SLOTS,
            ),
            adapter("Text_Create", Mir3UiNodeType::Text, false, TEXT),
            adapter_with_assets(
                "TextAtlas_Create",
                Mir3UiNodeType::TextAtlas,
                false,
                TEXT_ATLAS,
                TEXT_ATLAS_ASSET_SLOTS,
            ),
            adapter(
                "RichText_Create",
                Mir3UiNodeType::RichText,
                false,
                RICH_TEXT,
            ),
            adapter(
                "ScrollText_Create",
                Mir3UiNodeType::ScrollText,
                false,
                SCROLL_TEXT,
            ),
            adapter("Node_Create", Mir3UiNodeType::Node, false, NODE),
            adapter("ItemShow_Create", Mir3UiNodeType::ItemShow, true, SET_DATA),
            adapter_with_assets(
                "CheckBox_Create",
                Mir3UiNodeType::CheckBox,
                false,
                CHECK_BOX,
                CHECK_BOX_ASSET_SLOTS,
            ),
            adapter(
                "TextInput_Create",
                Mir3UiNodeType::TextInput,
                false,
                TEXT_INPUT,
            ),
            adapter_with_assets(
                "Slider_Create",
                Mir3UiNodeType::Slider,
                false,
                SLIDER,
                SLIDER_ASSET_SLOTS,
            ),
            adapter_with_assets(
                "ProgressTimer_Create",
                Mir3UiNodeType::ProgressTimer,
                false,
                XY_IMAGE,
                PROGRESS_TIMER_ASSET_SLOTS,
            ),
            adapter_with_assets(
                "LoadingBar_Create",
                Mir3UiNodeType::LoadingBar,
                false,
                LOADING_BAR,
                LOADING_BAR_ASSET_SLOTS,
            ),
            adapter("Effect_Create", Mir3UiNodeType::Effect, true, EFFECT),
            adapter("UIModel_Create", Mir3UiNodeType::UiModel, true, UI_MODEL),
            adapter_with_assets(
                "SpineAnim_Create",
                Mir3UiNodeType::SpineAnim,
                true,
                SPINE_ANIM,
                SPINE_ASSET_SLOTS,
            ),
            adapter(
                "PageView_Create",
                Mir3UiNodeType::PageView,
                false,
                CONTAINER,
            ),
            adapter_with_assets(
                "ListView_Create",
                Mir3UiNodeType::ListView,
                false,
                CONTAINER,
                LIST_VIEW_ASSET_SLOTS,
            ),
            adapter_with_assets(
                "ScrollView_Create",
                Mir3UiNodeType::ScrollView,
                false,
                CONTAINER,
                SCROLL_VIEW_ASSET_SLOTS,
            ),
            adapter(
                "QuickCell_Create",
                Mir3UiNodeType::QuickCell,
                true,
                QUICK_CELL,
            ),
            adapter("MenuItem_Create", Mir3UiNodeType::MenuItem, false, SET_DATA),
            adapter(
                "TableView_Create",
                Mir3UiNodeType::TableView,
                false,
                TABLE_VIEW,
            ),
        ],
    }
}

/// 生成不携带静态生命周期的前端清单，便于通过 IPC 直接序列化。
pub fn widget_adapter_manifest() -> WidgetAdapterManifest {
    let registry = widget_adapter_registry();
    WidgetAdapterManifest {
        registry_version: registry.version,
        asset_slot_version: ASSET_SLOT_MANIFEST_VERSION,
        adapters: registry
            .adapters
            .into_iter()
            .map(|adapter| WidgetAdapterManifestEntry {
                create_method: adapter.create_method.to_string(),
                node_type: adapter.node_type,
                approximate: adapter.approximate,
                properties: adapter
                    .bindings
                    .iter()
                    .map(|binding| AdapterPropertyManifestEntry {
                        index: binding.index,
                        property: binding.property.to_string(),
                        kind: binding.kind,
                    })
                    .collect(),
                asset_slots: adapter
                    .asset_slots
                    .iter()
                    .map(|slot| AdapterAssetSlotManifestEntry {
                        slot: slot.slot.to_string(),
                        property: slot.property.to_string(),
                        kind: slot.kind,
                        primary: slot.primary,
                        setters: slot
                            .setters
                            .iter()
                            .map(|setter| AdapterAssetSetterManifestEntry {
                                method: setter.method.to_string(),
                                argument_index: setter.argument_index,
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

const fn adapter(
    create_method: &'static str,
    node_type: Mir3UiNodeType,
    approximate: bool,
    bindings: &'static [AdapterPropertyBinding],
) -> WidgetAdapter {
    WidgetAdapter {
        create_method,
        node_type,
        approximate,
        bindings,
        asset_slots: NO_ASSET_SLOTS,
    }
}

const fn adapter_with_assets(
    create_method: &'static str,
    node_type: Mir3UiNodeType,
    approximate: bool,
    bindings: &'static [AdapterPropertyBinding],
    asset_slots: &'static [AdapterAssetSlotBinding],
) -> WidgetAdapter {
    WidgetAdapter {
        create_method,
        node_type,
        approximate,
        bindings,
        asset_slots,
    }
}

const fn asset_slot(
    slot: &'static str,
    property: &'static str,
    kind: AdapterAssetKind,
    primary: bool,
    setters: &'static [AdapterAssetSetterBinding],
) -> AdapterAssetSlotBinding {
    AdapterAssetSlotBinding {
        slot,
        property,
        kind,
        primary,
        setters,
    }
}

const fn asset_setter(method: &'static str, argument_index: usize) -> AdapterAssetSetterBinding {
    AdapterAssetSetterBinding {
        method,
        argument_index,
    }
}

const fn number(index: usize, property: &'static str) -> AdapterPropertyBinding {
    AdapterPropertyBinding {
        index,
        property,
        kind: AdapterPropertyKind::Number,
    }
}

const fn string(index: usize, property: &'static str) -> AdapterPropertyBinding {
    AdapterPropertyBinding {
        index,
        property,
        kind: AdapterPropertyKind::String,
    }
}

const fn boolean(index: usize, property: &'static str) -> AdapterPropertyBinding {
    AdapterPropertyBinding {
        index,
        property,
        kind: AdapterPropertyKind::Boolean,
    }
}

const fn asset(index: usize, property: &'static str) -> AdapterPropertyBinding {
    AdapterPropertyBinding {
        index,
        property,
        kind: AdapterPropertyKind::Asset,
    }
}

const fn any(index: usize, property: &'static str) -> AdapterPropertyBinding {
    AdapterPropertyBinding {
        index,
        property,
        kind: AdapterPropertyKind::Any,
    }
}
