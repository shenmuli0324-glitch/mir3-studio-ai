use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mlua::chunk::ChunkMode;
use mlua::{HookTriggers, Lua, LuaOptions, MultiValue, StdLib, Table, Value, VmState};
use serde_json::{json, Value as JsonValue};

use crate::model::{
    DataProfileSnapshot, DataProvenance, DataProvenanceKind, DiagnosticSeverity, RuntimeDiagnostic,
    RuntimeLimits, RuntimeNode, RuntimeScene, RuntimeSize, RuntimeTransform, SourceRef, Viewport,
};

struct SceneBuilder {
    scene: RuntimeScene,
    next_node: usize,
    max_nodes: usize,
    source_ref: SourceRef,
}

impl SceneBuilder {
    fn new(
        profile_id: &str,
        data_origin: &str,
        viewport: Viewport,
        max_nodes: usize,
        source_ref: SourceRef,
    ) -> Self {
        let root_id = "runtime-root".to_string();
        let root = RuntimeNode {
            id: root_id.clone(),
            node_type: "Scene".to_string(),
            name: "Scene".to_string(),
            parent_id: None,
            children: Vec::new(),
            transform: RuntimeTransform::default(),
            size: RuntimeSize {
                width: viewport.width as f64,
                height: viewport.height as f64,
            },
            visible: true,
            text: None,
            asset: None,
            asset_slots: BTreeMap::new(),
            source_ref: Some(source_ref.clone()),
            properties: BTreeMap::new(),
        };
        let mut nodes = BTreeMap::new();
        nodes.insert(root_id.clone(), root);
        Self {
            scene: RuntimeScene {
                id: format!("scene-{profile_id}"),
                profile_id: profile_id.to_string(),
                viewport,
                roots: vec![root_id],
                nodes,
                diagnostics: Vec::new(),
                provenance: vec![DataProvenance {
                    kind: if data_origin == "projectStatic" {
                        DataProvenanceKind::StaticConfig
                    } else {
                        DataProvenanceKind::SceneMock
                    },
                    key: profile_id.to_string(),
                    description: if data_origin == "projectStatic" {
                        "场景使用当前项目白名单 XLS 的脱敏静态快照".to_string()
                    } else {
                        "场景使用脱机模拟运行态数据，不代表真实服务器状态".to_string()
                    },
                }],
            },
            next_node: 1,
            max_nodes,
            source_ref,
        }
    }

    fn root_handle(&self, lua: &Lua) -> mlua::Result<Table> {
        node_handle(lua, "runtime-root")
    }

