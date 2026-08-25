use mir3_ui::{
    parse_document, replace_bound_property, BoundValueSource, CompatibilityStatus, Mir3UiNodeType,
};

const SOURCE: &str = r##"local ui = {}
function ui.init(parent)
    local Scene = GUI:Node_Create(parent, "Scene", 0, 0)
    local Panel = GUI:Layout_Create(Scene, "Panel", 10, 20, 300, 200, false)
    GUI:setAnchorPoint(Panel, 0.5, 1)
    local Image = GUI:Image_Create(Panel, "Image", dynamicX, 40, "res/a.png")
    GUI:setContentSize(Image, 32, 48)
    local Text = GUI:Text_Create(Panel, "Text", 5, 6, 14, "#ffffff", [[你好]])
    GUI:setVisible(Text, false)
    local Button = GUI:Button_Create(Panel, "Button", 7, 8, "res/b.png")
    GUI:Button_loadTexturePressed(Button, "res/b_pressed.png")
    local Effect = GUI:Effect_Create(Panel, "Effect", 0, 0, 1)
end
return ui
"##;

#[test]
fn parses_core_nodes_bindings_assets_and_unsupported_nodes() {
    let document = parse_document(SOURCE, "GUIExport/example.lua", "abc", "utf-8", "\n").unwrap();
    assert_eq!(document.nodes.len(), 6);
    assert_eq!(document.roots.len(), 1);
    assert_eq!(document.nodes[1].node_type, Mir3UiNodeType::Panel);
    assert_eq!(document.nodes[1].size.width.value, 300.0);
    assert_eq!(document.nodes[1].anchor.x.value, 0.5);
    assert_eq!(
        document.nodes[2].position.x.source,
        BoundValueSource::Dynamic
    );
    assert!(!document.nodes[2].position.x.writable);
    assert_eq!(document.nodes[3].text.value, "你好");
    assert!(!document.nodes[3].visible.value);
    assert_eq!(document.nodes[4].pressed_image.value, "res/b_pressed.png");
    assert_eq!(
        document.nodes[5].compatibility.status,
        CompatibilityStatus::Approximate
    );
    assert_eq!(document.assets.len(), 3);
    assert!(document.nodes[1]
        .source_binding
        .property_spans
        .contains_key("width"));
}

#[test]
fn setter_targets_most_recent_reused_variable() {
    let source = r#"local A = GUI:Image_Create(parent, "one", 1, 2, "a.png")
A = GUI:Image_Create(parent, "two", 3, 4, "b.png")
GUI:setContentSize(A, 90, 80)
"#;
    let document = parse_document(source, "GUIExport/reuse.lua", "sha", "utf-8", "\n").unwrap();
    assert_eq!(document.nodes.len(), 2);
    assert_eq!(document.nodes[0].size.width.value, 0.0);
    assert_eq!(document.nodes[1].size.width.value, 90.0);
}

#[test]
fn parent_links_form_the_same_nested_tree_as_the_lua_variables() {
    let source = r##"local Root = GUI:Node_Create(parent, "Root", 0, 0)
local Panel = GUI:Layout_Create(Root, "Panel", 10, 20, 300, 200, false)
GUI:setAnchorPoint(Panel, 0.5, 1)
local Image = GUI:Image_Create(Panel, "Image", 30, 40, "res/a.png")
local Missing = GUI:Text_Create(dynamicParent, "Missing", 1, 2, 14, "#fff", "text")
"##;
    let document = parse_document(source, "GUIExport/tree.lua", "sha", "utf-8", "\n").unwrap();
    let root = &document.nodes[0];
    let panel = &document.nodes[1];
    let image = &document.nodes[2];
    let missing = &document.nodes[3];

    assert_eq!(document.roots, vec![root.id.clone(), missing.id.clone()]);
    assert_eq!(root.children, vec![panel.id.clone()]);
    assert_eq!(panel.parent_id.as_deref(), Some(root.id.as_str()));
    assert_eq!(panel.children, vec![image.id.clone()]);
    assert_eq!(image.parent_id.as_deref(), Some(panel.id.as_str()));
    assert_eq!(panel.position.x.value, 10.0);
    assert_eq!(panel.position.y.value, 20.0);
    assert_eq!(panel.anchor.x.value, 0.5);
    assert_eq!(panel.anchor.y.value, 1.0);
    assert_eq!(panel.size.width.value, 300.0);
    assert_eq!(panel.size.height.value, 200.0);
    assert_eq!(missing.compatibility.status, CompatibilityStatus::Dynamic);
    assert_eq!(
        missing.compatibility.reason_code.as_deref(),
        Some("unresolved_parent")
    );
}

