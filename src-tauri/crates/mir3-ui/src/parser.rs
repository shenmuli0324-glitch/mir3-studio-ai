use crate::{
    widget_adapter_registry, AdapterAssetSlotBinding, AdapterPropertyKind, BoundValue,
    BoundValueSource, CompatibilityStatus, DiagnosticSeverity, Mir3UiAsset, Mir3UiCompatibility,
    Mir3UiContainer, Mir3UiDiagnostic, Mir3UiDocument, Mir3UiNode, Mir3UiNodeType, Mir3UiPoint,
    Mir3UiPropertyValue, Mir3UiScale9, Mir3UiSize, Mir3UiSource, Mir3UiTransform, Mir3UiViewport,
    SourceBinding, SourcePoint, SourceSpan, WidgetAdapter, MIR3_UI_SCHEMA_VERSION,
};
use std::collections::{BTreeMap, HashMap};
use tree_sitter::{Node, Parser, Point};

#[derive(Debug, Clone)]
struct CallArgument {
    text: String,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct GuiCall {
    method: String,
    arguments: Vec<CallArgument>,
    call_span: SourceSpan,
    statement_span: SourceSpan,
    assignment_variable: Option<String>,
    scope_start_byte: usize,
}

/// 将 Lua 源码静态转换为 MIR3 UI DOM；语法错误作为诊断返回而不是导致崩溃。
pub fn parse_document(
    source: &str,
    dev_relative_path: &str,
    source_sha256: &str,
    encoding: &str,
    newline: &str,
) -> Result<Mir3UiDocument, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_lua::LANGUAGE.into())
        .map_err(|error| format!("GUI_PARSER_LANGUAGE_FAILED: {error}"))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "GUI_PARSE_CANCELLED: tree-sitter did not return a tree".to_string())?;

    let mut diagnostics = Vec::new();
    collect_syntax_diagnostics(tree.root_node(), &mut diagnostics);
    let mut calls = Vec::new();
    collect_gui_calls(tree.root_node(), source, &mut calls);
    calls.sort_by_key(|call| call.call_span.start_byte);

    let mut nodes = Vec::new();
    let mut symbols: HashMap<(usize, String), usize> = HashMap::new();
    for call in calls {
        if call.method.ends_with("_Create") {
            process_create(call, &mut nodes, &mut symbols, &mut diagnostics);
        } else {
            process_setter(call, &mut nodes, &symbols);
        }
    }

    let known_ids: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect();
    let parent_links: Vec<(String, String)> = nodes
        .iter()
        .filter_map(|node| {
            node.parent_id
                .as_ref()
                .map(|parent| (parent.clone(), node.id.clone()))
        })
        .collect();
    for (parent, child) in parent_links {
        if let Some(index) = known_ids.get(&parent) {
            nodes[*index].children.push(child);
        }
    }
    let roots = nodes
        .iter()
        .filter(|node| node.parent_id.is_none())
        .map(|node| node.id.clone())
        .collect();
    let assets = collect_assets(&nodes);

    Ok(Mir3UiDocument {
        schema_version: MIR3_UI_SCHEMA_VERSION,
        source: Mir3UiSource {
            dev_relative_path: dev_relative_path.to_string(),
            sha256: source_sha256.to_string(),
            encoding: encoding.to_string(),
            newline: newline.to_string(),
            byte_length: source.len(),
        },
        viewport: Mir3UiViewport {
            width: 1136,
            height: 640,
        },
        roots,
        nodes,
        assets,
        diagnostics,
    })
}

fn collect_gui_calls(node: Node<'_>, source: &str, calls: &mut Vec<GuiCall>) {
    if node.kind() == "function_call" {
        if let Some(call) = gui_call(node, source) {
            calls.push(call);
            return;
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_gui_calls(child, source, calls);
    }
}

fn gui_call(node: Node<'_>, source: &str) -> Option<GuiCall> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(name, source)?;
    let method = name_text.strip_prefix("GUI:")?.to_string();
    let arguments_node = node.child_by_field_name("arguments")?;
    let mut cursor = arguments_node.walk();
    let arguments = arguments_node
        .named_children(&mut cursor)
        .filter_map(|argument| {
            Some(CallArgument {
                text: node_text(argument, source)?.to_string(),
                span: span(argument),
            })
        })
        .collect();
    let assignment = ancestor(node, "assignment_statement");
    let assignment_variable = assignment.and_then(|value| assignment_lhs(value, source));
    let scope_start_byte = function_scope_start(node);
    let statement_node = assignment
        .and_then(|value| {
            value
                .parent()
                .filter(|parent| parent.kind() == "variable_declaration")
                .or(Some(value))
        })
        .unwrap_or(node);
    Some(GuiCall {
        method,
        arguments,
        call_span: span(node),
        statement_span: span(statement_node),
        assignment_variable,
        scope_start_byte,
    })
}

fn function_scope_start(mut node: Node<'_>) -> usize {
    while let Some(parent) = node.parent() {
        if matches!(
            parent.kind(),
            "function_declaration" | "function_definition"
        ) {
            return parent.start_byte();
        }
        node = parent;
    }
    0
}

fn ancestor<'tree>(mut node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    while let Some(parent) = node.parent() {
        if parent.kind() == kind {
            return Some(parent);
        }
        node = parent;
    }
    None
}