    fn create_node(&mut self, lua: &Lua, method: &str, args: &MultiValue) -> mlua::Result<Table> {
        if self.scene.nodes.len() >= self.max_nodes {
            return Err(mlua::Error::RuntimeError(
                "RUNTIME_NODE_LIMIT: 节点数量超过沙箱限制".to_string(),
            ));
        }
        let values: Vec<Value> = args.iter().cloned().collect();
        let parent_index = values.iter().position(node_id_from_value);
        let parent_id = parent_index
            .and_then(|index| node_id(&values[index]))
            .unwrap_or_else(|| "runtime-root".to_string());
        let first = parent_index.map(|index| index + 1).unwrap_or(0);
        let name = string_at(&values, first).unwrap_or_else(|| format!("Node_{}", self.next_node));
        let x = number_at(&values, first + 1).unwrap_or(0.0);
        let y = number_at(&values, first + 2).unwrap_or(0.0);
        let node_type = method.strip_suffix("_Create").unwrap_or(method).to_string();
        let (width, height) = create_size(&node_type, &values, first);
        let asset_slots = create_asset_slots(&node_type, &values, first);
        let asset = primary_asset(&node_type, &asset_slots);
        let text = create_text(&node_type, &values, first);
        let id = format!("runtime-node-{}", self.next_node);
        self.next_node += 1;
        let node = RuntimeNode {
            id: id.clone(),
            node_type,
            name,
            parent_id: Some(parent_id.clone()),
            children: Vec::new(),
            transform: RuntimeTransform {
                x,
                y,
                ..RuntimeTransform::default()
            },
            size: RuntimeSize { width, height },
            visible: true,
            text,
            asset,
            asset_slots,
            source_ref: Some(self.source_ref.clone()),
            properties: BTreeMap::new(),
        };
        if let Some(parent) = self.scene.nodes.get_mut(&parent_id) {
            parent.children.push(id.clone());
        } else {
            self.scene.diagnostics.push(RuntimeDiagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "RUNTIME_PARENT_MISSING".to_string(),
                message: format!("节点父级 {parent_id} 不存在，已保留为未连接节点"),
                source_ref: Some(self.source_ref.clone()),
                provenance: None,
            });
        }
        self.scene.nodes.insert(id.clone(), node);
        node_handle(lua, &id)
    }

    fn mutate_node(&mut self, method: &str, args: &MultiValue) {
        let values: Vec<Value> = args.iter().cloned().collect();
        let Some(index) = values.iter().position(node_id_from_value) else {
            self.denied(method, "setter 未收到有效节点");
            return;
        };
        let Some(id) = node_id(&values[index]) else {
            return;
        };
        let Some(node) = self.scene.nodes.get_mut(&id) else {
            return;
        };
        match method {
            "setPosition" | "Widget_setPosition" => {
                node.transform.x = number_at(&values, index + 1).unwrap_or(node.transform.x);
                node.transform.y = number_at(&values, index + 2).unwrap_or(node.transform.y);
            }
            "setContentSize" | "Widget_setContentSize" => {
                node.size.width = number_at(&values, index + 1).unwrap_or(node.size.width);
                node.size.height = number_at(&values, index + 2).unwrap_or(node.size.height);
            }
            "setAnchorPoint" | "Widget_setAnchorPoint" => {
                node.transform.anchor_x =
                    number_at(&values, index + 1).unwrap_or(node.transform.anchor_x);
                node.transform.anchor_y =
                    number_at(&values, index + 2).unwrap_or(node.transform.anchor_y);
            }
            "setScale" | "Widget_setScale" => {
                let scale_x = number_at(&values, index + 1).unwrap_or(node.transform.scale_x);
                node.transform.scale_x = scale_x;
                node.transform.scale_y = number_at(&values, index + 2).unwrap_or(scale_x);
            }
            "setRotation" | "Widget_setRotation" => {
                node.transform.rotation =
                    number_at(&values, index + 1).unwrap_or(node.transform.rotation);
            }
            "setVisible" | "Widget_setVisible" => {
                node.visible = bool_at(&values, index + 1).unwrap_or(node.visible);
            }
            "Text_setString" | "setString" => {
                node.text = string_at(&values, index + 1);
            }
            "Image_loadTexture" => set_asset_slot(node, "normal", string_at(&values, index + 1)),
            "Layout_setBackGroundImage"
            | "ListView_setBackGroundImage"
            | "ScrollView_setBackGroundImage" => {
                set_asset_slot(node, "background", string_at(&values, index + 1));
            }
            "Button_loadTextureNormal" => {
                set_asset_slot(node, "normal", string_at(&values, index + 1));
            }
            "Button_loadTexturePressed" => {
                set_asset_slot(node, "pressed", string_at(&values, index + 1));
            }
            "Button_loadTextureDisabled" => {
                set_asset_slot(node, "disabled", string_at(&values, index + 1));
            }
            "CheckBox_loadTextureBackGround" => {
                set_asset_slot(node, "normal", string_at(&values, index + 1));
            }
            "CheckBox_loadTextureFrontCross" => {
                set_asset_slot(node, "selected", string_at(&values, index + 1));
            }
            "Slider_loadBarTexture" => {
                set_asset_slot(node, "background", string_at(&values, index + 1));
            }
            "Slider_loadProgressBarTexture" => {
                set_asset_slot(node, "progress", string_at(&values, index + 1));
            }
            "Slider_loadSlidBallTextureNormal"
            | "Slider_loadSlidBallTexturePressed"
            | "Slider_loadSlidBallTextureDisabled" => {
                set_asset_slot(node, "thumb", string_at(&values, index + 1));
            }
            "LoadingBar_loadTexture" => {
                set_asset_slot(node, "progress", string_at(&values, index + 1));
            }
            "TextAtlas_loadTexture" => {
                set_asset_slot(node, "atlas", string_at(&values, index + 1));
            }
            "ProgressTimer_loadTexture" => {
                set_asset_slot(node, "normal", string_at(&values, index + 1));
            }
            "SpineAnim_loadJson" => {
                set_asset_slot(node, "json", string_at(&values, index + 1));
            }
            "SpineAnim_loadAtlas" => {
                set_asset_slot(node, "atlas", string_at(&values, index + 1));
            }
            "setTexture" | "loadTexture" => {
                if let Some(asset) = string_at(&values, index + 1) {
                    let slot = primary_asset_slot(&node.node_type);
                    set_asset_slot(node, slot, Some(asset));
                }
            }
            _ => {
                let property = method.to_string();
                let value = values
                    .get(index + 1)
                    .map(lua_value_to_json)
                    .unwrap_or(JsonValue::Null);
                node.properties.insert(property, value);
            }
        }
    }

    fn denied(&mut self, api: &str, detail: &str) {
        self.scene.diagnostics.push(RuntimeDiagnostic {
            severity: DiagnosticSeverity::Warning,
            code: "RUNTIME_API_DENIED".to_string(),
            message: format!("API {api} 已被沙箱拒绝：{detail}"),
            source_ref: Some(self.source_ref.clone()),
            provenance: Some(DataProvenance {
                kind: DataProvenanceKind::Missing,
                key: api.to_string(),
                description: "该能力需要真实游戏运行时或网络数据".to_string(),
            }),
        });
    }
}

