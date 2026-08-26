#![cfg_attr(windows, windows_subsystem = "windows")]

//! MIR3 领域 MCP STDIO 服务。
//!
//! 仅暴露 996 项目状态、领域索引、知识、Draft 与校验，不重复 Harness 的通用
//! 文件读取、搜索、编辑器或会话能力。

use mir3_domain::{
    DomainFileQuery, DomainStore, DraftChangeInput, MapDraftOperation, SafeTextPatch,
    SafeXlsDraftPatch,
};
use serde_json::{json, Value};
use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[cfg(test)]
const TOOLS: [&str; 11] = [
    "mir3_system_list",
    "mir3_system_describe",
    "mir3_resource_query",
    "mir3_resource_get",
    "mir3_dependency_resolve",
    "mir3_draft_open",
    "mir3_domain_operate",
    "mir3_capability_list",
    "mir3_capability_describe",
    "mir3_capability_invoke",
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
    let store =
        DomainStore::new_with_domain_pack_root(home.join("projects"), home.join("domain-packs"))?;
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
    let Some(input_schema) = tool_definitions()
        .into_iter()
        .find(|definition| definition.get("name").and_then(Value::as_str) == Some(name))
        .and_then(|definition| definition.get("inputSchema").cloned())
    else {
        return tool_failure(&format!("MCP_TOOL_UNKNOWN: {name}"));
    };
    if let Err(error) = validate_json_schema(&input_schema, &args, "arguments") {
        return tool_failure(&error);
    }
    let scope_token = match required_string(&args, "scopeToken") {
        Ok(value) => value,
        Err(error) => return tool_failure(&error),
    };
    let scope_token = scope_token.as_str();
    let result = match name {
        "mir3_system_list" => store
            .authorize_task_scope(project_id, scope_token, None, None, None)
            .and_then(|_| store.list_domain_systems())
            .map(|systems| json!({"systems": systems})),
        "mir3_system_describe" => required_string(&args, "systemId")
            .and_then(|system_id| {
                store.authorize_task_scope(
                    project_id,
                    scope_token,
                    Some(&system_id),
                    None,
                    None,
                )?;
                store.describe_domain_system(project_id, &system_id)
            })
            .map(|description| json!({"description": description})),
        "mir3_resource_query" => {
            let system_id = required_string(&args, "systemId");
            let query = serde_json::from_value::<DomainFileQuery>(json!({
                "text": args.get("text").and_then(Value::as_str).unwrap_or(""),
                "limit": args.get("limit"),
                "offset": args.get("offset")
            }))
            .map_err(|error| format!("MCP_ARGUMENT_INVALID: {error}"));
            system_id
                .and_then(|system_id| query.map(|query| (system_id, query)))
                .and_then(|(system_id, query)| {
                    store.authorize_task_scope(
                        project_id,
                        scope_token,
                        Some(&system_id),
                        None,
                        None,
                    )?;
                    store.query_domain_files(project_id, &system_id, &query)
                })
                .map(|resources| json!({"resources": resources}))
        }
        "mir3_resource_get" => {
            let system_id = required_string(&args, "systemId");
            let resource_id = required_string(&args, "resourceId");
            system_id
                .and_then(|system_id| resource_id.map(|resource_id| (system_id, resource_id)))
                .and_then(|(system_id, resource_id)| {
                    store.authorize_task_scope(
                        project_id,
                        scope_token,
                        Some(&system_id),
                        None,
                        None,
                    )?;
                    store.get_domain_resource(project_id, &system_id, &resource_id)
                })
                .map(|resource| json!({"resource": resource}))
        }
        "mir3_dependency_resolve" => required_string(&args, "systemId")
            .and_then(|system_id| {
                store.authorize_task_scope(
                    project_id,
                    scope_token,
                    Some(&system_id),
                    None,
                    None,
                )?;
                store.resolve_domain_dependencies(&system_id)
            })
            .map(|graph| json!({"graph": graph})),
        "mir3_draft_open" => {
            let system_id = required_string(&args, "systemId");
            let intent = required_string(&args, "intent");
            system_id
                .and_then(|system_id| intent.map(|intent| (system_id, intent)))
                .and_then(|(system_id, intent)| {
                    store.authorize_task_scope(
                        project_id,
                        scope_token,
                        Some(&system_id),
                        Some(&system_id),
                        None,
                    )?;
                    let description = store.describe_domain_system(project_id, &system_id)?;
                    let draft = store.open_draft(project_id, &intent)?;
                    store.bind_draft_domain(
                        project_id,
                        &draft.id,
                        &system_id,
                        &description.manifest.version,
                        None,
                    )?;
                    store.attach_draft_to_scope(project_id, scope_token, &system_id, &draft.id)?;
                    Ok(draft)
                })
                .map(|draft| json!({"draft": draft}))
        }
        "mir3_domain_operate" => {
            let draft_id = required_string(&args, "draftId");
            let capability_id = required_string(&args, "capabilityId");
            draft_id
                .and_then(|draft_id| capability_id.map(|capability_id| (draft_id, capability_id)))
                .and_then(|(draft_id, capability_id)| {
                    let capability =
                        store.validate_draft_capability(project_id, &draft_id, &capability_id)?;
                    validate_requested_version(&args, &capability.version)?;
                    let write_system = capability.write_systems.first().ok_or_else(|| {
                        "DOMAIN_CAPABILITY_READONLY: capability has no write scope".to_string()
                    })?;
                    store.authorize_task_scope(
                        project_id,
                        scope_token,
                        Some(write_system),
                        Some(write_system),
                        Some(&draft_id),
                    )?;
                    let params = args
                        .get("params")
                        .ok_or_else(|| "MCP_ARGUMENT_INVALID: params is required".to_string())?;
                    validate_json_schema(&capability.parameter_schema, params, "params")?;
                    execute_manifest_operation(
                        store,
                        project_id,
                        &draft_id,
                        &capability_id,
                        write_system,
                        &capability.steps,
                        params,
                    )
                })
        }
        "mir3_capability_list" => {
            let system_id = args.get("systemId").and_then(Value::as_str);
            store.authorize_task_scope(project_id, scope_token, system_id, None, None).and_then(|_| {
                let systems = store.list_domain_systems()?;
                let mut capabilities = systems.into_iter()
                    .filter(|system| system_id.is_none_or(|id| id == system.system_id))
                    .flat_map(|system| system.capabilities.into_iter().map(move |capability| json!({"source":"official","systemId":system.system_id,"capability":capability})))
                    .collect::<Vec<_>>();
                capabilities.extend(store.list_user_capabilities(project_id, system_id)?.into_iter().map(|capability| json!({"source":"user","systemId":capability.system_id,"capability":capability})));
                Ok(json!({"capabilities": capabilities}))
            })
        }
        "mir3_capability_describe" => {
            required_string(&args, "capabilityId").and_then(|capability_id| {
                store.authorize_task_scope(project_id, scope_token, None, None, None)?;
                if let Some(found) = store.list_domain_systems()?.into_iter().find_map(|system| {
                    system
                        .capabilities
                        .into_iter()
                        .find(|capability| capability.id == capability_id)
                        .map(|capability| (system.system_id, capability))
                }) {
                    return Ok(
                        json!({"source":"official","systemId":found.0,"capability":found.1}),
                    );
                }
                let capability = store.get_user_capability(project_id, &capability_id, None)?;
                Ok(json!({"source":"user","systemId":capability.system_id,"capability":capability}))
            })
        }
        "mir3_capability_invoke" => {
            let capability_id = required_string(&args, "capabilityId");
            capability_id.and_then(|capability_id| {
                if let Some((system_id, capability)) = find_official_capability(
                    store,
                    &capability_id,
                    args.get("systemId").and_then(Value::as_str),
                )? {
                    if capability.write_systems.is_empty() {
                        validate_requested_version(&args, &capability.version)?;
                        store.authorize_task_scope(
                            project_id,
                            scope_token,
                            Some(&system_id),
                            None,
                            None,
                        )?;
                        let parameters = args.get("params").cloned().ok_or_else(|| {
                            "MCP_ARGUMENT_INVALID: params is required".to_string()
                        })?;
                        validate_json_schema(
                            &capability.parameter_schema,
                            &parameters,
                            "params",
                        )?;
                        let files = store.query_domain_files(
                            project_id,
                            &system_id,
                            &DomainFileQuery {
                                text: parameters
                                    .get("query")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                limit: Some(1_000),
                                offset: None,
                            },
                        )?;
                        let mut resource_ids = parameters
                            .get("resourceIds")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect::<Vec<_>>();
                        if let Some(resource_id) =
                            parameters.get("resourceId").and_then(Value::as_str)
                        {
                            resource_ids.push(resource_id.to_string());
                        }
                        resource_ids.sort();
                        resource_ids.dedup();
                        let resources = resource_ids
                            .iter()
                            .map(|resource_id| {
                                store.get_domain_resource(project_id, &system_id, resource_id)
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        let dependencies = store.resolve_domain_dependencies(&system_id)?;
                        let validation = (capability_id.starts_with("validate-")
                            || capability_id.starts_with("preview-"))
                        .then(|| store.validate_domain_system(project_id, &system_id))
                        .transpose()?;
                        return Ok(json!({
                            "mode":"read",
                            "systemId":system_id,
                            "operation":capability_id,
                            "files":files,
                            "resources":resources,
                            "dependencies":dependencies,
                            "validation":validation
                        }));
                    }
                    let draft_id = required_string(&args, "draftId")?;
                    let pinned = store.validate_draft_capability(
                        project_id,
                        &draft_id,
                        &capability_id,
                    )?;
                    validate_requested_version(&args, &pinned.version)?;
                    let write_system = pinned.write_systems.first().ok_or_else(|| {
                        "DOMAIN_CAPABILITY_READONLY: capability has no write scope".to_string()
                    })?;
                    if write_system != &system_id {
                        return Err(format!(
                            "DOMAIN_CAPABILITY_DRAFT_SYSTEM_MISMATCH: requested {system_id}, draft is scoped to {write_system}"
                        ));
                    }
                    store.authorize_task_scope(
                        project_id,
                        scope_token,
                        Some(write_system),
                        Some(write_system),
                        Some(&draft_id),
                    )?;
                    let params = args
                        .get("params")
                        .ok_or_else(|| "MCP_ARGUMENT_INVALID: params is required".to_string())?;
                    validate_json_schema(&pinned.parameter_schema, params, "params")?;
                    return execute_manifest_operation(
                        store,
                        project_id,
                        &draft_id,
                        &capability_id,
                        &system_id,
                        &pinned.steps,
                        params,
                    );
                }
                let draft_id = required_string(&args, "draftId")?;
                let requested_version = args.get("version").and_then(Value::as_str);
                let user = store.get_user_capability(
                    project_id,
                    &capability_id,
                    requested_version,
                )?;
                if user.status != "active" {
                    return Err(format!(
                        "CAPABILITY_NOT_ACTIVE: {}@{}",
                        capability_id, user.version
                    ));
                }
                let scoped = store.validate_user_capability_for_draft(
                    project_id,
                    &draft_id,
                    &capability_id,
                )?;
                if scoped.version != user.version {
                    return Err(format!(
                        "CAPABILITY_VERSION_NOT_CURRENT: requested {}, active {}",
                        user.version, scoped.version
                    ));
                }
                let write_system = user.write_systems.first().ok_or_else(|| {
                    "CAPABILITY_WRITE_SYSTEM_REQUIRED: capability has no write scope".to_string()
                })?;
                store.authorize_task_scope(
                    project_id,
                    scope_token,
                    Some(&write_system),
                    Some(&write_system),
                    Some(&draft_id),
                )?;
                let params = args
                    .get("params")
                    .ok_or_else(|| "MCP_ARGUMENT_INVALID: params is required".to_string())?;
                validate_json_schema(&user.parameter_schema, params, "params")?;
                let semantic_steps = user
                    .steps
                    .as_array()
                    .ok_or_else(|| "CAPABILITY_STEP_INVALID: steps must be an array".to_string())?;
                if semantic_steps.len() != 1 {
                    return Err(
                        "CAPABILITY_STEP_COMPILATION_UNSUPPORTED: expected exactly one registered operation"
                            .to_string(),
                    );
                }
                let semantic_step = semantic_steps[0].as_object().ok_or_else(|| {
                    "CAPABILITY_STEP_INVALID: step must be an object".to_string()
                })?;
                if semantic_step.keys().any(|key| key != "type" && key != "operation")
                    || semantic_step.get("type").and_then(Value::as_str)
                        != Some("domain-operation")
                {
                    return Err(
                        "CAPABILITY_STEP_INVALID: only a registered domain-operation is allowed"
                            .to_string(),
                    );
                }
                let operation_id = semantic_step
                    .get("operation")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "CAPABILITY_STEP_INVALID: operation is required".to_string())?;
                let manifest = store
                    .list_domain_systems()?
                    .into_iter()
                    .find(|manifest| manifest.system_id == user.system_id)
                    .ok_or_else(|| format!("DOMAIN_SYSTEM_NOT_FOUND: {}", user.system_id))?;
                let operation = manifest
                    .operations
                    .iter()
                    .find(|operation| operation.id == operation_id)
                    .ok_or_else(|| {
                        format!("CAPABILITY_OPERATION_NOT_REGISTERED: {operation_id}")
                    })?;
                validate_json_schema(&operation.parameter_schema, params, "params")?;
                execute_manifest_operation(
                    store,
                    project_id,
                    &draft_id,
                    operation_id,
                    &user.system_id,
                    &operation.steps,
                    params,
                )
            })
        }
        "mir3_validate" => store.validate_project(project_id).and_then(|project| {
            let system_id = args.get("systemId").and_then(Value::as_str);
            let draft = args.get("draftId").and_then(Value::as_str);
            store.authorize_task_scope(project_id, scope_token, system_id, None, draft)?;
            let domain = system_id
                .map(|system_id| store.validate_domain_system(project_id, system_id))
                .transpose()?;
            let draft_preview = draft
                .map(|draft_id| store.preview_draft(project_id, draft_id))
                .transpose()?;
            Ok(json!({
                "valid": project.status != mir3_domain::ProjectStatus::Missing,
                "warnings": project.warnings,
                "project": project,
                "domain": domain,
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

fn find_official_capability(
    store: &DomainStore,
    capability_id: &str,
    requested_system_id: Option<&str>,
) -> Result<Option<(String, mir3_domain::OfficialCapability)>, String> {
    let matches = store
        .list_domain_systems()?
        .into_iter()
        .filter(|manifest| {
            requested_system_id.is_none_or(|system_id| system_id == manifest.system_id)
        })
        .filter_map(|manifest| {
            manifest
                .capabilities
                .into_iter()
                .find(|capability| capability.id == capability_id)
                .map(|capability| (manifest.system_id, capability))
        })
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(format!(
            "DOMAIN_CAPABILITY_AMBIGUOUS: {capability_id} requires systemId"
        ));
    }
    Ok(matches.into_iter().next())
}

fn tool_definitions() -> Vec<Value> {
    vec![
        tool(
            "mir3_system_list",
            "列出 MIR3 Studio Kernel 当前注册的 33 个领域系统。",
            with_scope(empty_schema()),
        ),
        tool(
            "mir3_system_describe",
            "返回一个领域包的版本、真实文件覆盖、依赖、视图和安全能力。",
            with_scope(system_schema()),
        ),
        tool(
            "mir3_resource_query",
            "按领域查询真实项目文件资源；未知格式会标记为只读。",
            with_scope(
                json!({"type":"object","properties":{"systemId":{"type":"string"},"text":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":10000},"offset":{"type":"integer","minimum":0}},"required":["systemId"],"additionalProperties":false}),
            ),
        ),
        tool(
            "mir3_resource_get",
            "通过稳定资源 ID 读取领域文件元数据。",
            with_scope(
                json!({"type":"object","properties":{"systemId":{"type":"string"},"resourceId":{"type":"string"}},"required":["systemId","resourceId"],"additionalProperties":false}),
            ),
        ),
        tool(
            "mir3_dependency_resolve",
            "返回当前系统可读依赖范围。",
            with_scope(system_schema()),
        ),
        tool(
            "mir3_draft_open",
            "为指定系统创建外置 Draft；不会修改正式项目。",
            with_scope(
                json!({"type":"object","properties":{"systemId":{"type":"string"},"intent":{"type":"string","minLength":1}},"required":["systemId","intent"],"additionalProperties":false}),
            ),
        ),
        tool(
            "mir3_domain_operate",
            "按领域包白名单向外置 Draft 写入安全结构化操作。",
            with_scope(operation_schema()),
        ),
        tool(
            "mir3_capability_list",
            "列出官方及可调用的领域能力。",
            with_scope(
                json!({"type":"object","properties":{"systemId":{"type":"string"}},"additionalProperties":false}),
            ),
        ),
        tool(
            "mir3_capability_describe",
            "读取一个能力的版本和安全策略。",
            with_scope(
                json!({"type":"object","properties":{"capabilityId":{"type":"string"}},"required":["capabilityId"],"additionalProperties":false}),
            ),
        ),
        tool(
            "mir3_capability_invoke",
            "在外置 Draft 中调用一个安全领域能力。",
            with_scope(
                json!({"type":"object","properties":{"capabilityId":{"type":"string"},"version":{"type":"string"},"systemId":{"type":"string"},"draftId":{"type":"string"},"params":{"type":"object"}},"required":["capabilityId","params"],"additionalProperties":false}),
            ),
        ),
        tool(
            "mir3_validate",
            "执行项目、领域和可选 Draft 校验。",
            with_scope(
                json!({"type":"object","properties":{"systemId":{"type":"string"},"draftId":{"type":"string"}},"additionalProperties":false}),
            ),
        ),
    ]
}

fn empty_schema() -> Value {
    json!({"type":"object","properties":{},"additionalProperties":false})
}

fn system_schema() -> Value {
    json!({"type":"object","properties":{"systemId":{"type":"string"}},"required":["systemId"],"additionalProperties":false})
}

fn operation_schema() -> Value {
    json!({"type":"object","properties":{"capabilityId":{"type":"string"},"version":{"type":"string"},"draftId":{"type":"string"},"params":{"type":"object"}},"required":["capabilityId","draftId","params"],"additionalProperties":false})
}

fn with_scope(mut schema: Value) -> Value {
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "scopeToken".to_string(),
            json!({"type":"string","minLength":32}),
        );
    }
    if let Some(root) = schema.as_object_mut() {
        let required = root
            .entry("required")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("tool schema required must be an array");
        if !required.iter().any(|value| value == "scopeToken") {
            required.push(json!("scopeToken"));
        }
    }
    schema
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

fn validate_requested_version(args: &Value, actual: &str) -> Result<(), String> {
    if let Some(requested) = args.get("version").and_then(Value::as_str) {
        if requested != actual {
            return Err(format!(
                "CAPABILITY_VERSION_MISMATCH: requested {requested}, current {actual}"
            ));
        }
    }
    Ok(())
}

fn validate_json_schema(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    if let Some(expected) = schema.get("const") {
        if value != expected {
            return Err(format!("CAPABILITY_PARAMETER_CONST_INVALID: {path}"));
        }
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        if !values.contains(value) {
            return Err(format!("CAPABILITY_PARAMETER_ENUM_INVALID: {path}"));
        }
    }
    let expected = schema.get("type").and_then(Value::as_str);
    let matches_type = match expected {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("integer") => value.as_i64().is_some() || value.as_u64().is_some(),
        Some("number") => value.is_number(),
        Some("boolean") => value.is_boolean(),
        Some("null") => value.is_null(),
        None => true,
        Some(kind) => {
            return Err(format!("CAPABILITY_SCHEMA_UNSUPPORTED: {path} type {kind}"));
        }
    };
    if !matches_type {
        return Err(format!("CAPABILITY_PARAMETER_TYPE_INVALID: {path}"));
    }
    if let Some(object) = value.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("CAPABILITY_PARAMETER_REQUIRED: {path}.{key}"));
                }
            }
        }
        if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
            if let Some(properties) = properties {
                if let Some(key) = object.keys().find(|key| !properties.contains_key(*key)) {
                    return Err(format!("CAPABILITY_PARAMETER_UNKNOWN: {path}.{key}"));
                }
            }
        }
        if let Some(properties) = properties {
            for (key, item) in object {
                if let Some(item_schema) = properties.get(key) {
                    validate_json_schema(item_schema, item, &format!("{path}.{key}"))?;
                }
            }
        }
        if schema
            .get("minProperties")
            .and_then(Value::as_u64)
            .is_some_and(|minimum| object.len() < minimum as usize)
        {
            return Err(format!("CAPABILITY_PARAMETER_RANGE_INVALID: {path}"));
        }
    }
    if let Some(array) = value.as_array() {
        if schema
            .get("minItems")
            .and_then(Value::as_u64)
            .is_some_and(|minimum| array.len() < minimum as usize)
        {
            return Err(format!("CAPABILITY_PARAMETER_RANGE_INVALID: {path}"));
        }
        if let Some(maximum) = schema.get("maxItems").and_then(Value::as_u64) {
            if array.len() as u64 > maximum {
                return Err(format!("CAPABILITY_PARAMETER_RANGE_INVALID: {path}"));
            }
        }
        if let Some(items) = schema.get("items") {
            for (index, item) in array.iter().enumerate() {
                validate_json_schema(items, item, &format!("{path}[{index}]"))?;
            }
        }
        if schema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
            for (index, item) in array.iter().enumerate() {
                if array[..index].contains(item) {
                    return Err(format!("CAPABILITY_PARAMETER_UNIQUE_INVALID: {path}"));
                }
            }
        }
    }
    if let Some(string) = value.as_str() {
        if schema
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|minimum| string.chars().count() < minimum as usize)
        {
            return Err(format!("CAPABILITY_PARAMETER_RANGE_INVALID: {path}"));
        }
        if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
            let pattern_matches = match pattern {
                "\\.(txt|lua)$" => {
                    string.to_ascii_lowercase().ends_with(".txt")
                        || string.to_ascii_lowercase().ends_with(".lua")
                }
                "^\\d+\\.\\d+$" => {
                    let parts = string.split('.').collect::<Vec<_>>();
                    parts.len() == 2
                        && parts.iter().all(|part| {
                            !part.is_empty() && part.chars().all(|value| value.is_ascii_digit())
                        })
                }
                _ => {
                    return Err(format!("CAPABILITY_SCHEMA_PATTERN_UNSUPPORTED: {path}"));
                }
            };
            if !pattern_matches {
                return Err(format!("CAPABILITY_PARAMETER_PATTERN_INVALID: {path}"));
            }
        }
    }
    if let Some(number) = value.as_f64() {
        if schema
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|minimum| number < minimum)
            || schema
                .get("maximum")
                .and_then(Value::as_f64)
                .is_some_and(|maximum| number > maximum)
        {
            return Err(format!("CAPABILITY_PARAMETER_RANGE_INVALID: {path}"));
        }
    }
    Ok(())
}

