use crate::{
    BoundValue, BoundValueSource, CompatibilityStatus, DiagnosticSeverity, Mir3UiAsset,
    Mir3UiCompatibility, Mir3UiDiagnostic, Mir3UiDocument, Mir3UiNode, Mir3UiNodeType, Mir3UiPoint,
    Mir3UiSize, Mir3UiSource, Mir3UiViewport, SourceBinding, SourcePoint, SourceSpan,
    MIR3_UI_SCHEMA_VERSION,
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
    let raw_type = call.method.trim_end_matches("_Create");
    let node_type = match raw_type {
        "Layout" => Mir3UiNodeType::Panel,
        "Image" => Mir3UiNodeType::Image,
        "Text" => Mir3UiNodeType::Text,
        "Button" => Mir3UiNodeType::Button,
        "Node" => Mir3UiNodeType::Node,
        _ => Mir3UiNodeType::Unsupported,
    };
    let id = format!("node-{}", call.call_span.start_byte);
    let parent_variable = call.arguments.first().map(|argument| argument.text.trim());
    let parent_id = parent_variable
        .and_then(|name| symbols.get(&(call.scope_start_byte, name.to_string())))
        .and_then(|index| nodes.get(*index))
        .map(|node| node.id.clone());
    let missing_parent = parent_id.is_none()
        && parent_variable.is_some_and(|name| !matches!(name, "parent" | "nil" | "_parent"));
    let supported = node_type != Mir3UiNodeType::Unsupported;
    let name = string_argument(&call.arguments, 1, String::new());
    let x = number_argument(&call.arguments, 2, 0.0);
    let y = number_argument(&call.arguments, 3, 0.0);
    let (width, height) = if node_type == Mir3UiNodeType::Panel {
        (
            number_argument(&call.arguments, 4, 0.0),
            number_argument(&call.arguments, 5, 0.0),
        )
    } else {
        (BoundValue::default(0.0), BoundValue::default(0.0))
    };
    let image = if matches!(node_type, Mir3UiNodeType::Image | Mir3UiNodeType::Button) {
        string_argument(&call.arguments, 4, String::new())
    } else {
        BoundValue::default(String::new())
    };
    let font_size = if node_type == Mir3UiNodeType::Text {
        number_argument(&call.arguments, 4, 14.0)
    } else {
        BoundValue::default(14.0)
    };
    let color = if node_type == Mir3UiNodeType::Text {
        string_argument(&call.arguments, 5, "#ffffff".to_string())
    } else {
        BoundValue::default(String::new())
    };
    let text = if node_type == Mir3UiNodeType::Text {
        string_argument(&call.arguments, 6, String::new())
    } else {
        BoundValue::default(String::new())
    };
    let has_dynamic = [name.source, x.source, y.source, width.source, height.source]
        .contains(&BoundValueSource::Dynamic)
        || [image.source, text.source, font_size.source, color.source]
            .contains(&BoundValueSource::Dynamic);
    let reason = if !supported {
        Some(format!("V0.1 不渲染 GUI:{raw_type}_Create"))
    } else if missing_parent {
        Some(format!(
            "无法静态解析父节点 {}",
            parent_variable.unwrap_or_default()
        ))
    } else if has_dynamic {
        Some("包含动态 Lua 属性；动态 token 保持只读".to_string())
    } else {
        None
    };
    let status = if !supported {
        CompatibilityStatus::Unsupported
    } else if missing_parent || has_dynamic {
        CompatibilityStatus::Partial
    } else {
        CompatibilityStatus::Supported
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
        pressed_image: BoundValue::default(String::new()),
        disabled_image: BoundValue::default(String::new()),
        font_size,
        color,
        opacity: BoundValue::default(255.0),
        tag: BoundValue::default(0.0),
        compatibility: Mir3UiCompatibility { status, reason },
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
        "setOpacity" => update_number(node, "opacity", &call.arguments, 1),
        "setTag" => update_number(node, "tag", &call.arguments, 1),
        "Text_setString" | "TextInput_setString" | "Button_setTitleText" => {
            update_string(node, "text", &call.arguments, 1)
        }
        "Image_loadTexture" | "Layout_setBackGroundImage" => {
            update_string(node, "image", &call.arguments, 1)
        }
        "Button_loadTextureNormal" => update_string(node, "image", &call.arguments, 1),
        "Button_loadTexturePressed" => update_string(node, "pressedImage", &call.arguments, 1),
        "Button_loadTextureDisabled" => update_string(node, "disabledImage", &call.arguments, 1),
        "Button_setTitleFontSize" => update_number(node, "fontSize", &call.arguments, 1),
        "Button_setTitleColor" => update_string(node, "color", &call.arguments, 1),
        _ => return,
    }
    if node.compatibility.status == CompatibilityStatus::Supported && node_has_dynamic(node) {
        node.compatibility.status = CompatibilityStatus::Partial;
        node.compatibility.reason = Some("包含动态 Lua 属性；动态 token 保持只读".to_string());
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
    ]
    .contains(&BoundValueSource::Dynamic)
}

fn update_number(node: &mut Mir3UiNode, property: &str, arguments: &[CallArgument], index: usize) {
    let value = number_argument(arguments, index, 0.0);
    bind_span(&mut node.source_binding.property_spans, property, &value);
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
        _ => {}
    }
}

fn update_string(node: &mut Mir3UiNode, property: &str, arguments: &[CallArgument], index: usize) {
    let value = string_argument(arguments, index, String::new());
    bind_span(&mut node.source_binding.property_spans, property, &value);
    match property {
        "text" => node.text = value,
        "image" => node.image = value,
        "pressedImage" => node.pressed_image = value,
        "disabledImage" => node.disabled_image = value,
        "color" => node.color = value,
        _ => {}
    }
}

fn update_bool(node: &mut Mir3UiNode, property: &str, arguments: &[CallArgument], index: usize) {
    let value = bool_argument(arguments, index, true);
    bind_span(&mut node.source_binding.property_spans, property, &value);
    if property == "visible" {
        node.visible = value;
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
        for value in [&node.image, &node.pressed_image, &node.disabled_image] {
            if !value.value.trim().is_empty() {
                assets
                    .entry(value.value.clone())
                    .or_default()
                    .push(node.id.clone());
            }
        }
    }
    assets
        .into_iter()
        .map(|(logical_path, node_ids)| Mir3UiAsset {
            logical_path,
            node_ids,
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