pub fn execute_scene(
    profile_id: &str,
    layout_path: &str,
    viewport: Viewport,
    modules: &BTreeMap<String, String>,
    data_profile: &DataProfileSnapshot,
    limits: RuntimeLimits,
    fallback_source: Option<&str>,
) -> Result<RuntimeScene, String> {
    validate_inputs(layout_path, modules, limits)?;
    let source_ref = SourceRef {
        dev_relative_path: layout_path.to_string(),
        line: None,
        column: None,
        template_node_id: None,
    };
    let builder = Arc::new(Mutex::new(SceneBuilder::new(
        profile_id,
        &data_profile.origin,
        viewport,
        limits.max_nodes,
        source_ref,
    )));
    let lua = Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH,
        LuaOptions::default(),
    )
    .map_err(|error| format!("RUNTIME_LUA_INIT: {error}"))?;
    lua.set_memory_limit(limits.max_memory_bytes)
        .map_err(|error| format!("RUNTIME_MEMORY_LIMIT: {error}"))?;
    install_instruction_limit(&lua, limits.max_instructions)?;
    remove_dangerous_globals(&lua)?;
    install_compatibility(&lua, Arc::clone(&builder), modules, data_profile)?;
    let root = builder
        .lock()
        .map_err(|_| "RUNTIME_STATE_POISONED: 场景状态不可用".to_string())?
        .root_handle(&lua)
        .map_err(lua_error)?;
    lua.globals()
        .set("parent", root.clone())
        .map_err(lua_error)?;
    let source = modules
        .get(layout_path)
        .map(String::as_str)
        .or(fallback_source)
        .ok_or_else(|| format!("RUNTIME_ENTRY_NOT_FOUND: 虚拟模块中不存在 {layout_path}"))?;
    let result: Value = lua
        .load(source)
        .set_name(format!("@{layout_path}"))
        .set_mode(ChunkMode::Text)
        .eval()
        .map_err(lua_error)?;
    call_entry_if_needed(&lua, result, root, data_profile).map_err(lua_error)?;
    let mut state = builder
        .lock()
        .map_err(|_| "RUNTIME_STATE_POISONED: 场景状态不可用".to_string())?;
    if !data_profile.values.is_empty() || !data_profile.meta_values.is_empty() {
        state.scene.provenance.push(DataProvenance {
            kind: DataProvenanceKind::UserSnapshot,
            key: "dataProfile".to_string(),
            description: "数据由调用方提供的只读快照注入".to_string(),
        });
    }
    Ok(state.scene.clone())
}

fn validate_inputs(
    layout_path: &str,
    modules: &BTreeMap<String, String>,
    limits: RuntimeLimits,
) -> Result<(), String> {
    if layout_path.is_empty() || layout_path.starts_with('/') || layout_path.contains("..") {
        return Err("RUNTIME_PATH_DENIED: layoutPath 必须是安全的 dev 相对路径".to_string());
    }
    if modules.len() > limits.max_modules {
        return Err(format!(
            "RUNTIME_MODULE_LIMIT: 虚拟模块数量超过 {}",
            limits.max_modules
        ));
    }
    let total_bytes = modules.values().map(String::len).sum::<usize>();
    if total_bytes > limits.max_source_bytes {
        return Err(format!(
            "RUNTIME_SOURCE_LIMIT: 虚拟源码总大小超过 {} 字节",
            limits.max_source_bytes
        ));
    }
    if modules
        .keys()
        .any(|path| path.starts_with('/') || path.contains(".."))
    {
        return Err("RUNTIME_PATH_DENIED: 虚拟模块包含越界路径".to_string());
    }
    Ok(())
}