fn execute_manifest_operation(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    operation_id: &str,
    system_id: &str,
    steps: &[mir3_domain::DomainCapabilityStep],
    params: &Value,
) -> Result<Value, String> {
    if steps.len() != 2
        || steps[0].action != "resolve-and-preview"
        || steps[0].schema.is_empty()
        || steps[1].action != "record-reversible-draft-step"
        || steps[1].operation != operation_id
        || steps[0].primitive != steps[1].primitive
        || !matches!(
            steps[1].primitive.as_str(),
            "text" | "xls" | "graph" | "timeline" | "map"
        )
    {
        return Err(format!(
            "DOMAIN_STEP_COMPILATION_DENIED: unsupported manifest steps for {operation_id}"
        ));
    }
    if params.get("operation").and_then(Value::as_str) != Some(operation_id) {
        return Err(format!(
            "DOMAIN_OPERATION_PARAMETER_MISMATCH: expected {operation_id}"
        ));
    }
    let mut revision = params
        .get("expectedRevision")
        .and_then(Value::as_i64)
        .ok_or_else(|| "MCP_ARGUMENT_INVALID: params.expectedRevision is required".to_string())?;
    let draft = store.get_draft(project_id, draft_id)?;
    if draft.revision != revision {
        return Err(format!(
            "DRAFT_REVISION_CONFLICT: expected {revision}, current {}",
            draft.revision
        ));
    }
    let mut resource_ids = params
        .get("resourceIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Some(resource_id) = params.get("resourceId").and_then(Value::as_str) {
        resource_ids.push(resource_id.to_string());
    }
    resource_ids.sort();
    resource_ids.dedup();
    if resource_ids.is_empty() || resource_ids.len() > 10_000 {
        return Err(
            "DOMAIN_OPERATION_RESOURCE_REQUIRED: manifest operation requires 1..10000 resourceIds"
                .to_string(),
        );
    }
    let mut changes = params
        .get("changes")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let expected_reference = if let (Some(field), Some(from), Some(to)) = (
        params.get("referenceField").and_then(Value::as_str),
        params.get("fromReference").and_then(Value::as_str),
        params.get("toReference").and_then(Value::as_str),
    ) {
        changes.insert(field.to_string(), Value::String(to.to_string()));
        Some((field.to_string(), from.to_string()))
    } else {
        None
    };
    if changes.is_empty() {
        return Err(format!(
            "DOMAIN_OPERATION_NOT_COMPILED: {operation_id} has no safe field-change mapping"
        ));
    }
    let registered = store
        .list_domain_systems()?
        .into_iter()
        .find(|manifest| manifest.system_id == system_id)
        .is_some_and(|manifest| {
            manifest
                .operations
                .iter()
                .any(|operation| operation.id == operation_id)
        });
    if !registered {
        return Err(format!(
            "DOMAIN_OPERATION_NOT_REGISTERED: {system_id}:{operation_id}"
        ));
    }
    let mut results = Vec::with_capacity(resource_ids.len());
    for resource_id in resource_ids {
        let resource = store.get_domain_resource(project_id, system_id, &resource_id)?;
        if !resource.writable || resource.files.len() != 1 {
            return Err(format!("DOMAIN_OPERATION_RESOURCE_READONLY: {resource_id}"));
        }
        let file = &resource.files[0];
        let extension = file.extension.as_deref().unwrap_or_default();
        let primitive = match extension {
            value if value.eq_ignore_ascii_case("txt") || value.eq_ignore_ascii_case("lua") => {
                if !matches!(
                    steps[1].primitive.as_str(),
                    "text" | "graph" | "timeline" | "map"
                ) {
                    return Err(format!(
                        "DOMAIN_STEP_FILE_TYPE_MISMATCH: {} cannot edit {}",
                        steps[1].primitive, file.path
                    ));
                }
                compile_text_field_changes(
                    store,
                    project_id,
                    draft_id,
                    &file.path,
                    &changes,
                    expected_reference.as_ref(),
                )?
            }
            value if value.eq_ignore_ascii_case("xls") => {
                if !matches!(steps[1].primitive.as_str(), "xls" | "graph" | "timeline") {
                    return Err(format!(
                        "DOMAIN_STEP_FILE_TYPE_MISMATCH: {} cannot edit {}",
                        steps[1].primitive, file.path
                    ));
                }
                compile_xls_field_changes(
                    store,
                    project_id,
                    &file.path,
                    &changes,
                    expected_reference.as_ref(),
                )?
            }
            _ => {
                return Err(format!(
                    "DOMAIN_OPERATION_UNKNOWN_WRITER: {operation_id} cannot safely compile {}",
                    file.path
                ));
            }
        };
        let result = apply_safe_operation(store, project_id, draft_id, revision, &primitive)?;
        revision = result
            .get("revision")
            .and_then(Value::as_i64)
            .or_else(|| {
                result
                    .pointer("/preview/draft/revision")
                    .and_then(Value::as_i64)
            })
            .ok_or_else(|| "DOMAIN_OPERATION_REVISION_MISSING: primitive result".to_string())?;
        results.push(result);
    }
    Ok(json!({
        "operation":operation_id,
        "manifestPrimitive":steps[1].primitive,
        "revision":revision,
        "results":results
    }))
}

