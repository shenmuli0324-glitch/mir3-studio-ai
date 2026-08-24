#![cfg_attr(windows, windows_subsystem = "windows")]

//! MIR3 领域 MCP STDIO 服务。
//!
//! 仅暴露 996 项目状态、领域索引、知识、Draft 与校验，不重复 Harness 的通用
//! 文件读取、搜索、编辑器或会话能力。

use mir3_domain::{DomainStore, DraftChangeInput, IndexQuery, KnowledgeStatus, SafeTextPatch};
use serde_json::{json, Value};
use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[cfg(test)]
const TOOLS: [&str; 8] = [
    "mir3_project_status",
    "mir3_index_query",
    "mir3_knowledge_search",
    "mir3_knowledge_get",
    "mir3_draft_open",
    "mir3_draft_patch",
    "mir3_draft_diff",
    "mir3_validate",
];

fn main() {
    if let Err(error) = run() {
        eprintln!("MIR3_MCP_FATAL: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let home = env::var_os("MIR3_STUDIO_HOME")
        .or_else(|| env::var_os("DSH_HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| "MIR3_MCP_HOME_MISSING: MIR3_STUDIO_HOME is required".to_string())?;
    let project_id = env::var("MIR3_ACTIVE_PROJECT_ID")
        .map_err(|_| "MIR3_MCP_PROJECT_MISSING: MIR3_ACTIVE_PROJECT_ID is required".to_string())?;
    let store = DomainStore::new(home.join("projects"))?;
    store.get_project(&project_id)?;

    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| format!("MIR3_MCP_READ_FAILED: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                write_json(
                    &mut stdout,
                    &rpc_error(Value::Null, -32700, &format!("parse error: {error}")),
                )?;
                continue;
            }
        };
        let Some(response) = handle_request(&store, &project_id, &request) else {
            continue;
        };
        write_json(&mut stdout, &response)?;
    }
    Ok(())
}

fn handle_request(store: &DomainStore, project_id: &str, request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    if id.is_none() {
        return None;
    }
    let id = id.unwrap_or(Value::Null);
    match method {
        "initialize" => {
            let version = request
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2024-11-05");
            Some(rpc_result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "MIR3 Studio AI Domain", "version": env!("CARGO_PKG_VERSION")}
                }),
            ))
        }
        "ping" => Some(rpc_result(id, json!({}))),
        "tools/list" => Some(rpc_result(id, json!({"tools": tool_definitions()}))),
        "tools/call" => {
            let name = request
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let arguments = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = call_tool(store, project_id, name, arguments);
            Some(rpc_result(id, result))
        }
        _ => Some(rpc_error(id, -32601, "method not found")),
    }
}