fn install_instruction_limit(lua: &Lua, max_instructions: u64) -> Result<(), String> {
    let step = 1_000_u32;
    let count = Arc::new(AtomicU64::new(0));
    lua.set_global_hook(
        HookTriggers::new().every_nth_instruction(step),
        move |_, _| {
            let current = count.fetch_add(step as u64, Ordering::Relaxed) + step as u64;
            if current > max_instructions {
                return Err(mlua::Error::RuntimeError(
                    "RUNTIME_INSTRUCTION_LIMIT: Lua 执行预算已耗尽".to_string(),
                ));
            }
            Ok(VmState::Continue)
        },
    )
    .map_err(|error| format!("RUNTIME_HOOK_INIT: {error}"))
}

fn remove_dangerous_globals(lua: &Lua) -> Result<(), String> {
    let globals = lua.globals();
    for name in [
        "io",
        "os",
        "package",
        "debug",
        "ffi",
        "jit",
        "load",
        "loadstring",
        "loadfile",
        "dofile",
        "module",
    ] {
        globals.set(name, Value::Nil).map_err(lua_error)?;
    }
    // Lua 的默认 print 会写入 sidecar stdout，破坏一行一响应的 JSONL 协议。
    globals
        .set(
            "print",
            lua.create_function(|_, _: MultiValue| Ok(()))
                .map_err(lua_error)?,
        )
        .map_err(lua_error)?;
    Ok(())
}

fn install_compatibility(
    lua: &Lua,
    builder: Arc<Mutex<SceneBuilder>>,
    modules: &BTreeMap<String, String>,
    data_profile: &DataProfileSnapshot,
) -> Result<(), String> {
    let modules = Arc::new(modules.clone());
    let gui_modules = Arc::clone(&modules);
    let gui = lua.create_table().map_err(lua_error)?;
    let gui_meta = lua.create_table().map_err(lua_error)?;
    let gui_builder = Arc::clone(&builder);
    gui_meta
        .set(
            "__index",
            lua.create_function(move |lua, (_table, key): (Table, String)| {
                let method = key.clone();
                let method_for_call = method.clone();
                let state = Arc::clone(&gui_builder);
                let virtual_modules = Arc::clone(&gui_modules);
                lua.create_function(move |lua, args: MultiValue| {
                    if method_for_call == "LoadExport" {
                        return load_export(lua, &args, &virtual_modules, Arc::clone(&state));
                    }
                    if method_for_call == "ui_delegate" {
                        return ui_delegate(lua, &args, Arc::clone(&state)).map(Value::Table);
                    }
                    if method_for_call == "getChildByName" {
                        return get_child_by_name(lua, &args, Arc::clone(&state));
                    }
                    if matches!(
                        method_for_call.as_str(),
                        "Text_getString" | "TextInput_getString"
                    ) {
                        return get_node_text(lua, &args, Arc::clone(&state));
                    }
                    if matches!(
                        method_for_call.as_str(),
                        "GetLayerOpenParam" | "SetLayerOpenParam"
                    ) {
                        return Ok(Value::Nil);
                    }
                    if method_for_call.ends_with("_Create") || method_for_call == "Win_Create" {
                        return state
                            .lock()
                            .map_err(|_| {
                                mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into())
                            })?
                            .create_node(lua, &method_for_call, &args)
                            .map(Value::Table);
                    }
                    if is_denied_api(&method_for_call) {
                        state
                            .lock()
                            .map_err(|_| {
                                mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into())
                            })?
                            .denied(&method_for_call, "文件、进程、网络和后端调用不允许执行");
                        return Ok(Value::Nil);
                    }
                    if !is_safe_widget_api(&method_for_call) {
                        state
                            .lock()
                            .map_err(|_| {
                                mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into())
                            })?
                            .denied(&method_for_call, "兼容层未声明该 GUI API，默认拒绝执行");
                        return Ok(Value::Nil);
                    }
                    state
                        .lock()
                        .map_err(|_| mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into()))?
                        .mutate_node(&method_for_call, &args);
                    Ok(Value::Nil)
                })
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;
    gui.set_metatable(Some(gui_meta)).map_err(lua_error)?;
    lua.globals().set("GUI", gui).map_err(lua_error)?;

    let loaded = Arc::new(Mutex::new(HashSet::<String>::new()));
    let require = make_require(
        lua,
        Arc::clone(&modules),
        Arc::new(data_profile.tables.clone()),
        Arc::clone(&loaded),
    )?;
    lua.globals()
        .set("require", require.clone())
        .map_err(lua_error)?;

    let sl = lua.create_table().map_err(lua_error)?;
    sl.set("Require", require).map_err(lua_error)?;
    install_snapshot_getter(lua, &sl, "GetValue", data_profile.values.clone())?;
    let mut meta_values = data_profile.values.clone();
    meta_values.extend(data_profile.meta_values.clone());
    install_snapshot_getter(lua, &sl, "GetMetaValue", meta_values)?;
    let sl_meta = lua.create_table().map_err(lua_error)?;
    let sl_builder = Arc::clone(&builder);
    sl_meta
        .set(
            "__index",
            lua.create_function(move |lua, (_table, key): (Table, String)| {
                let api = key.clone();
                let state = Arc::clone(&sl_builder);
                lua.create_function(move |_, _args: MultiValue| {
                    if is_denied_api(&api) || api.starts_with("Request") {
                        state
                            .lock()
                            .map_err(|_| {
                                mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into())
                            })?
                            .denied(&api, "需要真实游戏服务、网络或持久化状态");
                    } else {
                        state
                            .lock()
                            .map_err(|_| {
                                mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into())
                            })?
                            .denied(&api, "兼容层尚未声明该 API，已安全返回 nil");
                    }
                    Ok(Value::Nil)
                })
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;
    sl.set_metatable(Some(sl_meta)).map_err(lua_error)?;
    lua.globals().set("SL", sl).map_err(lua_error)?;
    install_symbol_globals(lua)?;
    Ok(())
}