fn assignment_lhs(node: Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let variable_list = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "variable_list")?;
    let mut variables = variable_list.walk();
    let mut names = variable_list.named_children(&mut variables);
    let first = names.next()?;
    if names.next().is_some() || first.kind() != "identifier" {
        return None;
    }
    node_text(first, source).map(str::to_string)
}

fn process_create(
    call: GuiCall,
    nodes: &mut Vec<Mir3UiNode>,
    symbols: &mut HashMap<(usize, String), usize>,
    diagnostics: &mut Vec<Mir3UiDiagnostic>,
) {
    let Some(variable) = call.assignment_variable.clone() else {
        diagnostics.push(Mir3UiDiagnostic {
            severity: DiagnosticSeverity::Warning,
            code: "GUI_CREATE_WITHOUT_VARIABLE".to_string(),
            message: format!("{} 未赋值给单一 Lua 变量，已跳过", call.method),
            span: Some(call.call_span),
            node_id: None,
        });
        return;
    };
    let registry = widget_adapter_registry();
    let adapter = registry.find(&call.method);
    let raw_type = call.method.trim_end_matches("_Create");
    let node_type = adapter
        .map(|value| value.node_type)
        .unwrap_or(Mir3UiNodeType::Unsupported);
    let id = format!("node-{}", call.call_span.start_byte);
    let parent_variable = call.arguments.first().map(|argument| argument.text.trim());
    let parent_id = parent_variable
        .and_then(|name| symbols.get(&(call.scope_start_byte, name.to_string())))
        .and_then(|index| nodes.get(*index))
        .map(|node| node.id.clone());
    let missing_parent = parent_id.is_none()
        && parent_variable.is_some_and(|name| !matches!(name, "parent" | "nil" | "_parent"));
    let name = string_argument(&call.arguments, 1, String::new());
    let x = bound_number(adapter, &call.arguments, "x", 0.0);
    let y = bound_number(adapter, &call.arguments, "y", 0.0);
    let width = bound_number(adapter, &call.arguments, "width", 0.0);
    let height = bound_number(adapter, &call.arguments, "height", 0.0);
    let asset_slots = create_asset_slots(adapter, &call.arguments);
    let image = primary_asset(adapter, &asset_slots)
        .cloned()
        .unwrap_or_else(|| bound_string(adapter, &call.arguments, "image", String::new()));
    let pressed_image = asset_slots
        .get("pressed")
        .cloned()
        .unwrap_or_else(|| BoundValue::default(String::new()));
    let disabled_image = asset_slots
        .get("disabled")
        .cloned()
        .unwrap_or_else(|| BoundValue::default(String::new()));
    let font_size = bound_number(adapter, &call.arguments, "fontSize", 14.0);
    let color = bound_string(adapter, &call.arguments, "color", String::new());
    let text = bound_string(adapter, &call.arguments, "text", String::new());
    let direction = bound_number(adapter, &call.arguments, "direction", 1.0);
    let clipping_enabled = bound_bool(
        adapter,
        &call.arguments,
        "clippingEnabled",
        matches!(
            node_type,
            Mir3UiNodeType::PageView
                | Mir3UiNodeType::ListView
                | Mir3UiNodeType::ScrollView
                | Mir3UiNodeType::TableView
        ),
    );
    let create_scale = bound_number(adapter, &call.arguments, "scale", 1.0);
    let properties = create_generic_properties(adapter, &call.arguments);
    let has_dynamic = name.source == BoundValueSource::Dynamic
        || properties
            .values()
            .any(|value| value.source == BoundValueSource::Dynamic);
    let status = if adapter.is_none() {
        CompatibilityStatus::Unknown
    } else if has_dynamic || missing_parent {
        CompatibilityStatus::Dynamic
    } else if adapter.is_some_and(|value| value.approximate) {
        CompatibilityStatus::Approximate
    } else {
        CompatibilityStatus::Supported
    };
    let reason = match status {
        CompatibilityStatus::Unknown => Some(format!("V0.2 未注册 GUI:{raw_type}_Create")),
        CompatibilityStatus::Dynamic if missing_parent => Some(format!(
            "无法静态解析父节点 {}",
            parent_variable.unwrap_or_default()
        )),
        CompatibilityStatus::Dynamic => Some("包含动态 Lua 属性；动态 token 保持只读".to_string()),
        CompatibilityStatus::Approximate => Some("运行时控件使用近似占位预览".to_string()),
        CompatibilityStatus::Supported => None,
    };
    let reason_code = match status {
        CompatibilityStatus::Unknown => Some("unsupported_api".to_string()),
        CompatibilityStatus::Dynamic if missing_parent => Some("unresolved_parent".to_string()),
        CompatibilityStatus::Dynamic => Some("dynamic_property".to_string()),
        CompatibilityStatus::Approximate => Some("runtime_approximation".to_string()),
        CompatibilityStatus::Supported => None,
    };
    let mut property_spans = BTreeMap::new();
    bind_span(&mut property_spans, "name", &name);
    bind_span(&mut property_spans, "x", &x);
    bind_span(&mut property_spans, "y", &y);
    bind_span(&mut property_spans, "width", &width);
    bind_span(&mut property_spans, "height", &height);
    bind_span(&mut property_spans, "image", &image);
    bind_span(&mut property_spans, "fontSize", &font_size);
    bind_span(&mut property_spans, "color", &color);
    bind_span(&mut property_spans, "text", &text);
    for (property, value) in &properties {
        bind_span(&mut property_spans, property, value);
    }

    let node = Mir3UiNode {
        id: id.clone(),
        node_type,
        parent_id,
        children: Vec::new(),
        lua_variable: variable.clone(),
        name,
        position: Mir3UiPoint { x, y },
        size: Mir3UiSize { width, height },
        anchor: Mir3UiPoint {
            x: BoundValue::default(0.0),
            y: BoundValue::default(0.0),
        },
        visible: BoundValue::default(true),
        text,
        image,
        pressed_image,
        disabled_image,
        asset_slots,
        font_size,
        color,
        opacity: BoundValue::default(255.0),
        tag: BoundValue::default(0.0),
        transform: Mir3UiTransform {
            scale_x: create_scale.clone(),
            scale_y: create_scale,
            ..Mir3UiTransform::default()
        },
        ignore_content_adapt_with_size: BoundValue::default(true),
        clipping_enabled,
        scale9: Mir3UiScale9::default(),
        container: Mir3UiContainer {
            direction,
            ..Mir3UiContainer::default()
        },
        properties,
        compatibility: Mir3UiCompatibility {
            status,
            reason_code,
            reason,
        },
        source_binding: SourceBinding {
            create_call: call.call_span,
            statement: call.statement_span,
            property_spans,
            insert_byte: call.statement_span.end_byte,
        },
    };
    let index = nodes.len();
    nodes.push(node);
    if has_dynamic {
        diagnostics.push(Mir3UiDiagnostic {
            severity: DiagnosticSeverity::Warning,
            code: "GUI_DYNAMIC_PROPERTY".to_string(),
            message: "控件包含无法静态求值的 Lua 表达式；相关属性已设为只读".to_string(),
            span: Some(call.call_span),
            node_id: Some(id),
        });
    }
    symbols.insert((call.scope_start_byte, variable), index);
}

