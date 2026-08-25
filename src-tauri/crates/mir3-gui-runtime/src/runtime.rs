use std::collections::{BTreeMap, HashSet};

use serde_json::{json, Value};

use crate::engine::RuntimeVm;
use crate::mocks;
use crate::model::{
    CatalogResult, DataProfileSnapshot, DataProvenance, DataProvenanceKind, DiagnosticSeverity,
    EventRequest, ReloadRequest, RuntimeCapabilities, RuntimeDiagnostic, RuntimeError,
    RuntimeOperation, RuntimeRequest, RuntimeResponse, RuntimeResult, RuntimeScene,
    RuntimeScenePatch, RuntimeWindowState, SceneResult, StartRequest, StopResult,
    LEGACY_PROTOCOL_VERSION, PROTOCOL_NAME, PROTOCOL_VERSION,
};

const CHARACTER_SELECT: &str = "character-select";
const CHARACTER_CREATE: &str = "character-create";
const GAME_MOBILE: &str = "game-mobile";
const GAME_PC: &str = "game-pc";

const GAME_STARTUP_CHAIN: &[&str] = &[
    "GUILayout/UIConst.lua",
    "GUILayout/GUIDefine.lua",
    "GUILayout/UIOperator.lua",
    "GUILayout/GUIFunction.lua",
    "GUILayout/GUIInit.lua",
];

const MOBILE_EXPORTS: &[&str] = &[
    "GUIExport/main/main_property.lua",
    "GUIExport/main/main_avartar.lua",
    "GUIExport/main/main_minimap.lua",
    "GUIExport/main/assist/assist.lua",
    "GUIExport/main/main_target.lua",
    "GUIExport/main/main_monster.lua",
    "GUIExport/be_strong_ui/be_strong_up.lua",
    "GUIExport/main/main_joy_stick.lua",
    "GUIExport/main/main_widgets.lua",
    "GUIExport/main/main_collect.lua",
    "GUIExport/main/skill/main_skill.lua",
];

const PC_EXPORTS: &[&str] = &[
    "GUIExport/main/main_minimap.lua",
    "GUIExport/main/main_buff_win32.lua",
    "GUIExport/main/main_pk_mode_win32.lua",
    "GUIExport/main/main_skill_shortcut_win32.lua",
    "GUIExport/main/main_item_shortcut_win32.lua",
    "GUIExport/main/main_skill_launch_win32.lua",
    "GUIExport/main/main_property_win32.lua",
    "GUIExport/main/main_chat_win32.lua",
    "GUIExport/main/assist/assist_win32.lua",
    "GUIExport/main/main_target.lua",
    "GUIExport/main/main_monster.lua",
    "GUIExport/be_strong_ui/be_strong_up.lua",
    "GUIExport/main/main_widgets_win32.lua",
    "GUIExport/main/main_collect.lua",
];

struct RuntimeSession {
    request: StartRequest,
    preset_id: String,
    sequence: u64,
    scene: RuntimeScene,
    vm: RuntimeVm,
    window_stack: Vec<RuntimeWindowState>,
    window_nodes: BTreeMap<String, HashSet<String>>,
    mock_state: BTreeMap<String, Value>,
}

#[derive(Default)]
pub struct RuntimeServer {
    sessions: BTreeMap<String, RuntimeSession>,
    next_session: u64,
}