fn load_export(
    lua: &Lua,
    args: &MultiValue,
    modules: &BTreeMap<String, String>,
    builder: Arc<Mutex<SceneBuilder>>,
) -> mlua::Result<Value> {
    let values: Vec<Value> = args.iter().cloned().collect();
    let root = values
        .iter()
        .find_map(|value| match value {
            Value::Table(table) if node_id(value).is_some() => Some(table.clone()),
            _ => None,
        })
        .ok_or_else(|| mlua::Error::RuntimeError("RUNTIME_EXPORT_PARENT: 缺少父节点".into()))?;
    let requested = values
        .iter()
        .find_map(|value| match value {
            Value::String(value) => value.to_str().ok().map(|value| value.to_string()),
            _ => None,
        })
        .ok_or_else(|| mlua::Error::RuntimeError("RUNTIME_EXPORT_PATH: 缺少导出路径".into()))?;
    let path = export_module_path(&requested);
    let Some(source) = modules.get(&path).or_else(|| modules.get(&requested)) else {
        builder
            .lock()
            .map_err(|_| mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into()))?
            .denied("GUI.LoadExport", &format!("虚拟模块中不存在 {path}"));
        return Ok(Value::Nil);
    };
    let previous_source_ref = {
        let mut state = builder
            .lock()
            .map_err(|_| mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into()))?;
        let previous = state.source_ref.clone();
        state.source_ref.dev_relative_path = path.clone();
        previous
    };
    let execution = (|| {
        let result = lua
            .load(source)
            .set_name(format!("@{path}"))
            .set_mode(ChunkMode::Text)
            .eval::<Value>()?;
        if let Value::Table(table) = &result {
            if let Ok(init) = table.get::<mlua::Function>("init") {
                init.call::<()>((root, lua.create_table()?, Value::Nil))?;
            }
        }
        Ok(result)
    })();
    builder
        .lock()
        .map_err(|_| mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into()))?
        .source_ref = previous_source_ref;
    execution
}