#[test]
fn literal_setters_clear_only_the_overridden_dynamic_properties() {
    let source = r#"local Image = GUI:Image_Create(parent, "Image", runtimeX, 2, runtimeImage)
GUI:setPosition(Image, 10, 20)
GUI:Image_loadTexture(Image, "res/final.png")
"#;
    let document = parse_document(source, "GUIExport/dynamic.lua", "sha", "utf-8", "\n").unwrap();
    let image = &document.nodes[0];

    assert_eq!(image.position.x.source, BoundValueSource::Literal);
    assert_eq!(image.position.y.source, BoundValueSource::Literal);
    assert_eq!(image.image.source, BoundValueSource::Literal);
    assert_eq!(image.image.value, "res/final.png");
    assert_eq!(image.compatibility.status, CompatibilityStatus::Supported);
    assert_eq!(image.compatibility.reason_code, None);
    assert!(!document
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "GUI_DYNAMIC_PROPERTY"));
}

#[test]
fn dynamic_setter_locks_only_its_property_and_unknown_create_never_panics() {
    let source = r#"local Button = GUI:Button_Create(parent, "Button", 1, 2, "res/normal.png")
GUI:Button_loadTexturePressed(Button, pressedTexture)
local Future = GUI:FutureWidget_Create(Button, dynamicName, runtimeX, 4, { any = value })
"#;
    let document = parse_document(source, "GUIExport/unknown.lua", "sha", "utf-8", "\n").unwrap();
    let button = &document.nodes[0];
    let future = &document.nodes[1];

    assert_eq!(button.position.x.source, BoundValueSource::Literal);
    assert!(button.position.x.writable);
    assert_eq!(
        button.asset_slots["normal"].source,
        BoundValueSource::Literal
    );
    assert_eq!(
        button.asset_slots["pressed"].source,
        BoundValueSource::Dynamic
    );
    assert!(!button.asset_slots["pressed"].writable);
    assert_eq!(button.compatibility.status, CompatibilityStatus::Dynamic);
    assert_eq!(future.node_type, Mir3UiNodeType::Unsupported);
    assert_eq!(future.compatibility.status, CompatibilityStatus::Unknown);
    assert_eq!(future.parent_id.as_deref(), Some(button.id.as_str()));
}

#[test]
fn malformed_lua_returns_document_with_diagnostic() {
    let document = parse_document(
        "local A = GUI:Image_Create(parent, \"A\", 1,",
        "GUIExport/bad.lua",
        "sha",
        "utf-8",
        "\n",
    )
    .unwrap();
    assert!(document
        .diagnostics
        .iter()
        .any(|item| item.code == "GUI_LUA_SYNTAX_ERROR"));
}

#[test]
fn replaces_only_bound_literal_token() {
    let document = parse_document(SOURCE, "GUIExport/example.lua", "abc", "utf-8", "\n").unwrap();
    let panel = &document.nodes[1];
    let output = replace_bound_property(SOURCE, &document, &panel.id, "x", "99").unwrap();
    assert!(output.contains("GUI:Layout_Create(Scene, \"Panel\", 99, 20, 300, 200, false)"));
    assert!(output.contains("[[你好]]"));
}

#[test]
fn document_contract_round_trips_as_camel_case_json() {
    let document = parse_document(SOURCE, "GUIExport/example.lua", "abc", "utf-8", "\n").unwrap();
    let json = serde_json::to_string(&document).unwrap();
    assert!(json.contains("\"schemaVersion\""));
    assert!(json.contains("\"sourceBinding\""));
    let decoded: mir3_ui::Mir3UiDocument = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.nodes.len(), document.nodes.len());
}