fn bound_number(
    adapter: Option<&WidgetAdapter>,
    arguments: &[CallArgument],
    property: &str,
    default: f64,
) -> BoundValue<f64> {
    adapter
        .and_then(|value| {
            value
                .bindings
                .iter()
                .find(|binding| binding.property == property)
        })
        .map_or_else(
            || BoundValue::default(default),
            |binding| number_argument(arguments, binding.index, default),
        )
}

fn bound_string(
    adapter: Option<&WidgetAdapter>,
    arguments: &[CallArgument],
    property: &str,
    default: String,
) -> BoundValue<String> {
    let binding = adapter.and_then(|value| {
        value
            .bindings
            .iter()
            .find(|binding| binding.property == property)
    });
    match binding {
        Some(binding) => string_argument(arguments, binding.index, default),
        None => BoundValue::default(default),
    }
}

fn create_asset_slots(
    adapter: Option<&WidgetAdapter>,
    arguments: &[CallArgument],
) -> BTreeMap<String, BoundValue<String>> {
    let mut slots = BTreeMap::new();
    let Some(adapter) = adapter else {
        return slots;
    };
    for slot in adapter.asset_slots {
        let value = bound_string(Some(adapter), arguments, slot.property, String::new());
        slots.insert(slot.slot.to_string(), value);
    }
    slots
}