fn ui_delegate(
    lua: &Lua,
    args: &MultiValue,
    builder: Arc<Mutex<SceneBuilder>>,
) -> mlua::Result<Table> {
    let values: Vec<Value> = args.iter().cloned().collect();
    let root_id = values
        .iter()
        .find_map(node_id)
        .unwrap_or_else(|| "runtime-root".to_string());
    let state = builder
        .lock()
        .map_err(|_| mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into()))?;
    let output = lua.create_table()?;
    let mut pending = vec![root_id];
    while let Some(id) = pending.pop() {
        let Some(node) = state.scene.nodes.get(&id) else {
            continue;
        };
        output.set(node.name.as_str(), node_handle(lua, &node.id)?)?;
        pending.extend(node.children.iter().cloned());
    }
    Ok(output)
}

fn get_child_by_name(
    lua: &Lua,
    args: &MultiValue,
    builder: Arc<Mutex<SceneBuilder>>,
) -> mlua::Result<Value> {
    let values: Vec<Value> = args.iter().cloned().collect();
    let Some(parent_id) = values.iter().find_map(node_id) else {
        return Ok(Value::Nil);
    };
    let Some(name) = values.iter().find_map(|value| match value {
        Value::String(value) => value.to_str().ok().map(|value| value.to_string()),
        _ => None,
    }) else {
        return Ok(Value::Nil);
    };
    let state = builder
        .lock()
        .map_err(|_| mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into()))?;
    let child = state
        .scene
        .nodes
        .get(&parent_id)
        .and_then(|parent| {
            parent
                .children
                .iter()
                .filter_map(|id| state.scene.nodes.get(id))
                .find(|node| node.name == name)
        })
        .map(|node| node_handle(lua, &node.id))
        .transpose()?;
    Ok(child.map(Value::Table).unwrap_or(Value::Nil))
}

fn get_node_text(
    lua: &Lua,
    args: &MultiValue,
    builder: Arc<Mutex<SceneBuilder>>,
) -> mlua::Result<Value> {
    let Some(id) = args.iter().find_map(node_id) else {
        return Ok(Value::String(lua.create_string("")?));
    };
    let state = builder
        .lock()
        .map_err(|_| mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into()))?;
    let text = state
        .scene
        .nodes
        .get(&id)
        .and_then(|node| node.text.as_deref())
        .unwrap_or("");
    Ok(Value::String(lua.create_string(text)?))
}

fn install_symbol_globals(lua: &Lua) -> Result<(), String> {
    for name in ["UIConst", "SLDefine", "GUIDefine"] {
        let root = lua.create_table().map_err(lua_error)?;
        let meta = lua.create_table().map_err(lua_error)?;
        meta.set(
            "__index",
            lua.create_function(|lua, (_table, _key): (Table, String)| {
                let namespace = lua.create_table()?;
                let namespace_meta = lua.create_table()?;
                namespace_meta.set(
                    "__index",
                    lua.create_function(|_, (_table, key): (Table, String)| Ok(key))?,
                )?;
                namespace.set_metatable(Some(namespace_meta))?;
                Ok(namespace)
            })
            .map_err(lua_error)?,
        )
        .map_err(lua_error)?;
        root.set_metatable(Some(meta)).map_err(lua_error)?;
        lua.globals().set(name, root).map_err(lua_error)?;
    }
    Ok(())
}

fn make_require(
    lua: &Lua,
    modules: Arc<BTreeMap<String, String>>,
    tables: Arc<BTreeMap<String, JsonValue>>,
    loaded: Arc<Mutex<HashSet<String>>>,
) -> Result<mlua::Function, String> {
    lua.create_function(move |lua, args: MultiValue| {
        let values: Vec<Value> = args.iter().cloned().collect();
        let path = values
            .iter()
            .find_map(|value| match value {
                Value::String(value) => value.to_str().ok().map(|value| value.to_string()),
                _ => None,
            })
            .ok_or_else(|| {
                mlua::Error::RuntimeError("RUNTIME_REQUIRE_PATH: 缺少模块路径".into())
            })?;
        let normalized = normalize_module_path(&path);
        if let Some(table_name) = config_table_name(&path) {
            if let Some(value) = tables.get(&table_name) {
                return json_to_lua(lua, value);
            }
        }
        let source = modules
            .get(&path)
            .or_else(|| modules.get(&normalized))
            .ok_or_else(|| {
                mlua::Error::RuntimeError(format!("RUNTIME_MODULE_NOT_FOUND: {path}"))
            })?;
        let mut active = loaded
            .lock()
            .map_err(|_| mlua::Error::RuntimeError("RUNTIME_STATE_POISONED".into()))?;
        if !active.insert(normalized.clone()) {
            return Err(mlua::Error::RuntimeError(format!(
                "RUNTIME_MODULE_CYCLE: {normalized}"
            )));
        }
        drop(active);
        let result = lua
            .load(source)
            .set_name(format!("@{normalized}"))
            .set_mode(ChunkMode::Text)
            .eval::<Value>();
        if let Ok(mut active) = loaded.lock() {
            active.remove(&normalized);
        }
        result
    })
    .map_err(lua_error)
}

