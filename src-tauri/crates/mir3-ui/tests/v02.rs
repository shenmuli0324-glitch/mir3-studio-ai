use mir3_ui::{
    parse_document, replace_bound_property, widget_adapter_manifest, widget_adapter_registry,
    AdapterAssetKind, CompatibilityStatus, Mir3UiNodeType, Mir3UiPropertyValue,
    ASSET_SLOT_MANIFEST_VERSION, MIR3_UI_SCHEMA_VERSION, WIDGET_ADAPTER_REGISTRY_VERSION,
};
use std::collections::HashSet;

#[test]
fn registry_parses_all_twenty_three_widget_types() {
    let source = r##"function ui_init(parent)
local Node = GUI:Node_Create(parent, "Node", 0, 0)
local Panel = GUI:Layout_Create(Node, "Panel", 1, 2, 100, 80, false)
local Image = GUI:Image_Create(Panel, "Image", 1, 2, "res/image.png")
local Button = GUI:Button_Create(Panel, "Button", 1, 2, "res/button.png")
local Text = GUI:Text_Create(Panel, "Text", 1, 2, 14, "#ffffff", [[text]])
local TextAtlas = GUI:TextAtlas_Create(Panel, "TextAtlas", 1, 2, "0", "res/atlas.png", 10, 12, "0")
local RichText = GUI:RichText_Create(Panel, "RichText", 1, 2, [[rich]], 100, 18, "#ffffff", 4, "onLink", "Arial")
local ScrollText = GUI:ScrollText_Create(Panel, "ScrollText", 1, 2, 100, 18, "#ffffff", [[scroll]])
local ItemShow = GUI:ItemShow_Create(Panel, "ItemShow", 1, 2, 1001)
local CheckBox = GUI:CheckBox_Create(Panel, "CheckBox", 1, 2, "res/off.png", "res/on.png")
local TextInput = GUI:TextInput_Create(Panel, "TextInput", 1, 2, 100, 30, 14)
local Slider = GUI:Slider_Create(Panel, "Slider", 1, 2, "res/bg.png", "res/bar.png", "res/thumb.png")
local ProgressTimer = GUI:ProgressTimer_Create(Panel, "ProgressTimer", 1, 2, "res/progress.png")
local LoadingBar = GUI:LoadingBar_Create(Panel, "LoadingBar", 1, 2, "res/bar.png", 0)
local Effect = GUI:Effect_Create(Panel, "Effect", 1, 2, 0, 4413, 0, 0, 0, 1)
local UIModel = GUI:UIModel_Create(Panel, "UIModel", 1, 2, 1, 100, 0.8)
local SpineAnim = GUI:SpineAnim_Create(Panel, "SpineAnim", 1, 2, "res/a.json", "res/a.atlas", 0, "idle", true)
local PageView = GUI:PageView_Create(Panel, "PageView", 1, 2, 100, 80, 1)
local ListView = GUI:ListView_Create(Panel, "ListView", 1, 2, 100, 80, 1)
local ScrollView = GUI:ScrollView_Create(Panel, "ScrollView", 1, 2, 100, 80, 1)
local QuickCell = GUI:QuickCell_Create(Panel, "QuickCell", 1, 2, 100, 80, 1)
local MenuItem = GUI:MenuItem_Create(Panel, "MenuItem", 1, 2, 1002)
local TableView = GUI:TableView_Create(Panel, "TableView", 1, 2, 100, 80, 1, 20, 10, 5)
end
"##;
    let document = parse_document(source, "GUIExport/all.lua", "sha", "utf-8", "\n").unwrap();
    assert_eq!(MIR3_UI_SCHEMA_VERSION, 2);
    assert_eq!(
        widget_adapter_registry().version,
        WIDGET_ADAPTER_REGISTRY_VERSION
    );
    assert_eq!(widget_adapter_registry().adapters.len(), 23);
    assert_eq!(document.nodes.len(), 23);
    let types: HashSet<_> = document.nodes.iter().map(|node| node.node_type).collect();
    assert_eq!(types.len(), 23);
    assert!(!types.contains(&Mir3UiNodeType::Unsupported));
    for node in &document.nodes {
        assert_ne!(node.compatibility.status, CompatibilityStatus::Unknown);
    }
}