fn primary_asset<'a>(
    adapter: Option<&WidgetAdapter>,
    slots: &'a BTreeMap<String, BoundValue<String>>,
) -> Option<&'a BoundValue<String>> {
    adapter
        .and_then(|adapter| adapter.asset_slots.iter().find(|slot| slot.primary))
        .and_then(|slot| slots.get(slot.slot))
}

fn bound_bool(
    adapter: Option<&WidgetAdapter>,
    arguments: &[CallArgument],
    property: &str,
    default: bool,
) -> BoundValue<bool> {
    adapter
        .and_then(|value| {
            value
                .bindings
                .iter()
                .find(|binding| binding.property == property)
        })
        .map_or_else(
            || BoundValue::default(default),
            |binding| bool_argument(arguments, binding.index, default),
        )
}

fn create_generic_properties(
    adapter: Option<&WidgetAdapter>,
    arguments: &[CallArgument],
) -> BTreeMap<String, BoundValue<Mir3UiPropertyValue>> {
    let mut properties = BTreeMap::new();
    for (index, argument) in arguments.iter().enumerate().skip(2) {
        let binding =
            adapter.and_then(|value| value.bindings.iter().find(|binding| binding.index == index));
        let property = binding
            .map(|value| value.property.to_string())
            .unwrap_or_else(|| format!("createArg{index}"));
        let kind = binding.map(|value| value.kind);
        properties.insert(property, generic_argument(argument, kind));
    }
    properties
}

fn generic_argument(
    argument: &CallArgument,
    kind: Option<AdapterPropertyKind>,
) -> BoundValue<Mir3UiPropertyValue> {
    let literal = match kind {
        Some(AdapterPropertyKind::Number) => argument
            .text
            .trim()
            .parse::<f64>()
            .ok()
            .map(Mir3UiPropertyValue::Number),
        Some(AdapterPropertyKind::String | AdapterPropertyKind::Asset) => {
            lua_string(&argument.text).map(Mir3UiPropertyValue::String)
        }
        Some(AdapterPropertyKind::Boolean) => match argument.text.trim() {
            "true" => Some(Mir3UiPropertyValue::Boolean(true)),
            "false" => Some(Mir3UiPropertyValue::Boolean(false)),
            _ => None,
        },
        Some(AdapterPropertyKind::Any) => infer_literal(&argument.text),
        None => infer_literal(&argument.text),
    };
    match literal {
        Some(value) => BoundValue::literal(value, argument.text.clone(), argument.span),
        None => BoundValue::dynamic(
            Mir3UiPropertyValue::String(argument.text.clone()),
            argument.text.clone(),
            argument.span,
        ),
    }
}

fn infer_literal(token: &str) -> Option<Mir3UiPropertyValue> {
    let trimmed = token.trim();
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with("function") && trimmed.ends_with("end"))
    {
        return Some(Mir3UiPropertyValue::RawLiteral {
            lua_literal: trimmed.to_string(),
        });
    }
    if let Ok(value) = trimmed.parse::<f64>() {
        return Some(Mir3UiPropertyValue::Number(value));
    }
    match trimmed {
        "true" => Some(Mir3UiPropertyValue::Boolean(true)),
        "false" => Some(Mir3UiPropertyValue::Boolean(false)),
        "nil" => Some(Mir3UiPropertyValue::Nil),
        _ => lua_string(trimmed).map(Mir3UiPropertyValue::String),
    }
}