fn install_snapshot_getter(
    lua: &Lua,
    sl: &Table,
    name: &str,
    values: BTreeMap<String, JsonValue>,
) -> Result<(), String> {
    let values = Arc::new(values);
    sl.set(
        name,
        lua.create_function(move |lua, args: MultiValue| {
            let key = args.iter().find_map(|value| match value {
                Value::String(value) => value.to_str().ok().map(|value| value.to_string()),
                _ => None,
            });
            match key.and_then(|key| values.get(&key)) {
                Some(value) => json_to_lua(lua, value),
                None => Ok(Value::Nil),
            }
        })
        .map_err(lua_error)?,
    )
    .map_err(lua_error)
}

fn call_entry_if_needed(
    lua: &Lua,
    result: Value,
    root: Table,
    data_profile: &DataProfileSnapshot,
) -> mlua::Result<()> {
    match result {
        Value::Table(table) => {
            if let Ok(init) = table.get::<mlua::Function>("init") {
                let data = snapshot_table(lua, data_profile)?;
                init.call::<()>((root, data, Value::Nil))?;
            }
        }
        Value::Function(function) => {
            function.call::<()>(root)?;
        }
        _ => {}
    }
    Ok(())
}

fn snapshot_table(lua: &Lua, snapshot: &DataProfileSnapshot) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (key, value) in &snapshot.values {
        table.set(key.as_str(), json_to_lua(lua, value)?)?;
    }
    Ok(table)
}