fn compile_text_field_changes(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    path: &str,
    changes: &serde_json::Map<String, Value>,
    expected_reference: Option<&(String, String)>,
) -> Result<Value, String> {
    let opened = store.safe_text_open(project_id, path, Some(draft_id))?;
    let mut replacements = Vec::with_capacity(changes.len());
    for (field, value) in changes {
        let (old, new, old_value) = field_line_replacement(&opened.content, field, value)?;
        if expected_reference.is_some_and(|(expected_field, expected)| {
            expected_field == field && expected != &old_value
        }) {
            return Err(format!("DOMAIN_REFERENCE_SOURCE_MISMATCH: {field}"));
        }
        replacements.push(json!({"old":old,"new":new,"expectedCount":1}));
    }
    Ok(json!({
        "type":"text.batch_replace",
        "path":path,
        "expectedSha256":opened.sha256,
        "replacements":replacements
    }))
}

fn field_line_replacement(
    content: &str,
    field: &str,
    value: &Value,
) -> Result<(String, String, String), String> {
    let rendered = scalar_text(value)?;
    let mut matches = Vec::new();
    for line in content.lines() {
        let leading = line.len().saturating_sub(line.trim_start().len());
        let trimmed = line.trim_start();
        for marker in [
            format!("{field}="),
            format!("{field}:"),
            format!("{field}\t"),
        ] {
            if let Some(old_value) = trimmed.strip_prefix(&marker) {
                matches.push((
                    line.to_string(),
                    format!("{}{}{}", &line[..leading], marker, rendered),
                    old_value.trim().trim_matches(['\"', '\'']).to_string(),
                ));
            }
        }
        let json_marker = format!("\"{field}\"");
        if let Some(rest) = trimmed.strip_prefix(&json_marker) {
            if let Some(rest) = rest.trim_start().strip_prefix(':') {
                let comma = rest.trim_end().ends_with(',');
                let old_value = rest.trim().trim_end_matches(',').trim();
                matches.push((
                    line.to_string(),
                    format!(
                        "{}{}: {}{}",
                        &line[..leading],
                        json_marker,
                        serde_json::to_string(value)
                            .map_err(|error| format!("DOMAIN_FIELD_VALUE_INVALID: {error}"))?,
                        if comma { "," } else { "" }
                    ),
                    old_value.trim_matches(['\"', '\'']).to_string(),
                ));
            }
        }
    }
    if matches.len() != 1 {
        return Err(format!(
            "DOMAIN_FIELD_ANCHOR_AMBIGUOUS: {field} matched {} lines",
            matches.len()
        ));
    }
    Ok(matches.remove(0))
}