#[test]
fn adapter_manifest_is_versioned_and_frontend_serializable() {
    let manifest = widget_adapter_manifest();
    assert_eq!(manifest.registry_version, WIDGET_ADAPTER_REGISTRY_VERSION);
    assert_eq!(manifest.asset_slot_version, ASSET_SLOT_MANIFEST_VERSION);
    assert_eq!(manifest.adapters.len(), 23);
    let json = serde_json::to_string(&manifest).unwrap();
    let decoded = serde_json::from_str(&json).unwrap();
    assert_eq!(manifest, decoded);

    let button = manifest
        .adapters
        .iter()
        .find(|adapter| adapter.create_method == "Button_Create")
        .unwrap();
    assert_eq!(button.asset_slots.len(), 3);
    assert_eq!(button.asset_slots[0].slot, "normal");
    assert!(button.asset_slots[0].primary);
    assert_eq!(button.asset_slots[1].slot, "pressed");
    assert_eq!(button.asset_slots[2].slot, "disabled");

    let spine = manifest
        .adapters
        .iter()
        .find(|adapter| adapter.create_method == "SpineAnim_Create")
        .unwrap();
    assert_eq!(spine.asset_slots[0].kind, AdapterAssetKind::Json);
    assert_eq!(spine.asset_slots[1].kind, AdapterAssetKind::Atlas);
}

#[test]
fn asset_slots_follow_create_and_setter_bindings_without_rewriting_source() {
    let source = r#"local Panel = GUI:Layout_Create(parent, "Panel", 0, 0, 320, 200, false)
GUI:Layout_setBackGroundImage(Panel, "res/layout.png") -- keep
local Button = GUI:Button_Create(Panel, "Button", 1, 2, "res/normal.png")
GUI:Button_loadTexturePressed(Button, "res/pressed.png")
GUI:Button_loadTextureDisabled(Button, "res/disabled.png")
local Check = GUI:CheckBox_Create(Panel, "Check", 1, 2, "res/off.png", "res/on.png")
GUI:CheckBox_loadTextureFrontCross(Check, "res/on-new.png")
local Slider = GUI:Slider_Create(Panel, "Slider", 1, 2, "res/slider-bg.png", "res/slider-progress.png", "res/thumb.png")
GUI:Slider_loadProgressBarTexture(Slider, "res/slider-progress-new.png")
local Loading = GUI:LoadingBar_Create(Panel, "Loading", 1, 2, "res/loading.png", 0)
GUI:LoadingBar_loadTexture(Loading, "res/loading-new.png")
local Atlas = GUI:TextAtlas_Create(Panel, "Atlas", 1, 2, "0", "res/font.png", 8, 12, "0")
local Spine = GUI:SpineAnim_Create(Panel, "Spine", 1, 2, "res/a.json", "res/a.atlas", 0, "idle", true)
local List = GUI:ListView_Create(Panel, "List", 1, 2, 692, 293, 1)
GUI:ListView_setBackGroundImage(List, "res/list.png")
"#;
    let document = parse_document(source, "GUIExport/assets.lua", "sha", "utf-8", "\n").unwrap();
    let panel = &document.nodes[0];
    assert_eq!(panel.asset_slots["background"].value, "res/layout.png");
    assert_eq!(panel.image.value, "res/layout.png");
    assert!(panel
        .source_binding
        .property_spans
        .contains_key("backgroundImage"));

    let button = &document.nodes[1];
    assert_eq!(button.asset_slots["normal"].value, "res/normal.png");
    assert_eq!(button.asset_slots["pressed"].value, "res/pressed.png");
    assert_eq!(button.asset_slots["disabled"].value, "res/disabled.png");
    let check = &document.nodes[2];
    assert_eq!(check.asset_slots["selected"].value, "res/on-new.png");
    let slider = &document.nodes[3];
    assert_eq!(
        slider.asset_slots["progress"].value,
        "res/slider-progress-new.png"
    );
    let loading = &document.nodes[4];
    assert_eq!(loading.asset_slots["progress"].value, "res/loading-new.png");
    assert_eq!(document.nodes[5].asset_slots["atlas"].value, "res/font.png");
    assert_eq!(document.nodes[6].asset_slots["json"].value, "res/a.json");
    assert_eq!(document.nodes[6].asset_slots["atlas"].value, "res/a.atlas");
    let list = &document.nodes[7];
    assert_eq!(list.size.width.value, 692.0);
    assert_eq!(list.size.height.value, 293.0);
    assert_eq!(list.asset_slots["background"].value, "res/list.png");

    let patched = replace_bound_property(
        source,
        &document,
        &panel.id,
        "backgroundImage",
        "\"res/layout-new.png\"",
    )
    .unwrap();
    assert!(patched.contains("\"res/layout-new.png\") -- keep"));
    assert!(!patched.contains("\"res/layout.png\" -- keep"));
}