fn json_to_lua(lua: &Lua, value: &JsonValue) -> mlua::Result<Value> {
    match value {
        JsonValue::Null => Ok(Value::Nil),
        JsonValue::Bool(value) => Ok(Value::Boolean(*value)),
        JsonValue::Number(value) => Ok(Value::Number(value.as_f64().unwrap_or_default())),
        JsonValue::String(value) => Ok(Value::String(lua.create_string(value)?)),
        JsonValue::Array(values) => {
            let table = lua.create_table()?;
            for (index, value) in values.iter().enumerate() {
                table.set(index + 1, json_to_lua(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
        JsonValue::Object(values) => {
            let table = lua.create_table()?;
            for (key, value) in values {
                table.set(key.as_str(), json_to_lua(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

fn node_handle(lua: &Lua, id: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("__nodeId", id)?;
    Ok(table)
}

fn node_id_from_value(value: &Value) -> bool {
    node_id(value).is_some()
}

fn node_id(value: &Value) -> Option<String> {
    match value {
        Value::Table(table) => table.get::<String>("__nodeId").ok(),
        _ => None,
    }
}

fn number_at(values: &[Value], index: usize) -> Option<f64> {
    match values.get(index) {
        Some(Value::Integer(value)) => Some(*value as f64),
        Some(Value::Number(value)) => Some(*value),
        _ => None,
    }
}

fn bool_at(values: &[Value], index: usize) -> Option<bool> {
    match values.get(index) {
        Some(Value::Boolean(value)) => Some(*value),
        _ => None,
    }
}

fn string_at(values: &[Value], index: usize) -> Option<String> {
    match values.get(index) {
        Some(Value::String(value)) => value.to_str().ok().map(|value| value.to_string()),
        _ => None,
    }
}

fn create_size(node_type: &str, values: &[Value], first: usize) -> (f64, f64) {
    match node_type {
        "Layout" | "ListView" | "ScrollView" | "PageView" | "TableView" | "QuickCell"
        | "TextInput" | "RichText" | "ScrollText" | "LoadingBar" | "Slider" => (
            number_at(values, first + 3).unwrap_or(0.0),
            number_at(values, first + 4).unwrap_or(0.0),
        ),
        "Text" => (80.0, 24.0),
        "Button" => (120.0, 40.0),
        "Image" => (64.0, 64.0),
        _ => (0.0, 0.0),
    }
}

fn create_asset_slots(node_type: &str, values: &[Value], first: usize) -> BTreeMap<String, String> {
    let mut slots = BTreeMap::new();
    let mut insert = |slot: &str, index: usize| {
        if let Some(value) = string_at(values, index).filter(|value| !value.is_empty()) {
            slots.insert(slot.to_string(), value);
        }
    };
    match node_type {
        "Image" | "Button" | "ProgressTimer" => insert("normal", first + 3),
        "TextAtlas" => insert("atlas", first + 4),
        "CheckBox" => {
            insert("normal", first + 3);
            insert("selected", first + 4);
        }
        "Slider" => {
            insert("background", first + 3);
            insert("progress", first + 4);
            insert("thumb", first + 5);
        }
        "LoadingBar" => insert("progress", first + 3),
        "SpineAnim" => {
            insert("json", first + 3);
            insert("atlas", first + 4);
        }
        _ => {}
    }
    slots
}

fn primary_asset(node_type: &str, slots: &BTreeMap<String, String>) -> Option<String> {
    slots.get(primary_asset_slot(node_type)).cloned()
}

fn primary_asset_slot(node_type: &str) -> &'static str {
    match node_type {
        "Layout" | "ListView" | "ScrollView" | "Slider" => "background",
        "LoadingBar" => "progress",
        "TextAtlas" => "atlas",
        "SpineAnim" => "json",
        _ => "normal",
    }
}

fn set_asset_slot(node: &mut RuntimeNode, slot: &str, asset: Option<String>) {
    let Some(asset) = asset.filter(|value| !value.is_empty()) else {
        return;
    };
    node.asset_slots.insert(slot.to_string(), asset.clone());
    if slot == primary_asset_slot(&node.node_type) {
        node.asset = Some(asset);
    }
}

fn create_text(node_type: &str, values: &[Value], first: usize) -> Option<String> {
    match node_type {
        "Text" | "RichText" | "ScrollText" | "TextAtlas" | "TextInput" => values
            .iter()
            .skip(first + 3)
            .filter_map(|value| match value {
                Value::String(value) => value.to_str().ok().map(|value| value.to_string()),
                _ => None,
            })
            .last(),
        _ => None,
    }
}

fn lua_value_to_json(value: &Value) -> JsonValue {
    match value {
        Value::Nil => JsonValue::Null,
        Value::Boolean(value) => JsonValue::Bool(*value),
        Value::Integer(value) => json!(*value),
        Value::Number(value) => json!(*value),
        Value::String(value) => value
            .to_str()
            .map(|value| JsonValue::String(value.to_string()))
            .unwrap_or(JsonValue::Null),
        _ => JsonValue::String("<runtime-value>".to_string()),
    }
}

fn normalize_module_path(path: &str) -> String {
    let mut normalized = path.replace('.', "/");
    if !normalized.ends_with(".lua") {
        normalized.push_str(".lua");
    }
    normalized
}

fn export_module_path(path: &str) -> String {
    let path = path.trim_start_matches('/');
    let mut output = if path.starts_with("GUIExport/") {
        path.to_string()
    } else {
        format!("GUIExport/{path}")
    };
    if !output.ends_with(".lua") {
        output.push_str(".lua");
    }
    output
}

fn config_table_name(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let name = normalized
        .strip_prefix("game_config/")?
        .trim_end_matches(".lua");
    if name.starts_with("cfg_") && !name.contains('/') {
        Some(name.to_string())
    } else {
        None
    }
}

fn is_denied_api(api: &str) -> bool {
    let lower = api.to_ascii_lowercase();
    [
        "request", "http", "socket", "network", "file", "process", "system", "execute", "download",
        "upload", "shell",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn is_safe_widget_api(api: &str) -> bool {
    api.starts_with("set")
        || api.contains("_set")
        || api.contains("_load")
        || api.starts_with("addOn")
        || api.ends_with("_addOnEvent")
        || matches!(
            api,
            "removeAllChildren" | "removeFromParent" | "stopAllActions"
        )
}

fn lua_error(error: mlua::Error) -> String {
    let message = error.to_string();
    if message.contains("RUNTIME_") {
        message
    } else {
        format!("RUNTIME_LUA_EXECUTION: {message}")
    }
}