fn process_setter(
    call: GuiCall,
    nodes: &mut [Mir3UiNode],
    symbols: &HashMap<(usize, String), usize>,
) {
    let Some(target) = call.arguments.first().map(|argument| argument.text.trim()) else {
        return;
    };
    let Some(index) = symbols
        .get(&(call.scope_start_byte, target.to_string()))
        .copied()
    else {
        return;
    };
    let Some(node) = nodes.get_mut(index) else {
        return;
    };
    let registry = widget_adapter_registry();
    let asset_setter = registry
        .find_by_node_type(node.node_type)
        .and_then(|adapter| {
            adapter.asset_slots.iter().find_map(|slot| {
                slot.setters
                    .iter()
                    .find(|setter| setter.method == call.method)
                    .map(|setter| (*slot, *setter))
            })
        });
    if let Some((slot, setter)) = asset_setter {
        update_asset_string(node, &slot, &call.arguments, setter.argument_index);
        finish_setter(node, &call);
        return;
    }
    match call.method.as_str() {
        "setPosition" => {
            update_number(node, "x", &call.arguments, 1);
            update_number(node, "y", &call.arguments, 2);
        }
        "setContentSize" | "_setContentSize" => {
            update_number(node, "width", &call.arguments, 1);
            update_number(node, "height", &call.arguments, 2);
        }
        "setAnchorPoint" => {
            update_number(node, "anchorX", &call.arguments, 1);
            update_number(node, "anchorY", &call.arguments, 2);
        }
        "setVisible" => update_bool(node, "visible", &call.arguments, 1),
        "setScale" => {
            let value = number_argument(&call.arguments, 1, 1.0);
            bind_span(&mut node.source_binding.property_spans, "scaleX", &value);
            bind_span(&mut node.source_binding.property_spans, "scaleY", &value);
            node.transform.scale_x = value.clone();
            node.transform.scale_y = value;
        }
        "setScaleX" => update_number(node, "scaleX", &call.arguments, 1),
        "setScaleY" => update_number(node, "scaleY", &call.arguments, 1),
        "setRotation" => update_number(node, "rotation", &call.arguments, 1),
        "setRotationSkewX" => update_number(node, "skewX", &call.arguments, 1),
        "setRotationSkewY" => update_number(node, "skewY", &call.arguments, 1),
        "setOpacity" => update_number(node, "opacity", &call.arguments, 1),
        "setTag" => update_number(node, "tag", &call.arguments, 1),
        "setTouchEnabled" | "setMouseEnabled" => {
            update_generic_bool(node, "touchEnabled", &call.arguments, 1)
        }
        "setChineseName" => update_generic_string(node, "chineseName", &call.arguments, 1),
        "setIgnoreContentAdaptWithSize" => {
            update_bool(node, "ignoreContentAdaptWithSize", &call.arguments, 1)
        }
        "Layout_setClippingEnabled"
        | "ListView_setClippingEnabled"
        | "ScrollView_setClippingEnabled" => {
            update_bool(node, "clippingEnabled", &call.arguments, 1)
        }
        "Text_setString"
        | "TextAtlas_setString"
        | "TextInput_setString"
        | "Button_setTitleText" => update_string(node, "text", &call.arguments, 1),
        "Button_setTitleFontSize" => update_number(node, "fontSize", &call.arguments, 1),
        "Button_setTitleColor" => update_string(node, "color", &call.arguments, 1),
        "TextInput_setFontColor" => update_string(node, "color", &call.arguments, 1),
        "TextInput_setPlaceholderFontColor" => {
            update_generic_string(node, "placeholderColor", &call.arguments, 1)
        }
        "Text_setTextAreaSize" => {
            update_number(node, "width", &call.arguments, 1);
            update_number(node, "height", &call.arguments, 2);
        }
        "Image_setScale9Slice"
        | "Button_setScale9Slice"
        | "Layout_setBackGroundImageScale9Slice"
        | "ListView_setBackGroundImageScale9Slice" => update_scale9(node, &call.arguments),
        "Image_setScale9Enabled" | "Layout_setBackGroundImageScale9Enabled" => {
            update_bool(node, "scale9Enabled", &call.arguments, 1)
        }
        "ListView_setGravity" => update_number(node, "gravity", &call.arguments, 1),
        "ListView_setItemsMargin" => update_number(node, "itemsMargin", &call.arguments, 1),
        "ListView_setBounceEnabled" | "ScrollView_setBounceEnabled" => {
            update_generic_bool(node, "bounceEnabled", &call.arguments, 1)
        }
        "ScrollView_setInnerContainerSize" => {
            update_number(node, "innerWidth", &call.arguments, 1);
            update_number(node, "innerHeight", &call.arguments, 2);
        }
        "TextInput_setPlaceHolder" => {
            update_generic_string(node, "placeholder", &call.arguments, 1)
        }
        "TextInput_setInputMode" => update_generic_number(node, "inputMode", &call.arguments, 1),
        "TextInput_setMaxLength" => update_generic_number(node, "maxLength", &call.arguments, 1),
        "Text_setTextHorizontalAlignment"
        | "TextInput_setTextHorizontalAlignment"
        | "Text_setTextVerticalAlignment"
        | "TextInput_setTextVerticalAlignment" => {
            update_generic_number(node, &call.method, &call.arguments, 1)
        }
        "Slider_setPercent" | "LoadingBar_setPercent" | "ProgressTimer_setPercentage" => {
            update_generic_number(node, "percent", &call.arguments, 1)
        }
        "LoadingBar_setColor" => update_generic_string(node, "progressColor", &call.arguments, 1),
        "Text_enableOutline" | "Button_titleEnableOutline" => {
            update_generic_bool_literal(node, "outlineEnabled", true);
            update_generic_string(node, "outlineColor", &call.arguments, 1);
            update_generic_number(node, "outlineSize", &call.arguments, 2);
        }
        "Text_disableOutLine" | "Button_titleDisableOutLine" => {
            update_generic_bool_literal(node, "outlineEnabled", false)
        }
        "CheckBox_setSelected" => update_generic_bool(node, "selected", &call.arguments, 1),
        _ => return,
    }
    finish_setter(node, &call);
}

