use std::collections::BTreeMap;

use serde_json::json;

use crate::{
    execute_json_line, execute_request, CatalogRequest, DataProfileSnapshot, DeviceKind,
    EventRequest, ReloadRequest, RuntimeLimits, RuntimeOperation, RuntimeRequest, RuntimeResult,
    RuntimeServer, StartRequest, StopRequest, Viewport, PROTOCOL_VERSION,
};

fn request(request_id: &str, operation: RuntimeOperation) -> RuntimeRequest {
    RuntimeRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: request_id.to_string(),
        operation,
    }
}

fn start_request(source: &str) -> StartRequest {
    let layout_path = "GUILayout/test.lua".to_string();
    StartRequest {
        scene_id: "auction".to_string(),
        layout_path: layout_path.clone(),
        modules: BTreeMap::from([(layout_path, source.to_string())]),
        device: DeviceKind::Mobile,
        viewport: Viewport::default(),
        data_profile: DataProfileSnapshot::default(),
        limits: None,
    }
}

#[test]
fn catalog_exposes_six_safe_profiles() {
    let mut server = RuntimeServer::new();
    let response = execute_request(
        &mut server,
        request("catalog-1", RuntimeOperation::Catalog(CatalogRequest {})),
    );
    assert!(response.ok);
    let Some(RuntimeResult::Catalog(catalog)) = response.result else {
        panic!("目录响应类型错误");
    };
    assert_eq!(catalog.scenes.len(), 6);
    assert!(!catalog.capabilities.filesystem);
    assert!(!catalog.capabilities.network);
    assert_eq!(catalog.capabilities.lua_version, "Lua 5.1 (vendored)");
}

#[test]
fn custom_scene_ids_execute_without_catalog_registration() {
    let mut start = start_request("return GUI:Node_Create(parent, 'Custom', 0, 0)");
    start.scene_id = "project:custom/scene".to_string();
    let mut server = RuntimeServer::new();
    let response = execute_request(
        &mut server,
        request("custom-1", RuntimeOperation::Start(start)),
    );
    assert!(response.ok, "{:?}", response.error);
}

#[test]
fn jsonl_protocol_uses_frozen_envelope() {
    let mut server = RuntimeServer::new();
    let response = execute_json_line(
        &mut server,
        r#"{"protocolVersion":1,"requestId":"json-1","type":"catalog","payload":{}}"#,
    );
    assert!(response.ok);
    assert_eq!(response.request_id, "json-1");
    let serialized = serde_json::to_value(response).expect("响应应可序列化");
    assert_eq!(serialized["protocolVersion"], 1);
    assert_eq!(serialized["requestId"], "json-1");
}

#[test]
fn tauri_start_payload_accepts_viewport_without_scale_and_profile_metadata() {
    let mut server = RuntimeServer::new();
    let response = execute_json_line(
        &mut server,
        r#"{"protocolVersion":1,"requestId":"wire-start","type":"start","payload":{"sceneId":"project:sample","layoutPath":"GUILayout/sample.lua","device":"mobile","viewport":{"width":1136,"height":640},"modules":{"GUILayout/sample.lua":"return GUI:Node_Create(parent, 'Root', 0, 0)"},"dataProfile":{"origin":"builtInMock","profileId":"default","values":{},"tables":{},"sourceHashes":{},"redactions":[]}}}"#,
    );
    assert!(response.ok, "{:?}", response.error);
    let Some(RuntimeResult::Scene(result)) = response.result else {
        panic!("应返回场景结果");
    };
    assert_eq!(result.scene.viewport.scale_factor, 1.0);
}