#[test]
fn list_view_binds_official_auction_bidding_size() {
    let source = r#"local ListView_items = GUI:ListView_Create(parent, "ListView_items", 359, 368, 692, 293, 1.0)"#;
    let document = parse_document(
        source,
        "GUIExport/auction/auction_bidding.lua",
        "sha",
        "utf-8",
        "\n",
    )
    .unwrap();
    let node = &document.nodes[0];
    assert_eq!(node.node_type, Mir3UiNodeType::ListView);
    assert_eq!(node.size.width.value, 692.0);
    assert_eq!(node.size.height.value, 293.0);
    assert_eq!(node.container.direction.value, 1.0);
    assert_eq!(node.compatibility.status, CompatibilityStatus::Supported);
    assert!(node.clipping_enabled.value);
}

#[test]
fn layout_create_clipping_flag_is_bound() {
    let source = r#"local Panel = GUI:Layout_Create(parent, "Panel", 0, 0, 320, 200, true)"#;
    let document = parse_document(source, "GUIExport/panel.lua", "sha", "utf-8", "\n").unwrap();
    assert!(document.nodes[0].clipping_enabled.value);
    assert!(document.nodes[0].clipping_enabled.writable);
}

#[test]
fn transform_scale9_container_and_input_setters_are_bound() {
    let source = r##"local List = GUI:ListView_Create(parent, "List", 1, 2, 100, 80, 1)
GUI:setScale(List, 1.25)
GUI:setScaleY(List, 0.75)
GUI:setRotation(List, 45)
GUI:setRotationSkewX(List, 3)
GUI:setRotationSkewY(List, 4)
GUI:setIgnoreContentAdaptWithSize(List, false)
GUI:ListView_setClippingEnabled(List, true)
GUI:Image_setScale9Slice(List, 1, 2, 3, 4)
GUI:ListView_setGravity(List, 2)
GUI:ListView_setItemsMargin(List, 6)
local Input = GUI:TextInput_Create(List, "Input", 1, 2, 120, 30, 16)
GUI:TextInput_setString(Input, [[hello]])
GUI:TextInput_setPlaceHolder(Input, [[type here]])
GUI:TextInput_setInputMode(Input, 3)
"##;
    let document = parse_document(source, "GUIExport/setters.lua", "sha", "utf-8", "\n").unwrap();
    let list = &document.nodes[0];
    assert_eq!(list.transform.scale_x.value, 1.25);
    assert_eq!(list.transform.scale_y.value, 0.75);
    assert_eq!(list.transform.rotation.value, 45.0);
    assert_eq!(list.transform.skew_x.value, 3.0);
    assert_eq!(list.transform.skew_y.value, 4.0);
    assert!(!list.ignore_content_adapt_with_size.value);
    assert!(list.clipping_enabled.value);
    assert_eq!(list.scale9.left.value, 1.0);
    assert_eq!(list.scale9.top.value, 4.0);
    assert_eq!(list.container.gravity.value, 2.0);
    assert_eq!(list.container.items_margin.value, 6.0);
    let input = &document.nodes[1];
    assert_eq!(input.text.value, "hello");
    assert_eq!(
        input.properties["placeholder"].value,
        Mir3UiPropertyValue::String("type here".to_string())
    );
}

#[test]
fn runtime_dynamic_and_external_types_have_distinct_compatibility() {
    let source = r#"local Effect = GUI:Effect_Create(parent, "Effect", 1, 2, 0, 4413)
local Dynamic = GUI:Image_Create(parent, "Dynamic", runtimeX, 2, "res/a.png")
local Future = GUI:FutureWidget_Create(parent, "Future", 1, 2, 3)
"#;
    let document = parse_document(source, "GUIExport/status.lua", "sha", "utf-8", "\n").unwrap();
    assert_eq!(
        document.nodes[0].compatibility.status,
        CompatibilityStatus::Approximate
    );
    assert_eq!(
        document.nodes[1].compatibility.status,
        CompatibilityStatus::Dynamic
    );
    assert_eq!(
        document.nodes[1].compatibility.reason_code.as_deref(),
        Some("dynamic_property")
    );
    assert_eq!(document.nodes[2].node_type, Mir3UiNodeType::Unsupported);
    assert_eq!(
        document.nodes[2].compatibility.status,
        CompatibilityStatus::Unknown
    );
}