impl RuntimeServer {
    pub fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
            next_session: 1,
        }
    }

    pub fn execute_request(&mut self, request: RuntimeRequest) -> RuntimeResponse {
        let response_version = request.protocol_version;
        let request_id = request.request_id;
        if !matches!(response_version, LEGACY_PROTOCOL_VERSION | PROTOCOL_VERSION) {
            return failure(
                response_version,
                request_id,
                "RUNTIME_PROTOCOL_VERSION",
                format!(
                    "仅支持协议版本 {LEGACY_PROTOCOL_VERSION} 和 {PROTOCOL_VERSION}，收到 {response_version}"
                ),
            );
        }
        match request.operation {
            RuntimeOperation::Catalog(_) => success(
                response_version,
                request_id,
                RuntimeResult::Catalog(CatalogResult {
                    protocol_name: PROTOCOL_NAME.to_string(),
                    protocol_version: PROTOCOL_VERSION,
                    scenes: mocks::catalog(),
                    capabilities: RuntimeCapabilities {
                        virtual_modules: true,
                        data_profile_snapshot: true,
                        event_dispatch: true,
                        filesystem: false,
                        network: false,
                        lua_version: "Lua 5.1 (vendored)".to_string(),
                        persistent_scene: true,
                        scene_patch: true,
                    },
                }),
            ),
            RuntimeOperation::Start(params) => self.start(response_version, request_id, params),
            RuntimeOperation::Event(params) => self.event(response_version, request_id, params),
            RuntimeOperation::Reload(params) => self.reload(response_version, request_id, params),
            RuntimeOperation::Stop(params) => {
                let stopped = self.sessions.remove(&params.session_id).is_some();
                success(
                    response_version,
                    request_id,
                    RuntimeResult::Stopped(StopResult {
                        session_id: params.session_id,
                        stopped,
                    }),
                )
            }
        }
    }

    fn start(
        &mut self,
        response_version: u32,
        request_id: String,
        request: StartRequest,
    ) -> RuntimeResponse {
        let session_id = format!("runtime-{}", self.next_session);
        self.next_session += 1;
        let preset_id = resolve_preset_id(&request);
        let window_stack = request
            .overlay_ids
            .iter()
            .filter_map(|kind| match kind.as_str() {
                "bag" | "team" | "store" => Some(window(kind)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut mock_state = BTreeMap::from([
            ("presetId".to_string(), json!(preset_id)),
            ("windowCount".to_string(), json!(window_stack.len())),
        ]);
        if let Some(value) = &request.map_id {
            mock_state.insert("mapId".to_string(), json!(value));
        }
        if let Some(value) = &request.mock_profile_id {
            mock_state.insert("mockProfileId".to_string(), json!(value));
        }
        if let Some(value) = &request.module_id {
            mock_state.insert("moduleId".to_string(), json!(value));
        }
        if let Some(value) = request.data_profile.values.get("previewDataMode") {
            mock_state.insert("dataMode".to_string(), value.clone());
        }
        match build_runtime(&request, &preset_id, &window_stack) {
            Ok((vm, window_nodes)) => {
                let scene = match vm.scene() {
                    Ok(scene) => scene,
                    Err(error) => return failure_from_string(response_version, request_id, error),
                };
                let result = scene_result(
                    session_id.clone(),
                    1,
                    scene.clone(),
                    None,
                    &preset_id,
                    &window_stack,
                    &mock_state,
                );
                self.sessions.insert(
                    session_id,
                    RuntimeSession {
                        request,
                        preset_id,
                        sequence: 1,
                        scene,
                        vm,
                        window_stack,
                        window_nodes,
                        mock_state,
                    },
                );
                success(response_version, request_id, RuntimeResult::Scene(result))
            }
            Err(error) => failure_from_string(response_version, request_id, error),
        }
    }

    fn event(
        &mut self,
        response_version: u32,
        request_id: String,
        event: EventRequest,
    ) -> RuntimeResponse {
        let Some(session) = self.sessions.get_mut(&event.session_id) else {
            return failure(
                response_version,
                request_id,
                "RUNTIME_SESSION_NOT_FOUND",
                format!("会话不存在：{}", event.session_id),
            );
        };
        let base_sequence = session.sequence;
        dispatch_lua_event(session, &event);
        let mut actions = Vec::new();
        if let Some(action) = resolve_event_action(&event, &session.scene) {
            actions.push(action);
        }
        match session.vm.take_window_signals() {
            Ok(signals) => {
                actions.extend(
                    signals
                        .into_iter()
                        .filter_map(|signal| signal_action(&signal)),
                );
            }
            Err(error) => {
                let _ = session
                    .vm
                    .push_diagnostic("RUNTIME_SIGNAL_READ_FAILED", error);
            }
        }
        actions.dedup();
        for action in actions {
            if let Err(error) = apply_action(session, &event, Some(&action)) {
                let _ = session.vm.push_diagnostic("RUNTIME_ACTION_FAILED", error);
            }
        }
        let next_sequence = base_sequence.saturating_add(1);
        match session.vm.scene() {
            Ok(mut scene) => {
                scene.provenance.push(DataProvenance {
                    kind: DataProvenanceKind::RuntimeDerived,
                    key: "event".to_string(),
                    description: "事件只修改沙箱窗口栈和脱机模拟状态".to_string(),
                });
                let patch = diff_scene(&session.scene, &scene, base_sequence, next_sequence);
                session.sequence = next_sequence;
                session.scene = scene.clone();
                let result = scene_result(
                    event.session_id,
                    next_sequence,
                    scene,
                    Some(patch),
                    &session.preset_id,
                    &session.window_stack,
                    &session.mock_state,
                );
                success(response_version, request_id, RuntimeResult::Scene(result))
            }
            Err(error) => failure_from_string(response_version, request_id, error),
        }
    }

    fn reload(
        &mut self,
        response_version: u32,
        request_id: String,
        reload: ReloadRequest,
    ) -> RuntimeResponse {
        let Some(session) = self.sessions.get_mut(&reload.session_id) else {
            return failure(
                response_version,
                request_id,
                "RUNTIME_SESSION_NOT_FOUND",
                format!("会话不存在：{}", reload.session_id),
            );
        };
        let mut next_request = session.request.clone();
        if !reload.layout_path.is_empty() {
            next_request.layout_path = reload.layout_path;
        }
        if !reload.modules.is_empty() {
            next_request.modules = reload.modules;
        }
        if let Some(data_profile) = reload.data_profile {
            next_request.data_profile = data_profile;
        }
        let base_sequence = session.sequence;
        let next_sequence = base_sequence.saturating_add(1);
        match build_runtime(&next_request, &session.preset_id, &session.window_stack) {
            Ok((vm, window_nodes)) => {
                let scene = match vm.scene() {
                    Ok(scene) => scene,
                    Err(error) => return failure_from_string(response_version, request_id, error),
                };
                let patch = diff_scene(&session.scene, &scene, base_sequence, next_sequence);
                session.request = next_request;
                session.sequence = next_sequence;
                session.scene = scene.clone();
                session.vm = vm;
                session.window_nodes = window_nodes;
                session
                    .mock_state
                    .insert("windowCount".to_string(), json!(session.window_stack.len()));
                let result = scene_result(
                    reload.session_id,
                    next_sequence,
                    scene,
                    Some(patch),
                    &session.preset_id,
                    &session.window_stack,
                    &session.mock_state,
                );
                success(response_version, request_id, RuntimeResult::Scene(result))
            }
            Err(error) => failure_from_string(response_version, request_id, error),
        }
    }
}

pub fn execute_request(server: &mut RuntimeServer, request: RuntimeRequest) -> RuntimeResponse {
    server.execute_request(request)
}

pub fn execute_json_line(server: &mut RuntimeServer, line: &str) -> RuntimeResponse {
    match serde_json::from_str::<RuntimeRequest>(line) {
        Ok(request) => execute_request(server, request),
        Err(error) => failure(
            PROTOCOL_VERSION,
            "unknown".to_string(),
            "RUNTIME_INVALID_REQUEST",
            error.to_string(),
        ),
    }
}

fn build_runtime(
    request: &StartRequest,
    preset_id: &str,
    windows: &[RuntimeWindowState],
) -> Result<(RuntimeVm, BTreeMap<String, HashSet<String>>), String> {
    let data_profile = runtime_data_profile(request);
    let mut vm = RuntimeVm::new(
        preset_id,
        request.viewport.clone(),
        &request.modules,
        &data_profile,
        request.limits.unwrap_or_default().sandboxed(),
    )?;
    let mut rendered = 0usize;
    if is_preset(preset_id) {
        if matches!(preset_id, GAME_MOBILE | GAME_PC) {
            let (active, diagnostics) = attempt_game_startup(&vm, request);
            if active {
                rendered = 1;
                vm.push_provenance(DataProvenance {
                    kind: DataProvenanceKind::RuntimeDerived,
                    key: "startupChain".to_string(),
                    description:
                        "场景由 UIConst/GUIDefine/UIOperator/GUIFunction/GUIInit 和 MAIN_INIT 生成"
                            .to_string(),
                })?;
            } else {
                // 启动链可能留下半成品节点和回调，回退前必须创建干净 VM。
                vm = RuntimeVm::new(
                    preset_id,
                    request.viewport.clone(),
                    &request.modules,
                    &data_profile,
                    request.limits.unwrap_or_default().sandboxed(),
                )?;
                for (code, message) in diagnostics {
                    vm.push_diagnostic(&code, message)?;
                }
                vm.push_provenance(DataProvenance {
                    kind: DataProvenanceKind::RuntimeDerived,
                    key: "staticCompositionFallback".to_string(),
                    description: "真实 GUI 启动链未完整生成画面，已使用安全 Export 组合回退"
                        .to_string(),
                })?;
            }
        }
        if rendered == 0 {
            for path in preset_source_paths(preset_id) {
                if !request.modules.contains_key(*path) {
                    continue;
                }
                match vm.execute_entry(path, None) {
                    Ok(()) => rendered += 1,
                    Err(error) => vm.push_diagnostic(
                        "RUNTIME_PRESET_MODULE_SKIPPED",
                        format!("组合场景模块 {path} 未能执行：{error}"),
                    )?,
                }
            }
        }
        if rendered == 0 {
            let source = mocks::source(preset_id).ok_or_else(|| {
                format!("RUNTIME_PRESET_SOURCE_MISSING: 预设 {preset_id} 没有可执行模块")
            })?;
            // 使用独立虚拟入口，避免 GUIInit 遮蔽安全预设源码。
            let fallback_path = format!("GUILayout/__runtime/{preset_id}.lua");
            vm.execute_entry(&fallback_path, Some(source))?;
        }
    } else {
        let fallback = if request.modules.contains_key(&request.layout_path) {
            None
        } else {
            mocks::source(&request.scene_id)
        };
        vm.execute_entry(&request.layout_path, fallback)?;
    }
    let mut window_nodes = BTreeMap::new();
    for window in windows {
        let ids = execute_window(&vm, request, window)?;
        window_nodes.insert(window.id.clone(), ids);
    }
    Ok((vm, window_nodes))
}

fn runtime_data_profile(request: &StartRequest) -> DataProfileSnapshot {
    let mut profile = request.data_profile.clone();
    profile.meta_values.insert(
        "IS_PC_OPER_MODE".to_string(),
        json!(matches!(request.device, crate::model::DeviceKind::Pc)),
    );
    profile
}

fn attempt_game_startup(vm: &RuntimeVm, request: &StartRequest) -> (bool, Vec<(String, String)>) {
    let before = vm.node_ids().unwrap_or_default();
    let mut diagnostics = Vec::new();
    let mut loaded = 0usize;
    let mut gui_init_loaded = false;
    for path in GAME_STARTUP_CHAIN {
        if !request.modules.contains_key(*path) {
            diagnostics.push((
                "RUNTIME_STARTUP_MODULE_MISSING".to_string(),
                format!("真实启动链缺少模块 {path}"),
            ));
            continue;
        }
        match vm.execute_entry(path, None) {
            Ok(()) => {
                loaded += 1;
                gui_init_loaded |= *path == "GUILayout/GUIInit.lua";
            }
            Err(error) => diagnostics.push((
                "RUNTIME_STARTUP_MODULE_FAILED".to_string(),
                format!("真实启动链模块 {path} 执行失败：{error}"),
            )),
        }
    }
    let callback_count = if gui_init_loaded {
        match vm.dispatch_registered_event("LUA_EVENT_MAIN_INIT", &json!({})) {
            Ok(count) => count,
            Err(error) => {
                diagnostics.push((
                    "RUNTIME_MAIN_INIT_FAILED".to_string(),
                    format!("LUA_EVENT_MAIN_INIT 执行失败：{error}"),
                ));
                0
            }
        }
    } else {
        0
    };
    let generated = vm
        .node_ids()
        .map(|after| after.difference(&before).count())
        .unwrap_or_default();
    if callback_count == 0 {
        diagnostics.push((
            "RUNTIME_MAIN_INIT_MISSING".to_string(),
            "GUIInit 未注册可执行的 LUA_EVENT_MAIN_INIT".to_string(),
        ));
    } else if generated == 0 {
        diagnostics.push((
            "RUNTIME_STARTUP_SCENE_EMPTY".to_string(),
            "真实启动链执行后没有生成可视节点".to_string(),
        ));
    }
    (
        loaded == GAME_STARTUP_CHAIN.len()
            && callback_count > 0
            && generated > 0
            && diagnostics.is_empty(),
        diagnostics,
    )
}

fn execute_window(
    vm: &RuntimeVm,
    request: &StartRequest,
    window: &RuntimeWindowState,
) -> Result<HashSet<String>, String> {
    let before = vm.node_ids()?;
    let mut rendered = 0usize;
    for path in &window.source_paths {
        if !request.modules.contains_key(path) {
            continue;
        }
        match vm.execute_entry(path, None) {
            Ok(()) => rendered += 1,
            Err(error) => vm.push_diagnostic(
                "RUNTIME_WINDOW_MODULE_SKIPPED",
                format!("窗口模块 {path} 未能执行：{error}"),
            )?,
        }
    }
    if rendered == 0 {
        let path = format!("GUILayout/__runtime/{}.lua", window.kind);
        if let Some(source) = window_fallback_source(&window.kind) {
            vm.execute_entry(&path, Some(source))?;
        }
    }
    let after = vm.node_ids()?;
    Ok(after.difference(&before).cloned().collect())
}

fn dispatch_lua_event(session: &mut RuntimeSession, event: &EventRequest) {
    let result = if matches!(event.name.as_str(), "click" | "node.click") {
        match event.payload.get("nodeId").and_then(Value::as_str) {
            Some(node_id) => session
                .vm
                .dispatch_click(node_id, &event.payload)
                .map(usize::from),
            None => Ok(0),
        }
    } else {
        session
            .vm
            .dispatch_registered_event(&event.name, &event.payload)
    };
    if let Err(error) = result {
        let _ = session.vm.push_diagnostic("RUNTIME_CALLBACK_FAILED", error);
    }
}

fn apply_action(
    session: &mut RuntimeSession,
    event: &EventRequest,
    action: Option<&str>,
) -> Result<(), String> {
    session
        .mock_state
        .insert("lastEvent".to_string(), json!(event.name));
    if let Some(node_id) = event.payload.get("nodeId").and_then(Value::as_str) {
        session
            .mock_state
            .insert("lastClickNodeId".to_string(), json!(node_id));
    }
    match action {
        Some("open-bag") => open_window(session, window("bag"))?,
        Some("open-team") => open_window(session, window("team"))?,
        Some("open-store") => open_window(session, window("store"))?,
        Some("close-top") => {
            if let Some(closed) = session.window_stack.pop() {
                if let Some(ids) = session.window_nodes.remove(&closed.id) {
                    session.vm.remove_nodes(&ids)?;
                }
            }
        }
        _ => {}
    }
    session
        .mock_state
        .insert("windowCount".to_string(), json!(session.window_stack.len()));
    if let Some(window) = session.window_stack.last() {
        session
            .mock_state
            .insert("topWindow".to_string(), json!(window.kind));
    } else {
        session.mock_state.remove("topWindow");
    }
    Ok(())
}

fn open_window(session: &mut RuntimeSession, window: RuntimeWindowState) -> Result<(), String> {
    if let Some(existing) = session
        .window_stack
        .iter()
        .find(|item| item.kind == window.kind)
        .cloned()
    {
        if let Some(ids) = session.window_nodes.remove(&existing.id) {
            session.vm.remove_nodes(&ids)?;
        }
        session.window_stack.retain(|item| item.kind != window.kind);
    }
    let ids = execute_window(&session.vm, &session.request, &window)?;
    session.window_nodes.insert(window.id.clone(), ids);
    session.window_stack.push(window);
    Ok(())
}

fn signal_action(signal: &str) -> Option<String> {
    if signal == "close-top" {
        return Some(signal.to_string());
    }
    let value = signal.strip_prefix("open:")?.to_ascii_lowercase();
    if value.contains("bag") {
        Some("open-bag".to_string())
    } else if value.contains("team") {
        Some("open-team".to_string())
    } else if value.contains("store") || value.contains("mall") {
        Some("open-store".to_string())
    } else {
        None
    }
}

fn window(kind: &str) -> RuntimeWindowState {
    let paths = match kind {
        "bag" => vec!["GUIExport/bag/bag_panel.lua"],
        "team" => vec![
            "GUIExport/team/team_fram.lua",
            "GUIExport/team/team_panel.lua",
        ],
        "store" => vec![
            "GUIExport/store/store_frame.lua",
            "GUIExport/store/page_store_panel.lua",
        ],
        _ => Vec::new(),
    };
    RuntimeWindowState {
        id: format!("{kind}-window"),
        kind: kind.to_string(),
        source_paths: paths.into_iter().map(str::to_string).collect(),
    }
}

fn resolve_event_action(event: &EventRequest, scene: &RuntimeScene) -> Option<String> {
    if matches!(
        event.name.as_str(),
        "open-bag" | "open-team" | "open-store" | "close-top"
    ) {
        return Some(event.name.clone());
    }
    let explicit = event
        .payload
        .get("action")
        .or_else(|| {
            event
                .payload
                .get("data")
                .and_then(|data| data.get("action"))
        })
        .and_then(Value::as_str);
    if let Some(action) = explicit {
        return Some(action.to_string());
    }
    if !matches!(event.name.as_str(), "click" | "node.click") {
        return None;
    }
    let node_id = event.payload.get("nodeId").and_then(Value::as_str)?;
    let name = scene.nodes.get(node_id)?.name.to_ascii_lowercase();
    if name.contains("bag") || name == "button_grey1" {
        Some("open-bag".to_string())
    } else if name.contains("team") || name == "button_red2" {
        Some("open-team".to_string())
    } else if name.contains("store") || name.contains("mall") {
        Some("open-store".to_string())
    } else if name.contains("close") {
        Some("close-top".to_string())
    } else {
        None
    }
}

fn resolve_preset_id(request: &StartRequest) -> String {
    let value = request
        .preset_id
        .as_deref()
        .unwrap_or(request.scene_id.as_str());
    match value {
        "hud-mobile" => GAME_MOBILE.to_string(),
        "hud-pc" => GAME_PC.to_string(),
        "login" => CHARACTER_SELECT.to_string(),
        _ => value.to_string(),
    }
}

fn is_preset(value: &str) -> bool {
    matches!(
        value,
        CHARACTER_CREATE | CHARACTER_SELECT | GAME_MOBILE | GAME_PC
    )
}

fn preset_source_paths(preset_id: &str) -> &'static [&'static str] {
    match preset_id {
        CHARACTER_CREATE => &["GUIExport/login_role/login_role_create.lua"],
        CHARACTER_SELECT => &["GUIExport/login_role/login_role.lua"],
        GAME_MOBILE => MOBILE_EXPORTS,
        GAME_PC => PC_EXPORTS,
        _ => &[],
    }
}

fn window_fallback_source(kind: &str) -> Option<&'static str> {
    match kind {
        "bag" => Some(
            r##"local root = GUI:Layout_Create(parent, "BagWindow", 620, 80, 470, 520, false)
GUI:Text_Create(root, "Title", 24, 475, 22, "#ffffff", "背包")
GUI:ListView_Create(root, "Items", 24, 80, 420, 370, 1)
return root"##,
        ),
        "team" => Some(
            r##"local root = GUI:Layout_Create(parent, "TeamWindow", 190, 100, 756, 480, false)
GUI:Text_Create(root, "Title", 330, 445, 22, "#ffffff", "组队")
GUI:ListView_Create(root, "Members", 180, 80, 520, 320, 1)
return root"##,
        ),
        "store" => Some(
            r##"local root = GUI:Layout_Create(parent, "StoreWindow", 188, 80, 760, 520, false)
GUI:Text_Create(root, "Title", 330, 480, 22, "#ffffff", "商城")
GUI:ListView_Create(root, "Goods", 190, 70, 540, 370, 1)
return root"##,
        ),
        _ => None,
    }
}