fn finish_setter(node: &mut Mir3UiNode, call: &GuiCall) {
    if node.node_type != Mir3UiNodeType::Unsupported && node_has_dynamic(node) {
        node.compatibility.status = CompatibilityStatus::Dynamic;
        node.compatibility.reason = Some("包含动态 Lua 属性；动态 token 保持只读".to_string());
        node.compatibility.reason_code = Some("dynamic_property".to_string());
    }
    node.source_binding.insert_byte = node
        .source_binding
        .insert_byte
        .max(call.statement_span.end_byte);
}

fn node_has_dynamic(node: &Mir3UiNode) -> bool {
    [
        node.name.source,
        node.position.x.source,
        node.position.y.source,
        node.size.width.source,
        node.size.height.source,
        node.anchor.x.source,
        node.anchor.y.source,
        node.visible.source,
        node.text.source,
        node.image.source,
        node.pressed_image.source,
        node.disabled_image.source,
        node.font_size.source,
        node.color.source,
        node.opacity.source,
        node.tag.source,
        node.transform.scale_x.source,
        node.transform.scale_y.source,
        node.transform.rotation.source,
        node.transform.skew_x.source,
        node.transform.skew_y.source,
        node.ignore_content_adapt_with_size.source,
        node.clipping_enabled.source,
        node.scale9.enabled.source,
        node.scale9.left.source,
        node.scale9.bottom.source,
        node.scale9.right.source,
        node.scale9.top.source,
        node.container.direction.source,
        node.container.gravity.source,
        node.container.items_margin.source,
        node.container.inner_width.source,
        node.container.inner_height.source,
    ]
    .contains(&BoundValueSource::Dynamic)
        || node
            .asset_slots
            .values()
            .any(|value| value.source == BoundValueSource::Dynamic)
        || node
            .properties
            .values()
            .any(|value| value.source == BoundValueSource::Dynamic)
}

fn update_number(node: &mut Mir3UiNode, property: &str, arguments: &[CallArgument], index: usize) {
    let value = number_argument(arguments, index, 0.0);
    bind_span(&mut node.source_binding.property_spans, property, &value);
    node.properties.insert(
        property.to_string(),
        map_bound_value(value.clone(), Mir3UiPropertyValue::Number),
    );
    match property {
        "x" => node.position.x = value,
        "y" => node.position.y = value,
        "width" => node.size.width = value,
        "height" => node.size.height = value,
        "anchorX" => node.anchor.x = value,
        "anchorY" => node.anchor.y = value,
        "opacity" => node.opacity = value,
        "tag" => node.tag = value,
        "fontSize" => node.font_size = value,
        "scaleX" => node.transform.scale_x = value,
        "scaleY" => node.transform.scale_y = value,
        "rotation" => node.transform.rotation = value,
        "skewX" => node.transform.skew_x = value,
        "skewY" => node.transform.skew_y = value,
        "scale9Left" => node.scale9.left = value,
        "scale9Bottom" => node.scale9.bottom = value,
        "scale9Right" => node.scale9.right = value,
        "scale9Top" => node.scale9.top = value,
        "gravity" => node.container.gravity = value,
        "itemsMargin" => node.container.items_margin = value,
        "innerWidth" => node.container.inner_width = value,
        "innerHeight" => node.container.inner_height = value,
        _ => {}
    }
}

fn update_string(node: &mut Mir3UiNode, property: &str, arguments: &[CallArgument], index: usize) {
    let value = string_argument(arguments, index, String::new());
    bind_span(&mut node.source_binding.property_spans, property, &value);
    node.properties.insert(
        property.to_string(),
        map_bound_value(value.clone(), Mir3UiPropertyValue::String),
    );
    match property {
        "text" => node.text = value,
        "image" => node.image = value,
        "pressedImage" => node.pressed_image = value,
        "disabledImage" => node.disabled_image = value,
        "color" => node.color = value,
        _ => {}
    }
}

