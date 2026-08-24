use crate::Mir3UiNodeType;

pub const WIDGET_ADAPTER_REGISTRY_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterPropertyKind {
    Number,
    String,
    Boolean,
    Asset,
    Any,
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
    asset(5, "image"),
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
    asset(4, "image"),
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

/// 返回 V0.2 的 996 控件适配器表；调用方可按 version 固定解析行为。
pub fn widget_adapter_registry() -> WidgetAdapterRegistry {
    WidgetAdapterRegistry {
        version: WIDGET_ADAPTER_REGISTRY_VERSION,
        adapters: vec![
            adapter("Layout_Create", Mir3UiNodeType::Panel, false, LAYOUT),
            adapter("Image_Create", Mir3UiNodeType::Image, false, XY_IMAGE),
            adapter("Button_Create", Mir3UiNodeType::Button, false, XY_IMAGE),
            adapter("Text_Create", Mir3UiNodeType::Text, false, TEXT),
            adapter(
                "TextAtlas_Create",
                Mir3UiNodeType::TextAtlas,
                false,
                TEXT_ATLAS,
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
            adapter(
                "CheckBox_Create",
                Mir3UiNodeType::CheckBox,
                false,
                CHECK_BOX,
            ),
            adapter(
                "TextInput_Create",
                Mir3UiNodeType::TextInput,
                false,
                TEXT_INPUT,
            ),
            adapter("Slider_Create", Mir3UiNodeType::Slider, false, SLIDER),
            adapter(
                "ProgressTimer_Create",
                Mir3UiNodeType::ProgressTimer,
                false,
                XY_IMAGE,
            ),
            adapter(
                "LoadingBar_Create",
                Mir3UiNodeType::LoadingBar,
                false,
                LOADING_BAR,
            ),
            adapter("Effect_Create", Mir3UiNodeType::Effect, true, EFFECT),
            adapter("UIModel_Create", Mir3UiNodeType::UiModel, true, UI_MODEL),
            adapter(
                "SpineAnim_Create",
                Mir3UiNodeType::SpineAnim,
                true,
                SPINE_ANIM,
            ),
            adapter(
                "PageView_Create",
                Mir3UiNodeType::PageView,
                false,
                CONTAINER,
            ),
            adapter(
                "ListView_Create",
                Mir3UiNodeType::ListView,
                false,
                CONTAINER,
            ),
            adapter(
                "ScrollView_Create",
                Mir3UiNodeType::ScrollView,
                false,
                CONTAINER,
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