#[test]
fn minimal_gui_and_snapshot_compatibility_build_scene() {
    let source = r#"
assert(io == nil and os == nil and package == nil and debug == nil)
local root = GUI:Layout_Create(parent, "Root", 10, 20, 300, 200, false)
local label = GUI:Text_Create(root, "Label", 30, 40, 18, SL:GetValue("title"))
GUI:setPosition(label, 44, 55)
GUI:setContentSize(label, 120, 28)
GUI:Text_setString(label, "新标题")
return root
"#;
    let mut start = start_request(source);
    start
        .data_profile
        .values
        .insert("title".to_string(), json!("快照标题"));
    let mut server = RuntimeServer::new();
    let response = execute_request(
        &mut server,
        request("start-1", RuntimeOperation::Start(start)),
    );
    assert!(response.ok, "{:?}", response.error);
    let Some(RuntimeResult::Scene(result)) = response.result else {
        panic!("场景响应类型错误");
    };
    assert_eq!(result.sequence, 1);
    assert_eq!(result.scene.nodes.len(), 3);
    let label = result
        .scene
        .nodes
        .values()
        .find(|node| node.name == "Label")
        .expect("应创建文本节点");
    assert_eq!(label.transform.x, 44.0);
    assert_eq!(label.transform.y, 55.0);
    assert_eq!(label.size.width, 120.0);
    assert_eq!(label.text.as_deref(), Some("新标题"));
}

#[test]
fn runtime_keeps_each_widget_asset_slot_without_overwriting_primary() {
    let source = r#"
local button = GUI:Button_Create(parent, "Submit", 1, 2, "res/normal.png")
GUI:Button_loadTexturePressed(button, "res/pressed.png")
GUI:Button_loadTextureDisabled(button, "res/disabled.png")
local slider = GUI:Slider_Create(parent, "Volume", 3, 4, "res/bar.png", "res/progress.png", "res/thumb.png")
return button
"#;
    let mut server = RuntimeServer::new();
    let response = execute_request(
        &mut server,
        request(
            "asset-slots",
            RuntimeOperation::Start(start_request(source)),
        ),
    );
    assert!(response.ok, "{:?}", response.error);
    let Some(RuntimeResult::Scene(result)) = response.result else {
        panic!("应返回场景");
    };
    let button = result
        .scene
        .nodes
        .values()
        .find(|node| node.name == "Submit")
        .expect("应创建按钮");
    assert_eq!(button.asset.as_deref(), Some("res/normal.png"));
    assert_eq!(
        button.asset_slots,
        BTreeMap::from([
            ("disabled".to_string(), "res/disabled.png".to_string()),
            ("normal".to_string(), "res/normal.png".to_string()),
            ("pressed".to_string(), "res/pressed.png".to_string()),
        ])
    );
    let slider = result
        .scene
        .nodes
        .values()
        .find(|node| node.name == "Volume")
        .expect("应创建滑块");
    assert_eq!(slider.asset.as_deref(), Some("res/bar.png"));
    assert_eq!(
        slider.asset_slots.get("progress").map(String::as_str),
        Some("res/progress.png")
    );
    assert_eq!(
        slider.asset_slots.get("thumb").map(String::as_str),
        Some("res/thumb.png")
    );
}

#[test]
fn virtual_require_cannot_read_filesystem() {
    let source = r#"
local child = require("GUIExport.child")
return child(parent)
"#;
    let mut start = start_request(source);
    start.modules.insert(
        "GUIExport/child.lua".to_string(),
        "return function(parent) return GUI:Button_Create(parent, 'Child', 1, 2, 'res/a.png') end"
            .to_string(),
    );
    let mut server = RuntimeServer::new();
    let response = execute_request(
        &mut server,
        request("require-1", RuntimeOperation::Start(start)),
    );
    assert!(response.ok, "{:?}", response.error);
    let Some(RuntimeResult::Scene(result)) = response.result else {
        panic!("场景响应类型错误");
    };
    assert!(result.scene.nodes.values().any(|node| node.name == "Child"));
}

