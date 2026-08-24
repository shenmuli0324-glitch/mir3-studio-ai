use crate::{
    CoreNodeType, InsertCoreNodeRequest, Mir3UiDocument, SourceEdit, SourcePoint, SourceSpan,
};
use std::collections::HashSet;

/// 按原始字节区间应用非重叠编辑；未命中的源码字节保持原样。
pub fn apply_source_edits(source: &str, edits: &[SourceEdit]) -> Result<String, String> {
    let mut ordered = edits.to_vec();
    ordered.sort_by_key(|edit| (edit.span.start_byte, edit.span.end_byte));
    let mut previous_end = 0usize;
    for edit in &ordered {
        if edit.span.start_byte < previous_end {
            return Err("GUI_SOURCE_EDIT_OVERLAP: source edits must not overlap".to_string());
        }
        if edit.span.start_byte > edit.span.end_byte || edit.span.end_byte > source.len() {
            return Err(
                "GUI_SOURCE_EDIT_RANGE_INVALID: source edit is outside the document".to_string(),
            );
        }
        if !source.is_char_boundary(edit.span.start_byte)
            || !source.is_char_boundary(edit.span.end_byte)
        {
            return Err(
                "GUI_SOURCE_EDIT_UTF8_BOUNDARY: source edit splits a UTF-8 character".to_string(),
            );
        }
        previous_end = edit.span.end_byte;
    }

    let mut output = String::with_capacity(source.len());
    let mut cursor = 0usize;
    for edit in ordered {
        output.push_str(&source[cursor..edit.span.start_byte]);
        output.push_str(&edit.replacement);
        cursor = edit.span.end_byte;
    }
    output.push_str(&source[cursor..]);
    Ok(output)
}

/// 使用 DOM 内记录的 token span 精准替换单个可写属性。
pub fn replace_bound_property(
    source: &str,
    document: &Mir3UiDocument,
    node_id: &str,
    property: &str,
    replacement: &str,
) -> Result<String, String> {
    let node = document
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| format!("GUI_NODE_NOT_FOUND: {node_id}"))?;
    let span = node
        .source_binding
        .property_spans
        .get(property)
        .copied()
        .ok_or_else(|| format!("GUI_PROPERTY_NOT_BOUND: {node_id}.{property}"))?;
    let writable = match property {
        "x" => node.position.x.writable,
        "y" => node.position.y.writable,
        "width" => node.size.width.writable,
        "height" => node.size.height.writable,
        "anchorX" => node.anchor.x.writable,
        "anchorY" => node.anchor.y.writable,
        "visible" => node.visible.writable,
        "text" => node.text.writable,
        "image" => node.image.writable,
        "pressedImage" => node.pressed_image.writable,
        "disabledImage" => node.disabled_image.writable,
        "fontSize" => node.font_size.writable,
        "color" => node.color.writable,
        "opacity" => node.opacity.writable,
        "tag" => node.tag.writable,
        "name" => node.name.writable,
        _ => false,
    };
    if !writable {
        return Err(format!("GUI_PROPERTY_DYNAMIC: {node_id}.{property}"));
    }
    apply_source_edits(
        source,
        &[SourceEdit {
            span,
            replacement: replacement.to_string(),
        }],
    )
}

/// 生成不依赖运行时数据的标准 996 GUIExport 页面。
pub fn generate_template(newline: &str) -> String {
    let nl = normalized_newline(newline);
    [
        "local ui = {}",
        "local FUNCQUEUE = {}",
        "local TAGOBJ = {}",
        "",
        "function ui.init(parent, __data__, __update__)",
        "    local Scene = GUI:Node_Create(parent, \"Scene\", 0, 0)",
        "    GUI:setTag(Scene, 0)",
        "",
        "    return Scene",
        "end",
        "",
        "function ui.update(__data__)",
        "end",
        "",
        "return ui",
        "",
    ]
    .join(nl)
}

