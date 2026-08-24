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
        CompatibilityStatus::Unsupported
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