#[test]
fn specialized_adapters_bind_every_official_argument() {
    let source = r##"local Rich = GUI:RichText_Create(parent, "Rich", 11, 12, [[rich text]], 301, 18, "#112233", 7, "onLink", "Microsoft YaHei")
local Scroll = GUI:ScrollText_Create(parent, "Scroll", 21, 22, 302, 19, "#223344", [[scroll text]])
local Item = GUI:ItemShow_Create(parent, "Item", 31, 32, 1001)
local Menu = GUI:MenuItem_Create(parent, "Menu", 41, 42, "menu-data")
local Model = GUI:UIModel_Create(parent, "Model", 51, 52, 1, 2024, 0.75)
local Spine = GUI:SpineAnim_Create(parent, "Spine", 61, 62, "res/spine/a.json", "res/spine/a.atlas", 2, "run", true)
local Table = GUI:TableView_Create(parent, "Table", 71, 72, 640, 360, 2, 80, 40, 12)
"##;
    let document = parse_document(source, "GUIExport/special.lua", "sha", "utf-8", "\n").unwrap();

    let rich = &document.nodes[0];
    assert_eq!(rich.text.value, "rich text");
    assert_eq!(rich.size.width.value, 301.0);
    assert_eq!(rich.size.height.value, 0.0);
    assert_eq!(rich.font_size.value, 18.0);
    assert_eq!(rich.color.value, "#112233");
    assert_property_number(rich, "verticalSpace", 7.0);
    assert_property_string(rich, "hyperlinkCallback", "onLink");
    assert_property_string(rich, "defaultFontFace", "Microsoft YaHei");

    let scroll = &document.nodes[1];
    assert_eq!(scroll.size.width.value, 302.0);
    assert_eq!(scroll.size.height.value, 0.0);
    assert_eq!(scroll.font_size.value, 19.0);
    assert_eq!(scroll.color.value, "#223344");
    assert_eq!(scroll.text.value, "scroll text");

    let item = &document.nodes[2];
    assert_eq!(item.image.value, "");
    assert_property_number(item, "setData", 1001.0);
    assert_eq!(item.compatibility.status, CompatibilityStatus::Approximate);

    let menu = &document.nodes[3];
    assert_eq!(menu.image.value, "");
    assert_property_string(menu, "setData", "menu-data");

    let model = &document.nodes[4];
    assert_property_number(model, "sex", 1.0);
    assert_property_number(model, "feature", 2024.0);
    assert_property_number(model, "scale", 0.75);
    assert_eq!(model.transform.scale_x.value, 0.75);
    assert_eq!(model.transform.scale_y.value, 0.75);

    let spine = &document.nodes[5];
    assert_property_string(spine, "jsonPath", "res/spine/a.json");
    assert_property_string(spine, "atlasPath", "res/spine/a.atlas");
    assert_property_number(spine, "trackIndex", 2.0);
    assert_property_string(spine, "animationName", "run");
    assert_eq!(
        spine.properties["loop"].value,
        Mir3UiPropertyValue::Boolean(true)
    );

    let table = &document.nodes[6];
    assert_eq!(table.size.width.value, 640.0);
    assert_eq!(table.size.height.value, 360.0);
    assert_eq!(table.container.direction.value, 2.0);
    assert_property_number(table, "cellWidth", 80.0);
    assert_property_number(table, "cellHeight", 40.0);
    assert_property_number(table, "itemCount", 12.0);
}

#[test]
fn static_set_data_table_remains_a_writable_raw_literal() {
    let source = r##"local Menu = GUI:MenuItem_Create(parent, "Menu", 1, 2, { itemname = "A", direction = 1 })"##;
    let document = parse_document(source, "GUIExport/menu.lua", "sha", "utf-8", "\n").unwrap();
    let value = &document.nodes[0].properties["setData"];
    assert!(value.writable);
    assert!(matches!(
        value.value,
        Mir3UiPropertyValue::RawLiteral { .. }
    ));
}

#[test]
fn quick_cell_callback_is_preserved_as_a_static_code_literal() {
    let source = r##"local Cell = GUI:QuickCell_Create(parent, "Cell", 1, 2, 100, 40, function(cellParent)
  return GUI:Layout_Create(cellParent, "root", 0, 0, 100, 40, false)
end)"##;
    let document = parse_document(source, "GUIExport/cell.lua", "sha", "utf-8", "\n").unwrap();
    let node = &document.nodes[0];
    assert_eq!(node.compatibility.status, CompatibilityStatus::Approximate);
    assert!(matches!(
        node.properties["createCell"].value,
        Mir3UiPropertyValue::RawLiteral { .. }
    ));
}

fn assert_property_number(node: &mir3_ui::Mir3UiNode, property: &str, expected: f64) {
    assert_eq!(
        node.properties[property].value,
        Mir3UiPropertyValue::Number(expected)
    );
}

fn assert_property_string(node: &mir3_ui::Mir3UiNode, property: &str, expected: &str) {
    assert_eq!(
        node.properties[property].value,
        Mir3UiPropertyValue::String(expected.to_string())
    );
}