/// 在已验证的父节点语句之后插入 V0.1 核心控件源码。
pub fn insert_core_node(
    source: &str,
    document: &Mir3UiDocument,
    parent_node_id: &str,
    request: &InsertCoreNodeRequest,
) -> Result<String, String> {
    validate_gui_name(&request.name)?;
    let parent = document
        .nodes
        .iter()
        .find(|node| node.id == parent_node_id)
        .ok_or_else(|| format!("GUI_PARENT_NOT_FOUND: {parent_node_id}"))?;
    let offset = parent.source_binding.insert_byte;
    if offset > source.len() || !source.is_char_boundary(offset) {
        return Err("GUI_INSERT_POINT_INVALID: parent insertion point is invalid".to_string());
    }
    let variables: HashSet<&str> = document
        .nodes
        .iter()
        .map(|node| node.lua_variable.as_str())
        .collect();
    let base = variable_name(&request.name);
    let variable = unique_variable(&base, &variables);
    let x = format_number(request.x);
    let y = format_number(request.y);
    let quoted_name = quote_lua_string(&request.name);
    let statement = match request.node_type {
        CoreNodeType::Panel => format!(
            "local {variable} = GUI:Layout_Create({}, {quoted_name}, {x}, {y}, 100, 100, false)",
            parent.lua_variable
        ),
        CoreNodeType::Image => format!(
            "local {variable} = GUI:Image_Create({}, {quoted_name}, {x}, {y}, \"\")",
            parent.lua_variable
        ),
        CoreNodeType::Text => format!(
            "local {variable} = GUI:Text_Create({}, {quoted_name}, {x}, {y}, 14, \"#ffffff\", [[]])",
            parent.lua_variable
        ),
        CoreNodeType::Button => format!(
            "local {variable} = GUI:Button_Create({}, {quoted_name}, {x}, {y}, \"\")",
            parent.lua_variable
        ),
    };
    let nl = normalized_newline(&document.source.newline);
    let indentation = line_indentation(source, parent.source_binding.statement.start_byte);
    let insertion = format!("{nl}{indentation}{statement}");
    let point = source_point(source, offset);
    apply_source_edits(
        source,
        &[SourceEdit {
            span: SourceSpan {
                start_byte: offset,
                end_byte: offset,
                start: point,
                end: point,
            },
            replacement: insertion,
        }],
    )
}

fn normalized_newline(newline: &str) -> &str {
    match newline {
        "\r\n" => "\r\n",
        "\r" => "\r",
        _ => "\n",
    }
}

fn validate_gui_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("GUI_NODE_NAME_EMPTY: node name is required".to_string());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("GUI_NODE_NAME_INVALID: node name contains control characters".to_string());
    }
    Ok(())
}

fn variable_name(name: &str) -> String {
    let mut output = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            output.push(character);
        } else {
            output.push('_');
        }
    }
    if output.is_empty() || output.as_bytes()[0].is_ascii_digit() {
        output.insert_str(0, "Node_");
    }
    output
}

fn unique_variable(base: &str, variables: &HashSet<&str>) -> String {
    if !variables.contains(base) {
        return base.to_string();
    }
    for suffix in 2..usize::MAX {
        let candidate = format!("{base}_{suffix}");
        if !variables.contains(candidate.as_str()) {
            return candidate;
        }
    }
    format!("{base}_new")
}

fn quote_lua_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('\"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn line_indentation(source: &str, byte: usize) -> &str {
    let line_start = source[..byte.min(source.len())]
        .rfind(['\n', '\r'])
        .map_or(0, |index| index + 1);
    let line = &source[line_start..byte.min(source.len())];
    let length = line
        .bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    &line[..length]
}

fn source_point(source: &str, byte: usize) -> SourcePoint {
    let prefix = &source[..byte];
    let row = prefix.bytes().filter(|value| *value == b'\n').count();
    let column = prefix
        .rfind('\n')
        .map_or(prefix.len(), |position| prefix.len() - position - 1);
    SourcePoint { row, column }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_document;

    #[test]
    fn template_uses_requested_newline() {
        let source = generate_template("\r\n");
        assert!(source.contains("local ui = {}\r\nlocal FUNCQUEUE"));
        assert!(!source.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn applies_disjoint_edits_without_touching_middle_source() {
        let source = "abcdef";
        let point = SourcePoint { row: 0, column: 0 };
        let output = apply_source_edits(
            source,
            &[
                SourceEdit {
                    span: SourceSpan {
                        start_byte: 1,
                        end_byte: 2,
                        start: point,
                        end: point,
                    },
                    replacement: "B".to_string(),
                },
                SourceEdit {
                    span: SourceSpan {
                        start_byte: 4,
                        end_byte: 5,
                        start: point,
                        end: point,
                    },
                    replacement: "E".to_string(),
                },
            ],
        )
        .unwrap();
        assert_eq!(output, "aBcdEf");
    }

    #[test]
    fn inserts_a_core_node_after_parent_binding() {
        let source = generate_template("\n");
        let document = parse_document(&source, "GUIExport/new.lua", "sha", "utf-8", "\n").unwrap();
        let root = document.roots.first().unwrap();
        let output = insert_core_node(
            &source,
            &document,
            root,
            &InsertCoreNodeRequest {
                node_type: CoreNodeType::Panel,
                name: "Panel_1".to_string(),
                x: 10.0,
                y: 20.0,
            },
        )
        .unwrap();
        assert!(output.contains("GUI:Layout_Create(Scene, \"Panel_1\", 10, 20, 100, 100, false)"));
    }
}