fn compile_xls_field_changes(
    store: &DomainStore,
    project_id: &str,
    path: &str,
    changes: &serde_json::Map<String, Value>,
    expected_reference: Option<&(String, String)>,
) -> Result<Value, String> {
    let workbook = store.safe_xls_open(project_id, path)?;
    let mut updates = Vec::with_capacity(changes.len());
    for (field, value) in changes {
        let mut matches = Vec::new();
        for sheet in &workbook.sheets {
            let data =
                store.safe_xls_sheet_read(project_id, path, &sheet.name, &workbook.sha256)?;
            if data.rows.len() != 2 {
                continue;
            }
            for (column, header) in data.rows[0].iter().enumerate() {
                if header == field {
                    matches.push((sheet.name.clone(), column, data.rows[1][column].clone()));
                }
            }
        }
        if matches.len() != 1 {
            return Err(format!(
                "DOMAIN_XLS_FIELD_AMBIGUOUS: {field} matched {} single-record sheets",
                matches.len()
            ));
        }
        let (sheet, column, old_value) = matches.remove(0);
        if expected_reference.is_some_and(|(expected_field, expected)| {
            expected_field == field && expected != &old_value
        }) {
            return Err(format!("DOMAIN_REFERENCE_SOURCE_MISMATCH: {field}"));
        }
        updates.push(json!({
            "sheet":sheet,
            "row":1,
            "column":column,
            "expectedValue":old_value,
            "value":value
        }));
    }
    Ok(json!({
        "type":"xls.update_cells",
        "path":path,
        "expectedSha256":workbook.sha256,
        "updates":updates
    }))
}