fn call_tool(store: &DomainStore, project_id: &str, name: &str, args: Value) -> Value {
    let result = match name {
        "mir3_project_status" => store.get_project(project_id).and_then(|project| {
            let stats = store.index_stats(project_id)?;
            Ok(json!({"project": project, "index": stats}))
        }),
        "mir3_index_query" => serde_json::from_value::<IndexQuery>(args)
            .map_err(|e| format!("MCP_ARGUMENT_INVALID: {e}"))
            .and_then(|query| store.query_index(project_id, &query))
            .map(|records| json!({"records": records})),
        "mir3_knowledge_search" => {
            let text = args.get("text").and_then(Value::as_str).unwrap_or("");
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(20)
                .clamp(1, 100) as usize;
            store
                .search_active_knowledge(project_id, text, limit)
                .map(|records| json!({"records": records}))
        }
        "mir3_knowledge_get" => required_string(&args, "knowledgeId")
            .and_then(|id| store.get_knowledge(project_id, &id))
            .and_then(|record| {
                if record.status == KnowledgeStatus::Active {
                    Ok(json!({"record": record}))
                } else {
                    Err("KNOWLEDGE_NOT_ACTIVE: MCP can only read ACTIVE knowledge".to_string())
                }
            }),
        "mir3_draft_open" => required_string(&args, "intent")
            .and_then(|intent| store.open_draft(project_id, &intent))
            .map(|draft| json!({"draft": draft})),
        "mir3_draft_patch" => {
            let draft_id = required_string(&args, "draftId");
            let revision = args
                .get("expectedRevision")
                .and_then(Value::as_i64)
                .ok_or_else(|| "MCP_ARGUMENT_INVALID: expectedRevision is required".to_string());
            draft_id
                .and_then(|draft_id| revision.map(|revision| (draft_id, revision)))
                .and_then(|(draft_id, revision)| {
                    if let Some(operation) = args.get("operation") {
                        return apply_safe_operation(
                            store,
                            project_id,
                            &draft_id,
                            revision,
                            operation,
                        );
                    }
                    let changes = args
                        .get("changes")
                        .cloned()
                        .ok_or_else(|| "MCP_ARGUMENT_INVALID: changes or operation is required".to_string())
                        .and_then(|value| {
                            serde_json::from_value::<Vec<DraftChangeInput>>(value)
                                .map_err(|e| format!("MCP_ARGUMENT_INVALID: {e}"))
                        })?;
                    if changes.iter().any(|change| is_protected_path(&change.path)) {
                        return Err("MIR3_SAFE_OPERATION_REQUIRED: TXT/Lua/XLS changes must use the operation field".to_string());
                    }
                    store
                        .patch_draft(project_id, &draft_id, revision, &changes)
                        .map(|preview| json!({"preview": preview}))
                })
        }
        "mir3_draft_diff" => required_string(&args, "draftId")
            .and_then(|draft_id| store.preview_draft(project_id, &draft_id))
            .map(|preview| json!({"preview": preview})),
        "mir3_validate" => store.validate_project(project_id).and_then(|project| {
            let draft = args.get("draftId").and_then(Value::as_str);
            let draft_preview = draft
                .map(|draft_id| store.preview_draft(project_id, draft_id))
                .transpose()?;
            Ok(json!({
                "valid": project.status != mir3_domain::ProjectStatus::Missing,
                "warnings": project.warnings,
                "project": project,
                "draft": draft_preview
            }))
        }),
        _ => Err(format!("MCP_TOOL_UNKNOWN: {name}")),
    };
    match result {
        Ok(value) => tool_success(value),
        Err(error) => tool_failure(&error),
    }
}