#[test]
fn project_static_tables_are_available_only_as_snapshot_modules() {
    let source = r#"
local config = SL:Require("game_config/cfg_item")
assert(config.rows[1].name == "测试物品")
assert(SL:GetMetaValue("currency") == 88)
return GUI:Text_Create(parent, "Item", 0, 0, 16, config.rows[1].name)
"#;
    let mut start = start_request(source);
    start.data_profile.origin = "projectStatic".to_string();
    start.data_profile.profile_id = "auction".to_string();
    start.data_profile.tables.insert(
        "cfg_item".to_string(),
        json!({"rows": [{"id": 1, "name": "测试物品"}]}),
    );
    start
        .data_profile
        .values
        .insert("currency".to_string(), json!(88));
    start
        .data_profile
        .source_hashes
        .insert("cfg_item".to_string(), "abc123".to_string());
    start.data_profile.redactions = vec!["playerState".to_string()];
    let encoded = serde_json::to_string(&request(
        "snapshot-roundtrip",
        RuntimeOperation::Start(start.clone()),
    ))
    .expect("快照请求应可序列化");
    let decoded: RuntimeRequest = serde_json::from_str(&encoded).expect("快照请求应可反序列化");
    let RuntimeOperation::Start(decoded_start) = decoded.operation else {
        panic!("请求类型应保持不变");
    };
    assert_eq!(decoded_start.data_profile, start.data_profile);

    let mut server = RuntimeServer::new();
    let response = execute_request(
        &mut server,
        request("snapshot-run", RuntimeOperation::Start(start)),
    );
    assert!(response.ok, "{:?}", response.error);
    let Some(RuntimeResult::Scene(result)) = response.result else {
        panic!("应返回场景");
    };
    assert!(result
        .scene
        .nodes
        .values()
        .any(|node| node.text.as_deref() == Some("测试物品")));
}

#[test]
fn minimal_guilayout_loads_export_and_builds_delegate() {
    let layout_path = "GUILayout/sample/SampleMain.lua".to_string();
    let layout = r#"
SampleMain = {}
function SampleMain.main()
    local layer = GUI:Win_Create(UIConst.LAYERID.SampleMain, 0, 0, 0, 0, false)
    GUI:LoadExport(layer, "sample/sample_main")
    local ui = GUI:ui_delegate(layer)
    GUI:setPosition(ui.Frame, 100, 120)
end
SampleMain.main()
"#;
    let export = r#"
local ui = {}
function ui.init(parent)
    local frame = GUI:Layout_Create(parent, "Frame", 0, 0, 320, 240, false)
    GUI:Image_Create(frame, "Background", 0, 0, "res/sample.png")
    return frame
end
return ui
"#;
    let start = StartRequest {
        scene_id: "project:sample-main".to_string(),
        layout_path: layout_path.clone(),
        modules: BTreeMap::from([
            (layout_path, layout.to_string()),
            (
                "GUIExport/sample/sample_main.lua".to_string(),
                export.to_string(),
            ),
        ]),
        device: DeviceKind::Mobile,
        viewport: Viewport::default(),
        data_profile: DataProfileSnapshot::default(),
        limits: None,
    };
    let mut server = RuntimeServer::new();
    let response = execute_request(
        &mut server,
        request("layout-run", RuntimeOperation::Start(start)),
    );
    assert!(response.ok, "{:?}", response.error);
    let Some(RuntimeResult::Scene(result)) = response.result else {
        panic!("应返回场景");
    };
    let frame = result
        .scene
        .nodes
        .values()
        .find(|node| node.name == "Frame")
        .expect("GUILayout 应通过 LoadExport 生成 Frame");
    assert_eq!((frame.transform.x, frame.transform.y), (100.0, 120.0));
    assert_eq!(
        frame
            .source_ref
            .as_ref()
            .map(|source| source.dev_relative_path.as_str()),
        Some("GUIExport/sample/sample_main.lua")
    );
    assert!(result
        .scene
        .nodes
        .values()
        .any(|node| node.name == "Background"));
}