fn scalar_text(value: &Value) -> Result<String, String> {
    match value {
        Value::String(value)
            if !value.contains('\r') && !value.contains('\n') && !value.contains('\0') =>
        {
            Ok(value.clone())
        }
        Value::String(_) => Err(
            "DOMAIN_FIELD_VALUE_INVALID: text field values must be single-line scalars".to_string(),
        ),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Null => Ok(String::new()),
        _ => Err("DOMAIN_FIELD_VALUE_INVALID: expected a scalar".to_string()),
    }
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
    if kind == "map.edit" {
        let operation = serde_json::from_value::<MapDraftOperation>(operation.clone())
            .map_err(|error| format!("MCP_ARGUMENT_INVALID: {error}"))?;
        let preview = store.map_draft_operate(project_id, draft_id, revision, &operation)?;
        return Ok(json!({"preview": preview}));
    }
    if kind == "xls.update_cells" {
        let request = serde_json::from_value::<SafeXlsDraftPatch>(json!({
            "relativePath": required_string(operation, "path")?,
            "draftId": draft_id,
            "expectedRevision": revision,
            "expectedSha256": required_string(operation, "expectedSha256")?,
            "updates": operation.get("updates").cloned().unwrap_or(Value::Null)
        }))
        .map_err(|error| format!("MCP_ARGUMENT_INVALID: {error}"))?;
        let result = store.safe_xls_patch(project_id, &request)?;
        return Ok(json!({
            "preview": result.preview,
            "draftId": result.draft_id,
            "revision": result.revision,
            "sha256": result.sha256
        }));
    }
    if kind == "resource.clone" {
        let source_path = required_string(operation, "sourcePath")?;
        let target_path = required_string(operation, "targetPath")?;
        let extension_supported = [".txt", ".lua"]
            .iter()
            .any(|extension| source_path.to_ascii_lowercase().ends_with(extension));
        let source_extension = std::path::Path::new(&source_path)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);
        let target_extension = std::path::Path::new(&target_path)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);
        if !extension_supported || source_extension != target_extension {
            return Err(
                "SAFE_CLONE_TYPE_UNSUPPORTED: generic clone requires matching TXT or Lua extensions"
                    .to_string(),
            );
        }
        let expected_sha256 = required_string(operation, "expectedSha256")?;
        let source = store.safe_text_open(project_id, &source_path, Some(draft_id))?;
        if source.sha256 != expected_sha256 {
            return Err(
                "SAFE_FILE_SOURCE_CONFLICT: source changed since it was opened".to_string(),
            );
        }
        let preview = store.patch_draft(
            project_id,
            draft_id,
            revision,
            &[DraftChangeInput {
                path: target_path,
                content: Some(source.content),
                deleted: false,
                expected_sha256: None,
            }],
        )?;
        return Ok(json!({"preview":preview}));
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
        "text.batch_replace" => {
            let replacements = operation
                .get("replacements")
                .and_then(Value::as_array)
                .filter(|items| !items.is_empty() && items.len() <= 10_000)
                .ok_or_else(|| {
                    "MCP_ARGUMENT_INVALID: replacements must contain 1..10000 items".to_string()
                })?;
            let mut output = opened.content.clone();
            for (index, replacement) in replacements.iter().enumerate() {
                let old = required_string(replacement, "old")?;
                let new = replacement
                    .get("new")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!("MCP_ARGUMENT_INVALID: replacements[{index}].new is required")
                    })?;
                let expected_count = replacement
                    .get("expectedCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(1) as usize;
                let actual = output.matches(&old).count();
                if old.is_empty() || actual != expected_count {
                    return Err(format!(
                        "SAFE_TEXT_ANCHOR_COUNT_CONFLICT: replacements[{index}] expected {expected_count}, got {actual}"
                    ));
                }
                output = output.replace(&old, new);
            }
            output
        }
        "reference.replace" => {
            let old = required_string(operation, "oldId")?;
            let new = required_string(operation, "newId")?;
            let expected_count = operation
                .get("expectedCount")
                .and_then(Value::as_u64)
                .ok_or_else(|| "MCP_ARGUMENT_INVALID: expectedCount is required".to_string())?
                as usize;
            let actual = opened.content.matches(&old).count();
            if actual != expected_count || expected_count == 0 {
                return Err(format!(
                    "SAFE_REFERENCE_COUNT_CONFLICT: expected {expected_count}, got {actual}"
                ));
            }
            opened.content.replace(&old, &new)
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
        assert!(!schemas.contains("\"primitive\""));
        assert!(!schemas.contains("\"action\""));
        assert!(!schemas.contains("\"path\""));
        assert!(!schemas.contains("\"field\""));
        assert!(definitions.iter().all(|definition| definition
            .pointer("/inputSchema/required")
            .and_then(Value::as_array)
            .is_some_and(|required| required.iter().any(|value| value == "scopeToken"))));
    }

    #[test]
    fn manifest_compiler_fails_closed_for_unknown_steps() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-step-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let store = DomainStore::new(base.join("data")).unwrap();
        let steps = vec![mir3_domain::DomainCapabilityStep {
            primitive: "shell".to_string(),
            action: "execute".to_string(),
            schema: "schemas/resource.schema.json".to_string(),
            ..Default::default()
        }];
        let error = execute_manifest_operation(
            &store,
            "missing-project",
            "missing-draft",
            "tampered-operation",
            "map",
            &steps,
            &json!({}),
        )
        .unwrap_err();
        assert!(error.starts_with("DOMAIN_STEP_COMPILATION_DENIED:"));
        fs::remove_dir_all(base).ok();
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

        let lease = store
            .issue_task_scope(
                &project.id,
                "task-mcp-test",
                &["quest".to_string()],
                &["quest".to_string()],
                &[],
                json!({"quest":"1.0.0"}),
                mir3_domain::now_millis() + 60_000,
            )
            .unwrap();
        let query = call_tool(
            &store,
            &project.id,
            "mir3_resource_query",
            json!({"scopeToken":lease.token.clone(),"systemId":"quest","text":"QuestDiary","limit":10}),
        );
        assert_eq!(query.get("isError"), Some(&Value::Bool(false)));
        let inspect = call_tool(
            &store,
            &project.id,
            "mir3_capability_invoke",
            json!({
                "scopeToken":lease.token,
                "systemId":"quest",
                "capabilityId":"inspect-quest",
                "params":{"operation":"inspect-quest"}
            }),
        );
        assert_eq!(inspect.get("isError"), Some(&Value::Bool(false)));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn capability_invoke_rejects_tampering_scope_escalation_and_revision_spoofing() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-security-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let root = base.join("项目/安全测试");
        let map_path = root.join("引擎/Mir200/Envir/MapInfo.txt");
        fs::create_dir_all(map_path.parent().unwrap()).unwrap();
        fs::create_dir_all(root.join("客户端/dev/Lua")).unwrap();
        fs::write(
            &map_path,
            "mapinfo\ndisplayName=Old\nwidth=10\nheight=10\nsafeZoneMode=none\nspawnNpcId=npc1\n",
        )
        .unwrap();
        let store = DomainStore::new(base.join("data")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let draft = store.open_draft(&project.id, "安全能力调用").unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "map", "1.0.0", None)
            .unwrap();
        let lease = store
            .issue_task_scope(
                &project.id,
                "task-mcp-security",
                &["map".to_string()],
                &["map".to_string()],
                &[],
                json!({"map":"1.0.0"}),
                mir3_domain::now_millis() + 60_000,
            )
            .unwrap();
        store
            .attach_draft_to_scope(&project.id, &lease.token, "map", &draft.id)
            .unwrap();
        let resource_id = store
            .query_domain_files(
                &project.id,
                "map",
                &DomainFileQuery {
                    text: "MapInfo.txt".to_string(),
                    limit: Some(10),
                    offset: None,
                },
            )
            .unwrap()
            .into_iter()
            .find(|file| file.path.ends_with("MapInfo.txt"))
            .unwrap()
            .resource_id;
        let valid_params = json!({
            "operation":"edit-map-config",
            "resourceIds":[resource_id],
            "changes":{"displayName":"New"},
            "expectedRevision":0
        });

        let top_level_primitive = call_tool(
            &store,
            &project.id,
            "mir3_capability_invoke",
            json!({
                "scopeToken":lease.token.clone(),
                "systemId":"map",
                "capabilityId":"edit-map-config",
                "draftId":draft.id,
                "params":valid_params.clone(),
                "primitive":{"type":"text.batch_replace","path":"../../escape"}
            }),
        );
        assert!(tool_error(&top_level_primitive)
            .contains("CAPABILITY_PARAMETER_UNKNOWN: arguments.primitive"));

        for forbidden in ["primitive", "action", "path", "field"] {
            let mut tampered_params = valid_params.clone();
            tampered_params
                .as_object_mut()
                .unwrap()
                .insert(forbidden.to_string(), json!("attacker-controlled"));
            let tampered = invoke_map_capability(
                &store,
                &project.id,
                &lease.token,
                &draft.id,
                None,
                tampered_params,
            );
            assert!(tool_error(&tampered)
                .contains(&format!("CAPABILITY_PARAMETER_UNKNOWN: params.{forbidden}")));
        }

        let mut wrong_operation_params = valid_params.clone();
        wrong_operation_params["operation"] = json!("clone-map");
        let wrong_operation = invoke_map_capability(
            &store,
            &project.id,
            &lease.token,
            &draft.id,
            None,
            wrong_operation_params,
        );
        assert!(tool_error(&wrong_operation)
            .starts_with("CAPABILITY_PARAMETER_CONST_INVALID: params.operation"));

        let wrong_version = invoke_map_capability(
            &store,
            &project.id,
            &lease.token,
            &draft.id,
            Some("9.9.9"),
            valid_params.clone(),
        );
        assert!(tool_error(&wrong_version).starts_with("CAPABILITY_VERSION_MISMATCH:"));

        let mut wrong_revision_params = valid_params.clone();
        wrong_revision_params["expectedRevision"] = json!(7);
        let wrong_revision = invoke_map_capability(
            &store,
            &project.id,
            &lease.token,
            &draft.id,
            None,
            wrong_revision_params,
        );
        assert!(tool_error(&wrong_revision).starts_with("DRAFT_REVISION_CONFLICT:"));

        let mut multiline_injection_params = valid_params.clone();
        multiline_injection_params["changes"]["displayName"] = json!("New\nwidth=4096");
        let multiline_injection = invoke_map_capability(
            &store,
            &project.id,
            &lease.token,
            &draft.id,
            None,
            multiline_injection_params,
        );
        assert!(tool_error(&multiline_injection).starts_with("DOMAIN_FIELD_VALUE_INVALID:"));

        let shop_draft = store.open_draft(&project.id, "越权商城能力").unwrap();
        store
            .bind_draft_domain(&project.id, &shop_draft.id, "shop", "1.0.0", None)
            .unwrap();
        let scope_escalation = call_tool(
            &store,
            &project.id,
            "mir3_capability_invoke",
            json!({
                "scopeToken":lease.token.clone(),
                "systemId":"shop",
                "capabilityId":"batch-price-shop",
                "draftId":shop_draft.id,
                "params":{}
            }),
        );
        assert!(tool_error(&scope_escalation).contains("SCOPE_"));

        let applied = invoke_map_capability(
            &store,
            &project.id,
            &lease.token,
            &draft.id,
            Some("1.0.0"),
            valid_params,
        );
        assert_eq!(applied.get("isError"), Some(&Value::Bool(false)));
        let preview = store.preview_draft(&project.id, &draft.id).unwrap();
        assert_eq!(preview.draft.revision, 1);
        assert!(fs::read_to_string(map_path)
            .unwrap()
            .contains("displayName=Old"));
        fs::remove_dir_all(base).ok();
    }

    fn invoke_map_capability(
        store: &DomainStore,
        project_id: &str,
        scope_token: &str,
        draft_id: &str,
        version: Option<&str>,
        params: Value,
    ) -> Value {
        let mut arguments = json!({
            "scopeToken":scope_token,
            "systemId":"map",
            "capabilityId":"edit-map-config",
            "draftId":draft_id,
            "params":params
        });
        if let Some(version) = version {
            arguments["version"] = json!(version);
        }
        call_tool(store, project_id, "mir3_capability_invoke", arguments)
    }

    fn tool_error(result: &Value) -> &str {
        result
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }
}