fn diff_scene(
    previous: &RuntimeScene,
    next: &RuntimeScene,
    base_sequence: u64,
    sequence: u64,
) -> RuntimeScenePatch {
    let upserted_nodes = next
        .nodes
        .iter()
        .filter(|(id, node)| previous.nodes.get(*id) != Some(*node))
        .map(|(id, node)| (id.clone(), node.clone()))
        .collect();
    let removed_node_ids = previous
        .nodes
        .keys()
        .filter(|id| !next.nodes.contains_key(*id))
        .cloned()
        .collect();
    RuntimeScenePatch {
        base_sequence,
        sequence,
        upserted_nodes,
        removed_node_ids,
        roots: next.roots.clone(),
    }
}

fn scene_result(
    session_id: String,
    sequence: u64,
    scene: RuntimeScene,
    patch: Option<RuntimeScenePatch>,
    preset_id: &str,
    window_stack: &[RuntimeWindowState],
    mock_state: &BTreeMap<String, Value>,
) -> SceneResult {
    SceneResult {
        session_id,
        sequence,
        diagnostics: scene.diagnostics.clone(),
        scene,
        patch,
        preset_id: preset_id.to_string(),
        window_stack: window_stack.to_vec(),
        mock_state: mock_state.clone(),
    }
}

fn success(response_version: u32, request_id: String, result: RuntimeResult) -> RuntimeResponse {
    let diagnostics = match &result {
        RuntimeResult::Scene(scene) => scene.diagnostics.clone(),
        _ => Vec::new(),
    };
    RuntimeResponse {
        protocol_version: response_version,
        request_id,
        ok: true,
        result: Some(result),
        error: None,
        diagnostics,
    }
}

fn failure(
    response_version: u32,
    request_id: String,
    code: &str,
    message: String,
) -> RuntimeResponse {
    RuntimeResponse {
        protocol_version: response_version,
        request_id,
        ok: false,
        result: None,
        error: Some(RuntimeError {
            code: code.to_string(),
            message: message.clone(),
        }),
        diagnostics: vec![RuntimeDiagnostic {
            severity: DiagnosticSeverity::Error,
            code: code.to_string(),
            message,
            source_ref: None,
            provenance: None,
        }],
    }
}

fn failure_from_string(
    response_version: u32,
    request_id: String,
    error: String,
) -> RuntimeResponse {
    let code = error
        .split(|character: char| character == ':' || character.is_whitespace())
        .find(|part| part.starts_with("RUNTIME_"))
        .unwrap_or("RUNTIME_EXECUTION_FAILED")
        .to_string();
    failure(response_version, request_id, &code, error)
}