fn tool_definitions() -> Vec<Value> {
    vec![
        tool("mir3_project_status", "返回当前 996 项目的角色、版本、Workspace 与索引状态。", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("mir3_index_query", "查询 Map、NPC、Monster、Item、Quest、Lua、Config 等领域索引；通用文件搜索请使用 Harness 原生工具。", json!({"type":"object","properties":{"text":{"type":"string"},"categories":{"type":"array","items":{"type":"string","enum":["Map","NPC","Monster","Item","Quest","Lua","Config","Other"]}},"role":{"type":"string","enum":["client","engine","project"]},"limit":{"type":"integer","minimum":1,"maximum":200}},"required":["text"],"additionalProperties":false})),
        tool("mir3_knowledge_search", "检索当前项目中已人工激活且版本兼容的 996 领域知识。", json!({"type":"object","properties":{"text":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100}},"required":["text"],"additionalProperties":false})),
        tool("mir3_knowledge_get", "读取一条已激活的 996 领域知识。", json!({"type":"object","properties":{"knowledgeId":{"type":"string"}},"required":["knowledgeId"],"additionalProperties":false})),
        tool("mir3_draft_open", "创建一个外置修改 Draft；不会修改正式项目。", json!({"type":"object","properties":{"intent":{"type":"string","minLength":1}},"required":["intent"],"additionalProperties":false})),
        tool("mir3_draft_patch", "向外置 Draft 写入结构化变更；TXT/Lua 必须使用格式安全 operation，不会修改正式项目。", json!({"type":"object","properties":{"draftId":{"type":"string"},"expectedRevision":{"type":"integer","minimum":0},"changes":{"type":"array","minItems":1,"items":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"deleted":{"type":"boolean"},"expectedSha256":{"type":"string"}},"required":["path"],"additionalProperties":false}},"operation":{"oneOf":[{"type":"object","properties":{"type":{"const":"text.replace"},"path":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"},"expectedSha256":{"type":"string"},"newline":{"type":"string","enum":["CRLF","LF","CR"]}},"required":["type","path","old","new","expectedSha256"],"additionalProperties":false},{"type":"object","properties":{"type":{"const":"text.splice"},"path":{"type":"string"},"start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":0},"expected":{"type":"string"},"text":{"type":"string"},"expectedSha256":{"type":"string"},"newline":{"type":"string","enum":["CRLF","LF","CR"]}},"required":["type","path","start","end","expected","text","expectedSha256"],"additionalProperties":false},{"type":"object","properties":{"type":{"const":"lua.replace_function"},"path":{"type":"string"},"functionName":{"type":"string"},"old":{"type":"string"},"replacement":{"type":"string"},"expectedSha256":{"type":"string"}},"required":["type","path","functionName","old","replacement","expectedSha256"],"additionalProperties":false},{"type":"object","properties":{"type":{"const":"xls.update_cells"},"path":{"type":"string"},"expectedSha256":{"type":"string"},"updates":{"type":"array"}},"required":["type","path","expectedSha256","updates"],"additionalProperties":false}]}},"required":["draftId","expectedRevision"],"anyOf":[{"required":["changes"]},{"required":["operation"]}],"additionalProperties":false})),
        tool("mir3_draft_diff", "返回 Draft 与当前 996 项目之间的修改预览。", json!({"type":"object","properties":{"draftId":{"type":"string"}},"required":["draftId"],"additionalProperties":false})),
        tool("mir3_validate", "执行 996 项目结构和 Draft 基线校验。", json!({"type":"object","properties":{"draftId":{"type":"string"}},"additionalProperties":false})),
    ]
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": input_schema})
}

fn required_string(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("MCP_ARGUMENT_INVALID: {key} is required"))
}

fn is_protected_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".txt") || lower.ends_with(".lua") || lower.ends_with(".xls")
}

fn apply_safe_operation(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    revision: i64,
    operation: &Value,
) -> Result<Value, String> {
    let kind = operation
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| "MCP_ARGUMENT_INVALID: operation.type is required".to_string())?;
    if kind == "xls.update_cells" {
        return Err(
            "SAFE_XLS_READ_ONLY: xls.update_cells is reserved for Safe Files 0.2.0".to_string(),
        );
    }
    let path = required_string(operation, "path")?;
    if !path.to_ascii_lowercase().ends_with(".txt") && !path.to_ascii_lowercase().ends_with(".lua")
    {
        return Err("MIR3_SAFE_OPERATION_TYPE: text operations require TXT or Lua".to_string());
    }
    let expected_sha256 = required_string(operation, "expectedSha256")?;
    let opened = store.safe_text_open(project_id, &path, Some(draft_id))?;
    if opened.sha256 != expected_sha256 {
        return Err("SAFE_FILE_SOURCE_CONFLICT: source changed since it was opened".to_string());
    }
    let new_content = match kind {
        "text.replace" => {
            let old = operation
                .get("old")
                .and_then(Value::as_str)
                .ok_or_else(|| "MCP_ARGUMENT_INVALID: operation.old is required".to_string())?;
            let replacement = operation
                .get("new")
                .and_then(Value::as_str)
                .ok_or_else(|| "MCP_ARGUMENT_INVALID: operation.new is required".to_string())?;
            if old.is_empty() || opened.content.matches(old).count() != 1 {
                return Err("SAFE_TEXT_ANCHOR_AMBIGUOUS: old must occur exactly once".to_string());
            }
            opened.content.replacen(old, replacement, 1)
        }
        "text.splice" => {
            let start = operation
                .get("start")
                .and_then(Value::as_u64)
                .ok_or_else(|| "MCP_ARGUMENT_INVALID: operation.start is required".to_string())?
                as usize;
            let end = operation
                .get("end")
                .and_then(Value::as_u64)
                .ok_or_else(|| "MCP_ARGUMENT_INVALID: operation.end is required".to_string())?
                as usize;
            let expected = operation
                .get("expected")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "MCP_ARGUMENT_INVALID: operation.expected is required".to_string()
                })?;
            let replacement = operation
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| "MCP_ARGUMENT_INVALID: operation.text is required".to_string())?;
            let char_count = opened.content.chars().count();
            if start > end || end > char_count {
                return Err("SAFE_TEXT_SPLICE_RANGE: invalid character range".to_string());
            }
            let start_byte = char_byte_index(&opened.content, start);
            let end_byte = char_byte_index(&opened.content, end);
            if &opened.content[start_byte..end_byte] != expected {
                return Err("SAFE_TEXT_SPLICE_CONFLICT: expected text does not match".to_string());
            }
            format!(
                "{}{}{}",
                &opened.content[..start_byte],
                replacement,
                &opened.content[end_byte..]
            )
        }
        "lua.replace_function" => {
            if !path.to_ascii_lowercase().ends_with(".lua") {
                return Err("SAFE_LUA_TYPE_UNSUPPORTED: expected a .lua file".to_string());
            }
            let function_name = required_string(operation, "functionName")?;
            let old = operation
                .get("old")
                .and_then(Value::as_str)
                .ok_or_else(|| "MCP_ARGUMENT_INVALID: operation.old is required".to_string())?;
            let replacement = operation
                .get("replacement")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "MCP_ARGUMENT_INVALID: operation.replacement is required".to_string()
                })?;
            if !old.contains("function") || !old.contains(&function_name) {
                return Err(
                    "SAFE_LUA_FUNCTION_MISMATCH: anchor does not name the requested function"
                        .to_string(),
                );
            }
            if opened.content.matches(old).count() != 1 {
                return Err(
                    "SAFE_LUA_FUNCTION_AMBIGUOUS: function anchor must occur exactly once"
                        .to_string(),
                );
            }
            opened.content.replacen(old, replacement, 1)
        }
        _ => return Err(format!("MIR3_SAFE_OPERATION_UNKNOWN: {kind}")),
    };
    let newline = operation
        .get("newline")
        .and_then(Value::as_str)
        .map(|value| match value {
            "CRLF" => "\r\n",
            "CR" => "\r",
            _ => "\n",
        })
        .map(str::to_string);
    let result = store.safe_text_patch(
        project_id,
        &SafeTextPatch {
            relative_path: path,
            draft_id: Some(draft_id.to_string()),
            expected_revision: revision,
            expected_sha256,
            original_content: opened.content,
            new_content,
            newline,
        },
    )?;
    Ok(json!({
        "preview": result.preview,
        "draftId": result.draft_id,
        "revision": result.revision,
        "sha256": result.sha256
    }))
}