#[test]
fn denied_service_api_returns_nil_and_diagnostic() {
    let source = r#"
local value = SL:RequestPlayerBag()
assert(value == nil)
local root = GUI:Node_Create(parent, "Root", 0, 0)
assert(GUI:UnknownMutation(root, 1) == nil)
return root
"#;
    let mut server = RuntimeServer::new();
    let response = execute_request(
        &mut server,
        request("denied-1", RuntimeOperation::Start(start_request(source))),
    );
    assert!(response.ok, "{:?}", response.error);
    let Some(RuntimeResult::Scene(result)) = response.result else {
        panic!("场景响应类型错误");
    };
    assert!(
        result
            .scene
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "RUNTIME_API_DENIED")
            .count()
            >= 2
    );
}

#[test]
fn instruction_and_node_budgets_stop_unsafe_sources() {
    let mut loop_start = start_request("while true do end");
    loop_start.limits = Some(RuntimeLimits {
        max_instructions: 2_000,
        ..RuntimeLimits::default()
    });
    let mut server = RuntimeServer::new();
    let response = execute_request(
        &mut server,
        request("loop-1", RuntimeOperation::Start(loop_start)),
    );
    assert!(!response.ok);
    assert!(response
        .error
        .as_ref()
        .is_some_and(|error| error.message.contains("RUNTIME_INSTRUCTION_LIMIT")));

    let mut node_start =
        start_request("for i = 1, 4 do GUI:Node_Create(parent, 'Node' .. i, 0, 0) end");
    node_start.limits = Some(RuntimeLimits {
        max_nodes: 3,
        ..RuntimeLimits::default()
    });
    let response = execute_request(
        &mut server,
        request("nodes-1", RuntimeOperation::Start(node_start)),
    );
    assert!(!response.ok);
    assert!(response
        .error
        .as_ref()
        .is_some_and(|error| error.message.contains("RUNTIME_NODE_LIMIT")));
}

#[test]
fn event_reload_and_stop_advance_session_safely() {
    let mut server = RuntimeServer::new();
    let start = execute_request(
        &mut server,
        request(
            "start-flow",
            RuntimeOperation::Start(start_request(
                "return GUI:Node_Create(parent, 'First', 0, 0)",
            )),
        ),
    );
    let Some(RuntimeResult::Scene(started)) = start.result else {
        panic!("应启动运行时会话");
    };
    let session_id = started.session_id;

    let event = execute_request(
        &mut server,
        request(
            "event-flow",
            RuntimeOperation::Event(EventRequest {
                session_id: session_id.clone(),
                name: "refresh".to_string(),
                payload: json!({"page": 2}),
            }),
        ),
    );
    let Some(RuntimeResult::Scene(event_result)) = event.result else {
        panic!("事件应返回新场景");
    };
    assert_eq!(event_result.sequence, 2);

    let layout_path = "GUILayout/reloaded.lua".to_string();
    let reload = execute_request(
        &mut server,
        request(
            "reload-flow",
            RuntimeOperation::Reload(ReloadRequest {
                session_id: session_id.clone(),
                layout_path: layout_path.clone(),
                modules: BTreeMap::from([(
                    layout_path,
                    "return GUI:Text_Create(parent, 'Reloaded', 1, 2, 18, 'ok')".to_string(),
                )]),
                data_profile: None,
            }),
        ),
    );
    let Some(RuntimeResult::Scene(reload_result)) = reload.result else {
        panic!("重载应返回新场景");
    };
    assert_eq!(reload_result.sequence, 3);
    assert!(reload_result
        .scene
        .nodes
        .values()
        .any(|node| node.name == "Reloaded"));

    let stopped = execute_request(
        &mut server,
        request(
            "stop-flow",
            RuntimeOperation::Stop(StopRequest {
                session_id: session_id.clone(),
            }),
        ),
    );
    let Some(RuntimeResult::Stopped(result)) = stopped.result else {
        panic!("停止应返回状态");
    };
    assert!(result.stopped);
}