fn update_asset_string(
    node: &mut Mir3UiNode,
    slot: &AdapterAssetSlotBinding,
    arguments: &[CallArgument],
    index: usize,
) {
    let value = string_argument(arguments, index, String::new());
    bind_span(
        &mut node.source_binding.property_spans,
        slot.property,
        &value,
    );
    node.properties.insert(
        slot.property.to_string(),
        map_bound_value(value.clone(), Mir3UiPropertyValue::String),
    );
    node.asset_slots
        .insert(slot.slot.to_string(), value.clone());
    if slot.primary {
        node.image = value.clone();
        bind_span(&mut node.source_binding.property_spans, "image", &value);
    }
    match slot.slot {
        "pressed" => node.pressed_image = value,
        "disabled" => node.disabled_image = value,
        _ => {}
    }
}

fn update_bool(node: &mut Mir3UiNode, property: &str, arguments: &[CallArgument], index: usize) {
    let value = bool_argument(arguments, index, true);
    bind_span(&mut node.source_binding.property_spans, property, &value);
    node.properties.insert(
        property.to_string(),
        map_bound_value(value.clone(), Mir3UiPropertyValue::Boolean),
    );
    match property {
        "visible" => node.visible = value,
        "ignoreContentAdaptWithSize" => node.ignore_content_adapt_with_size = value,
        "clippingEnabled" => node.clipping_enabled = value,
        "scale9Enabled" => node.scale9.enabled = value,
        _ => {}
    }
}

fn update_scale9(node: &mut Mir3UiNode, arguments: &[CallArgument]) {
    update_number(node, "scale9Left", arguments, 1);
    update_number(node, "scale9Bottom", arguments, 2);
    update_number(node, "scale9Right", arguments, 3);
    update_number(node, "scale9Top", arguments, 4);
    node.scale9.enabled = BoundValue::default(true);
}

fn update_generic_number(
    node: &mut Mir3UiNode,
    property: &str,
    arguments: &[CallArgument],
    index: usize,
) {
    let value = number_argument(arguments, index, 0.0);
    bind_span(&mut node.source_binding.property_spans, property, &value);
    node.properties.insert(
        property.to_string(),
        map_bound_value(value, Mir3UiPropertyValue::Number),
    );
}

fn update_generic_string(
    node: &mut Mir3UiNode,
    property: &str,
    arguments: &[CallArgument],
    index: usize,
) {
    let value = string_argument(arguments, index, String::new());
    bind_span(&mut node.source_binding.property_spans, property, &value);
    node.properties.insert(
        property.to_string(),
        map_bound_value(value, Mir3UiPropertyValue::String),
    );
}

fn update_generic_bool(
    node: &mut Mir3UiNode,
    property: &str,
    arguments: &[CallArgument],
    index: usize,
) {
    let value = bool_argument(arguments, index, false);
    bind_span(&mut node.source_binding.property_spans, property, &value);
    node.properties.insert(
        property.to_string(),
        map_bound_value(value, Mir3UiPropertyValue::Boolean),
    );
}

fn update_generic_bool_literal(node: &mut Mir3UiNode, property: &str, value: bool) {
    node.properties.insert(
        property.to_string(),
        BoundValue::default(Mir3UiPropertyValue::Boolean(value)),
    );
}

fn map_bound_value<T, U>(value: BoundValue<T>, mapper: impl FnOnce(T) -> U) -> BoundValue<U> {
    BoundValue {
        value: mapper(value.value),
        source: value.source,
        writable: value.writable,
        original_token: value.original_token,
        span: value.span,
    }
}

fn number_argument(arguments: &[CallArgument], index: usize, default: f64) -> BoundValue<f64> {
    let Some(argument) = arguments.get(index) else {
        return BoundValue::default(default);
    };
    match argument.text.trim().parse::<f64>() {
        Ok(value) => BoundValue::literal(value, argument.text.clone(), argument.span),
        Err(_) => BoundValue::dynamic(default, argument.text.clone(), argument.span),
    }
}

fn bool_argument(arguments: &[CallArgument], index: usize, default: bool) -> BoundValue<bool> {
    let Some(argument) = arguments.get(index) else {
        return BoundValue::default(default);
    };
    match argument.text.trim() {
        "true" => BoundValue::literal(true, argument.text.clone(), argument.span),
        "false" => BoundValue::literal(false, argument.text.clone(), argument.span),
        _ => BoundValue::dynamic(default, argument.text.clone(), argument.span),
    }
}