fn char_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map_or(value.len(), |(index, _)| index)
}

fn tool_success(value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string());
    json!({"content":[{"type":"text","text":text}],"structuredContent":value,"isError":false})
}

fn tool_failure(error: &str) -> Value {
    json!({"content":[{"type":"text","text":error}],"isError":true})
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn write_json(writer: &mut impl Write, value: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, value)
        .map_err(|e| format!("MIR3_MCP_WRITE_FAILED: {e}"))?;
    writer
        .write_all(b"\n")
        .map_err(|e| format!("MIR3_MCP_WRITE_FAILED: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("MIR3_MCP_WRITE_FAILED: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn tool_surface_is_exact_and_contains_no_generic_file_tools() {
        let definitions = tool_definitions();
        let names: Vec<&str> = definitions
            .iter()
            .filter_map(|item| item.get("name").and_then(Value::as_str))
            .collect();
        assert_eq!(names, TOOLS);
        assert!(!names.contains(&"mir3_scan"));
        assert!(!names.contains(&"mir3_search"));
        assert!(!names.contains(&"mir3_read"));
        let schemas = serde_json::to_string(&definitions).unwrap();
        assert!(!schemas.contains("\"root\""));
    }

    #[test]
    fn project_status_and_domain_query_use_the_registered_project() {
        let base = std::env::temp_dir().join(format!("mir3-mcp-{}", std::process::id()));
        let root = base.join("项目/木立传奇");
        fs::create_dir_all(root.join("客户端/dev/Lua")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Envir/QuestDiary")).unwrap();
        fs::write(root.join("客户端/dev/Lua/main.lua"), "return 'MIR3'\n").unwrap();
        let store = DomainStore::new(base.join("data")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();

        let status = call_tool(&store, &project.id, "mir3_project_status", json!({}));
        assert_eq!(status.get("isError"), Some(&Value::Bool(false)));
        assert_eq!(
            status.pointer("/structuredContent/project/id"),
            Some(&Value::String(project.id.clone()))
        );

        let query = call_tool(
            &store,
            &project.id,
            "mir3_index_query",
            json!({"text":"main","categories":["Lua"],"role":"client","limit":10}),
        );
        assert_eq!(
            query.pointer("/structuredContent/records/0/category"),
            Some(&json!("Lua"))
        );
        fs::remove_dir_all(base).ok();
    }
}