fn string_argument(
    arguments: &[CallArgument],
    index: usize,
    default: String,
) -> BoundValue<String> {
    let Some(argument) = arguments.get(index) else {
        return BoundValue::default(default);
    };
    match lua_string(&argument.text) {
        Some(value) => BoundValue::literal(value, argument.text.clone(), argument.span),
        None => BoundValue::dynamic(default, argument.text.clone(), argument.span),
    }
}

fn lua_string(token: &str) -> Option<String> {
    let trimmed = token.trim();
    if let Some(value) = lua_long_string(trimmed) {
        return Some(value);
    }
    if trimmed.len() >= 2 {
        let quote = trimmed.as_bytes()[0];
        if (quote == b'\'' || quote == b'\"') && trimmed.as_bytes()[trimmed.len() - 1] == quote {
            let body = &trimmed[1..trimmed.len() - 1];
            return Some(unescape_lua_string(body));
        }
    }
    None
}

fn lua_long_string(value: &str) -> Option<String> {
    if !value.starts_with('[') || value.len() < 4 {
        return None;
    }
    let equals = value.as_bytes()[1..]
        .iter()
        .take_while(|byte| **byte == b'=')
        .count();
    let opening_length = equals + 2;
    if value.as_bytes().get(opening_length - 1) != Some(&b'[') {
        return None;
    }
    let closing = format!("]{}]", "=".repeat(equals));
    if value.len() < opening_length + closing.len() || !value.ends_with(&closing) {
        return None;
    }
    Some(value[opening_length..value.len() - closing.len()].to_string())
}

fn unescape_lua_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match chars.next() {
            Some('n') => output.push('\n'),
            Some('r') => output.push('\r'),
            Some('t') => output.push('\t'),
            Some('\\') => output.push('\\'),
            Some('\"') => output.push('\"'),
            Some('\'') => output.push('\''),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }
    output
}

fn bind_span<T>(spans: &mut BTreeMap<String, SourceSpan>, property: &str, value: &BoundValue<T>) {
    if let Some(span) = value.span {
        spans.insert(property.to_string(), span);
    }
}

fn collect_assets(nodes: &[Mir3UiNode]) -> Vec<Mir3UiAsset> {
    let mut assets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for node in nodes {
        for value in node.asset_slots.values() {
            if !value.value.trim().is_empty() {
                assets
                    .entry(value.value.clone())
                    .or_default()
                    .push(node.id.clone());
            }
        }
        for value in [&node.image, &node.pressed_image, &node.disabled_image] {
            if !value.value.trim().is_empty() {
                assets
                    .entry(value.value.clone())
                    .or_default()
                    .push(node.id.clone());
            }
        }
        for (property, value) in &node.properties {
            let lower = property.to_ascii_lowercase();
            let is_asset = lower.contains("image")
                || lower.contains("texture")
                || lower.contains("asset")
                || lower.contains("file");
            if !is_asset {
                continue;
            }
            if let Mir3UiPropertyValue::String(path) = &value.value {
                if !path.trim().is_empty() {
                    assets
                        .entry(path.clone())
                        .or_default()
                        .push(node.id.clone());
                }
            }
        }
    }
    assets
        .into_iter()
        .map(|(logical_path, mut node_ids)| {
            node_ids.sort();
            node_ids.dedup();
            Mir3UiAsset {
                logical_path,
                node_ids,
            }
        })
        .collect()
}

fn collect_syntax_diagnostics(node: Node<'_>, diagnostics: &mut Vec<Mir3UiDiagnostic>) {
    if diagnostics.len() >= 100 {
        return;
    }
    if node.is_error() || node.is_missing() {
        diagnostics.push(Mir3UiDiagnostic {
            severity: DiagnosticSeverity::Error,
            code: "GUI_LUA_SYNTAX_ERROR".to_string(),
            message: format!("Lua 语法节点异常：{}", node.kind()),
            span: Some(span(node)),
            node_id: None,
        });
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_syntax_diagnostics(child, diagnostics);
    }
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    source.get(node.byte_range())
}

fn span(node: Node<'_>) -> SourceSpan {
    SourceSpan {
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        start: point(node.start_position()),
        end: point(node.end_position()),
    }
}

fn point(value: Point) -> SourcePoint {
    SourcePoint {
        row: value.row,
        column: value.column,
    }
}
