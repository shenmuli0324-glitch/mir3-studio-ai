#![cfg_attr(windows, windows_subsystem = "windows")]

//! MIR3 领域 MCP STDIO 服务。
//!
//! 仅暴露 996 项目状态、领域索引、知识、Draft 与校验，不重复 Harness 的通用
//! 文件读取、搜索、编辑器或会话能力。

use mir3_domain::{
    DomainFileQuery, DomainManifest, DomainResourceQuery, DomainResourceRecord, DomainStore,
    DraftChangeInput, GuiWorkspaceOperateRequest, MapDraftOperation, SafeTextPatch,
    SafeXlsDraftPatch,
};
use semver::{Version, VersionReq};
use serde_json::{json, Value};
use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[cfg(test)]
const TOOLS: [&str; 16] = [
    "mir3_gui_context",
    "mir3_gui_operate",
    "mir3_gui_asset_query",
    "mir3_gui_validate",
    "mir3_system_list",
    "mir3_system_describe",
    "mir3_resource_query",
    "mir3_resource_get",
    "mir3_dependency_resolve",
    "mir3_draft_open",
    "mir3_draft_diff",
    "mir3_domain_operate",
    "mir3_capability_list",
    "mir3_capability_describe",
    "mir3_capability_invoke",
    "mir3_validate",
];

const MCP_MAX_QUERY_ITEMS: usize = 200;
const MCP_MAX_RESULT_BYTES: usize = 128 * 1024;
const MCP_MAX_SCHEMA_BYTES: usize = 24 * 1024;

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
    let domain_pack_root = env::var_os("MIR3_DOMAIN_PACK_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("domain-packs"));
    let store = DomainStore::new_with_domain_pack_root(home.join("projects"), domain_pack_root)?;
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
    let id = request.get("id").cloned()?;
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
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
        "tools/list" => {
            let tools = tool_definitions();
            let schema_bytes = serde_json::to_vec(&tools).map_or(usize::MAX, |value| value.len());
            if schema_bytes > MCP_MAX_SCHEMA_BYTES {
                Some(rpc_error(
                    id,
                    -32603,
                    &format!(
                        "MCP_SCHEMA_BUDGET_EXCEEDED: {schema_bytes} bytes exceeds {MCP_MAX_SCHEMA_BYTES}"
                    ),
                ))
            } else {
                Some(rpc_result(id, json!({"tools": tools})))
            }
        }
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
    let scope_token = args
        .get("scopeToken")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let result = match name {
        "mir3_gui_context" => required_string(&args, "path")
            .and_then(|path| {
                let token = required_string(&args, "workspaceToken")?;
                store.authorize_gui_workspace(project_id, &path, &token)?;
                store.get_gui_workspace(project_id, &path)
            })
            .map(|snapshot| {
                let selected_node = snapshot.selected_node_id.as_ref().and_then(|node_id| {
                    snapshot
                        .document
                        .nodes
                        .iter()
                        .find(|node| &node.id == node_id)
                });
                json!({
                    "guiResult": {
                        "kind": "context",
                        "workspaceId": snapshot.workspace_id,
                        "path": snapshot.path,
                        "baseSha256": snapshot.base_sha256,
                        "workingRevision": snapshot.working_revision,
                        "dirty": snapshot.dirty,
                        "selectedNodeId": snapshot.selected_node_id,
                        "selectedNode": selected_node,
                        "roots": snapshot.document.roots,
                        "nodes": snapshot.document.nodes,
                        "assets": snapshot.document.assets,
                        "diagnostics": snapshot.document.diagnostics,
                        "source": snapshot.source,
                    }
                })
            }),
        "mir3_gui_operate" => serde_json::from_value::<GuiWorkspaceOperateRequest>(args.clone())
            .map_err(|error| format!("MCP_ARGUMENT_INVALID: {error}"))
            .and_then(|request| store.operate_gui_workspace(project_id, &request))
            .map(|result| {
                json!({
                    "guiResult": {
                        "kind": "operation",
                        "workspaceId": result.workspace_id,
                        "path": result.path,
                        "baseSha256": result.base_sha256,
                        "workingRevision": result.working_revision,
                        "source": result.source,
                        "diagnostics": result.diagnostics,
                        "valid": result.valid,
                    }
                })
            }),
        "mir3_gui_asset_query" => required_string(&args, "path")
            .and_then(|path| {
                let token = required_string(&args, "workspaceToken")?;
                store.authorize_gui_workspace(project_id, &path, &token)?;
                store.get_gui_workspace(project_id, &path)
            })
            .map(|snapshot| {
                let query = args
                    .get("query")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let assets = snapshot
                    .document
                    .assets
                    .into_iter()
                    .filter(|asset| {
                        query.is_empty() || asset.logical_path.to_ascii_lowercase().contains(&query)
                    })
                    .take(MCP_MAX_QUERY_ITEMS)
                    .collect::<Vec<_>>();
                json!({
                    "guiResult": {
                        "kind": "assets",
                        "workspaceId": snapshot.workspace_id,
                        "path": snapshot.path,
                        "workingRevision": snapshot.working_revision,
                        "assets": assets,
                    }
                })
            }),
        "mir3_gui_validate" => {
            let path = required_string(&args, "path");
            let expected_revision = required_i64(&args, "expectedRevision");
            path.and_then(|path| expected_revision.map(|revision| (path, revision)))
                .and_then(|(path, expected_revision)| {
                    let token = required_string(&args, "workspaceToken")?;
                    store.authorize_gui_workspace(project_id, &path, &token)?;
                    let snapshot = store.get_gui_workspace(project_id, &path)?;
                    if snapshot.working_revision != expected_revision {
                        return Err(format!(
                            "GUI_WORKSPACE_REVISION_CONFLICT: expected {expected_revision}, got {}",
                            snapshot.working_revision
                        ));
                    }
                    let valid = !snapshot.document.diagnostics.iter().any(|diagnostic| {
                        diagnostic.severity == mir3_ui::DiagnosticSeverity::Error
                    });
                    Ok(json!({
                        "guiResult": {
                            "kind": "validation",
                            "workspaceId": snapshot.workspace_id,
                            "path": snapshot.path,
                            "baseSha256": snapshot.base_sha256,
                            "workingRevision": snapshot.working_revision,
                            "valid": valid,
                            "diagnostics": snapshot.document.diagnostics,
                        }
                    }))
                })
        }
        "mir3_system_list" => authorize_project_read(store, project_id, scope_token, None)
            .and_then(|lease| {
                store.list_domain_systems().map(|systems| match lease {
                    Some(lease) => systems
                        .into_iter()
                        .filter(|system| lease.read_systems.contains(&system.system_id))
                        .collect(),
                    None => systems,
                })
            })
            .map(system_list_payload),
        "mir3_system_describe" => required_string(&args, "systemId")
            .and_then(|system_id| {
                authorize_project_read(store, project_id, scope_token, Some(&system_id))?;
                store.describe_domain_system(project_id, &system_id)
            })
            .map(|description| json!({"description": description})),
        "mir3_resource_query" => {
            let system_id = required_string(&args, "systemId");
            let query = serde_json::from_value::<DomainResourceQuery>(json!({
                "text": args.get("text").and_then(Value::as_str).unwrap_or(""),
                "resourceType": args.get("resourceType"),
                "limit": args.get("limit"),
                "offset": args.get("offset")
            }))
            .map_err(|error| format!("MCP_ARGUMENT_INVALID: {error}"));
            system_id
                .and_then(|system_id| query.map(|query| (system_id, query)))
                .and_then(|(system_id, query)| {
                    authorize_project_read(store, project_id, scope_token, Some(&system_id))?;
                    store.query_domain_resources(project_id, &system_id, &query)
                })
                .map(|resources| {
                    json!({"resources": resources.into_iter().map(|resource| json!({
                        "id": resource.id,
                        "systemId": resource.system_id,
                        "resourceType": resource.resource_type,
                        "label": resource.label,
                        "writable": resource.writable,
                        "source": resource.source,
                        "dependencies": resource.dependencies,
                        "diagnostics": resource.diagnostics,
                        "fields": resource.fields,
                    })).collect::<Vec<_>>()})
                })
        }
        "mir3_resource_get" => {
            let system_id = required_string(&args, "systemId");
            let resource_id = required_string(&args, "resourceId");
            system_id
                .and_then(|system_id| resource_id.map(|resource_id| (system_id, resource_id)))
                .and_then(|(system_id, resource_id)| {
                    authorize_project_read(store, project_id, scope_token, Some(&system_id))?;
                    store.get_domain_resource(project_id, &system_id, &resource_id)
                })
                .map(|resource| json!({"resource": resource}))
        }
        "mir3_dependency_resolve" => required_string(&args, "systemId")
            .and_then(|system_id| {
                authorize_project_read(store, project_id, scope_token, Some(&system_id))?;
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
                    Ok((system_id, draft))
                })
                .map(|(system_id, draft)| {
                    json!({
                        "systemId": system_id,
                        "draftId": draft.id,
                        "revision": draft.revision,
                        "validation": Value::Null,
                        "changedResources": [],
                        "draft": draft
                    })
                })
        }
        "mir3_draft_diff" => required_string(&args, "draftId").and_then(|draft_id| {
            store.authorize_task_scope(project_id, scope_token, None, None, Some(&draft_id))?;
            let preview = store.preview_draft(project_id, &draft_id)?;
            Ok(json!({
                "draftId": draft_id,
                "revision": preview.draft.revision,
                "status": preview.draft.status,
                "diffHash": preview.diff_hash,
                "preview": &preview,
            }))
        }),
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
            store
                .authorize_task_scope(project_id, scope_token, system_id, None, None)
                .and_then(|lease| {
                    let systems = store.list_domain_systems()?;
                    let mut capabilities = systems
                        .into_iter()
                        .filter(|system| lease.read_systems.contains(&system.system_id))
                        .filter(|system| system_id.is_none_or(|id| id == system.system_id))
                        .flat_map(|system| {
                            system.capabilities.into_iter().map(move |capability| {
                                json!({
                                    "source":"official",
                                    "systemId":system.system_id,
                                    "id":capability.id,
                                    "version":capability.version,
                                    "readSystems":capability.read_systems,
                                    "writeSystems":capability.write_systems,
                                    "reversible":capability.reversible,
                                })
                            })
                        })
                        .collect::<Vec<_>>();
                    capabilities.extend(
                        store
                            .resolve_user_capabilities(project_id, system_id)?
                            .into_iter()
                            .filter(|resolution| {
                                let capability = &resolution.capability;
                                if capability.system_id == "__global__" {
                                    return lease.task_id.starts_with("global-")
                                        && capability
                                            .read_systems
                                            .iter()
                                            .all(|system| lease.read_systems.contains(system));
                                }
                                lease.read_systems.contains(&capability.system_id)
                            })
                            .map(|resolution| {
                                let capability = resolution.capability;
                                json!({
                                    "source":"user",
                                    "systemId":capability.system_id,
                                    "id":capability.id,
                                    "version":capability.version,
                                    "name":capability.name,
                                    "scope":capability.scope,
                                    "resolvedScope":resolution.resolved_scope,
                                    "sourceProjectId":resolution.source_project_id,
                                    "shadowedScopes":resolution.shadowed_scopes,
                                    "status":capability.status,
                                    "readSystems":capability.read_systems,
                                    "writeSystems":capability.write_systems,
                                })
                            }),
                    );
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
                    store.authorize_task_scope(
                        project_id,
                        scope_token,
                        Some(&found.0),
                        None,
                        None,
                    )?;
                    validate_requested_version(&args, &found.1.version)?;
                    return Ok(
                        json!({"source":"official","systemId":found.0,"capability":found.1}),
                    );
                }
                let requested_version = args.get("version").and_then(Value::as_str);
                let resolution =
                    store.resolve_user_capability(project_id, &capability_id, requested_version)?;
                if resolution.capability.system_id == "__global__" {
                    let lease =
                        store.authorize_task_scope(project_id, scope_token, None, None, None)?;
                    if !lease.task_id.starts_with("global-")
                        || resolution
                            .capability
                            .read_systems
                            .iter()
                            .any(|system| !lease.read_systems.contains(system))
                    {
                        return Err(
                            "TASK_SCOPE_READ_DENIED: global capability is outside the lease"
                                .to_string(),
                        );
                    }
                } else {
                    store.authorize_task_scope(
                        project_id,
                        scope_token,
                        Some(&resolution.capability.system_id),
                        None,
                        None,
                    )?;
                }
                Ok(json!({
                    "source":"user",
                    "systemId":resolution.capability.system_id,
                    "capability":resolution.capability,
                    "resolvedScope":resolution.resolved_scope,
                    "sourceProjectId":resolution.source_project_id,
                    "shadowedScopes":resolution.shadowed_scopes
                }))
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
                        let projection = parameters
                            .get("projection")
                            .and_then(Value::as_str)
                            .unwrap_or("merged");
                        let include_dependencies = parameters
                            .get("includeDependencies")
                            .and_then(Value::as_bool)
                            .or_else(|| {
                                parameters
                                    .get("includeDependencyValues")
                                    .and_then(Value::as_bool)
                            })
                            .unwrap_or(true);
                        let mut files = store.query_domain_files(
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
                        filter_files_for_projection(&mut files, projection)?;
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
                        let mut resources = resource_ids
                            .iter()
                            .map(|resource_id| {
                                store.get_domain_resource(project_id, &system_id, resource_id)
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        filter_resources_for_projection(&mut resources, projection)?;
                        if let Some(limit) = parameters.get("sampleLimit").and_then(Value::as_u64)
                        {
                            resources.truncate(limit as usize);
                        }
                        if !include_dependencies {
                            for resource in &mut resources {
                                resource.dependencies.clear();
                                resource.dependency_systems.clear();
                            }
                        }
                        let dependencies = include_dependencies
                            .then(|| store.resolve_domain_dependencies(&system_id))
                            .transpose()?;
                        let mut validation = (capability_id.starts_with("validate-")
                            || capability_id.starts_with("preview-"))
                        .then(|| store.validate_domain_system(project_id, &system_id))
                        .transpose()?;
                        let include_runtime_diagnostics = parameters
                            .get("includeRuntimeDiagnostics")
                            .and_then(Value::as_bool)
                            .unwrap_or(true);
                        if !include_runtime_diagnostics {
                            if let Some(report) = &mut validation {
                                report.validators.retain(|validator| {
                                    validator.kind != "runtime-diagnostics"
                                });
                                report.diagnostics.retain(|diagnostic| {
                                    !diagnostic.contains("runtime")
                                        && !diagnostic.contains("RUNTIME")
                                });
                            }
                        }
                        let target_engine = parameters
                            .get("targetEngineVersion")
                            .and_then(Value::as_str)
                            .map(|version| {
                                validate_target_engine(store, &system_id, version)
                            })
                            .transpose()?;
                        return Ok(json!({
                            "mode":"read",
                            "systemId":system_id,
                            "operation":capability_id,
                            "files":files,
                            "resources":resources,
                            "dependencies":dependencies,
                            "validation":validation,
                            "projection":projection,
                            "targetEngine":target_engine
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
                let requested_version = required_string(&args, "version")?;
                let user = store.get_user_capability(project_id, &capability_id, Some(&requested_version))?;
                if user.status != "active" {
                    return Err(format!(
                        "CAPABILITY_NOT_ACTIVE: {}@{}",
                        capability_id, user.version
                    ));
                }
                if user.system_id == "__global__" {
                    return invoke_global_user_capability(
                        store,
                        project_id,
                        scope_token,
                        &capability_id,
                        &requested_version,
                        &user,
                        &args,
                    );
                }
                let draft_id = required_string(&args, "draftId")?;
                let scoped = store.validate_user_capability_version_for_draft(
                    project_id,
                    &draft_id,
                    &capability_id,
                    Some(&requested_version),
                )?;
                let write_system = user.write_systems.first().ok_or_else(|| {
                    "CAPABILITY_WRITE_SYSTEM_REQUIRED: capability has no write scope".to_string()
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
                validate_json_schema(&user.parameter_schema, params, "params")?;
                let semantic_steps = user
                    .steps
                    .as_array()
                    .ok_or_else(|| "CAPABILITY_STEP_INVALID: steps must be an array".to_string())?;
                let manifest = store.draft_domain_manifest(project_id, &draft_id)?;
                let mut results = Vec::with_capacity(semantic_steps.len());
                for (index, semantic_step) in semantic_steps.iter().enumerate() {
                    let operation_id = semantic_step
                        .get("operation")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "CAPABILITY_STEP_INVALID: operation is required".to_string())?;
                    let operation = manifest
                        .operations
                        .iter()
                        .find(|operation| operation.id == operation_id)
                        .ok_or_else(|| format!("CAPABILITY_OPERATION_NOT_REGISTERED: {operation_id}"))?;
                    let supplied = if semantic_steps.len() == 1 {
                        params.clone()
                    } else {
                        params
                            .get(format!("step{index}"))
                            .cloned()
                            .ok_or_else(|| format!("CAPABILITY_PARAMETER_REQUIRED: step{index}"))?
                    };
                    let mut operation_params = supplied
                        .as_object()
                        .cloned()
                        .ok_or_else(|| format!("CAPABILITY_PARAMETER_INVALID: step{index} must be an object"))?;
                    operation_params.insert("operation".to_string(), Value::String(operation_id.to_string()));
                    operation_params.insert(
                        "expectedRevision".to_string(),
                        Value::from(store.get_draft(project_id, &draft_id)?.revision),
                    );
                    let operation_params = Value::Object(operation_params);
                    validate_json_schema(&operation.parameter_schema, &operation_params, "params")?;
                    results.push(execute_manifest_operation(
                        store,
                        project_id,
                        &draft_id,
                        operation_id,
                        &scoped.system_id,
                        &operation.steps,
                        &operation_params,
                    )?);
                }
                Ok(json!({
                    "capabilityId":capability_id,
                    "version":requested_version,
                    "draftId":draft_id,
                    "results":results,
                }))
            })
        }
        "mir3_validate" => store.validate_project(project_id).and_then(|project| {
            let system_id = args.get("systemId").and_then(Value::as_str);
            let draft = args.get("draftId").and_then(Value::as_str);
            store.authorize_task_scope(project_id, scope_token, system_id, None, draft)?;
            let domain = system_id
                .map(|system_id| store.validate_domain_system(project_id, system_id))
                .transpose()?;
            let draft_validation = draft
                .map(|draft_id| store.validate_domain_draft(project_id, draft_id))
                .transpose()?;
            if let (Some(system_id), Some(report)) = (system_id, draft_validation.as_ref()) {
                if report.system_id != system_id {
                    return Err(format!(
                        "DOMAIN_DRAFT_SYSTEM_MISMATCH: expected {system_id}, got {}",
                        report.system_id
                    ));
                }
            }
            let draft_preview = draft
                .map(|draft_id| store.preview_draft(project_id, draft_id))
                .transpose()?;
            let valid = project.status != mir3_domain::ProjectStatus::Missing
                && domain.as_ref().is_none_or(|report| report.valid)
                && draft_validation.as_ref().is_none_or(|report| report.valid);
            let revision = draft_preview.as_ref().map(|preview| preview.draft.revision);
            let changed_files = draft_preview
                .as_ref()
                .map(|preview| {
                    preview
                        .changes
                        .iter()
                        .map(|change| change.path.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let changed_resources = system_id
                .map(|system_id| changed_resource_ids(store, project_id, system_id, &changed_files))
                .unwrap_or_default();
            Ok(json!({
                "valid": valid,
                "systemId": system_id,
                "draftId": draft,
                "revision": revision,
                "validation": draft_validation.clone(),
                "changedFiles": changed_files,
                "changedResources": changed_resources,
                "warnings": project.warnings,
                "project": project,
                "domain": domain,
                "draft": draft_preview,
                "draftValidation": draft_validation
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

fn filter_files_for_projection(
    files: &mut Vec<mir3_domain::DomainFileRecord>,
    projection: &str,
) -> Result<(), String> {
    if projection == "merged" {
        return Ok(());
    }
    if !matches!(projection, "client" | "engine") {
        return Err(format!("DOMAIN_RESOURCE_PROJECTION_INVALID: {projection}"));
    }
    files.retain(|file| file.role == projection);
    Ok(())
}

fn filter_resources_for_projection(
    resources: &mut [DomainResourceRecord],
    projection: &str,
) -> Result<(), String> {
    if projection == "merged" {
        return Ok(());
    }
    if !matches!(projection, "client" | "engine") {
        return Err(format!("DOMAIN_RESOURCE_PROJECTION_INVALID: {projection}"));
    }
    for resource in resources.iter_mut() {
        resource.files.retain(|file| file.role == projection);
    }
    if resources.iter().any(|resource| resource.files.is_empty()) {
        return Err(format!(
            "DOMAIN_RESOURCE_PROJECTION_UNAVAILABLE: requested {projection} projection"
        ));
    }
    Ok(())
}

fn validate_target_engine(
    store: &DomainStore,
    system_id: &str,
    requested: &str,
) -> Result<Value, String> {
    let manifest = store
        .list_domain_systems()?
        .into_iter()
        .find(|manifest| manifest.system_id == system_id)
        .ok_or_else(|| format!("DOMAIN_SYSTEM_NOT_FOUND: {system_id}"))?;
    let normalized = if requested.matches('.').count() == 1 {
        format!("{requested}.0")
    } else {
        requested.to_string()
    };
    let version = Version::parse(&normalized)
        .map_err(|error| format!("DOMAIN_TARGET_ENGINE_INVALID: {requested}: {error}"))?;
    let requirement = VersionReq::parse(&manifest.supported_engine_range).map_err(|error| {
        format!(
            "DOMAIN_ENGINE_RANGE_INVALID: {}: {error}",
            manifest.supported_engine_range
        )
    })?;
    if !requirement.matches(&version) {
        return Err(format!(
            "DOMAIN_TARGET_ENGINE_INCOMPATIBLE: {version} does not match {} for {system_id}",
            manifest.supported_engine_range
        ));
    }
    Ok(json!({
        "requested": requested,
        "normalized": version.to_string(),
        "supportedRange": manifest.supported_engine_range,
        "compatible": true
    }))
}

fn invoke_global_user_capability(
    store: &DomainStore,
    project_id: &str,
    scope_token: &str,
    capability_id: &str,
    requested_version: &str,
    capability: &mir3_domain::UserCapability,
    args: &Value,
) -> Result<Value, String> {
    if args.get("draftId").is_some() {
        return Err(
            "GLOBAL_CAPABILITY_COMPOSITE_REQUIRED: global workflows do not accept draftId"
                .to_string(),
        );
    }
    let composite_id = required_string(args, "compositeId")?;
    let scoped = store.validate_global_capability_for_composite(
        project_id,
        &composite_id,
        capability_id,
        Some(requested_version),
    )?;
    let params = args
        .get("params")
        .ok_or_else(|| "MCP_ARGUMENT_INVALID: params is required".to_string())?;
    validate_json_schema(&scoped.parameter_schema, params, "params")?;
    let bindings = store.list_composite_draft_bindings(project_id, &composite_id)?;
    let semantic_steps = scoped
        .steps
        .as_array()
        .ok_or_else(|| "CAPABILITY_STEP_INVALID: steps must be an array".to_string())?;
    let mut parameter_counts = std::collections::BTreeMap::<String, usize>::new();
    for step in semantic_steps {
        let key = step
            .get("parameterKey")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "GLOBAL_CAPABILITY_PARAMETER_KEY_REQUIRED: recompile this workflow".to_string()
            })?;
        *parameter_counts.entry(key.to_string()).or_default() += 1;
    }
    let mut prepared = Vec::with_capacity(semantic_steps.len());
    for step in semantic_steps {
        let system_id = step
            .get("systemId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "GLOBAL_CAPABILITY_STEP_SYSTEM_REQUIRED: systemId missing".to_string()
            })?;
        let binding = bindings
            .iter()
            .find(|binding| binding.system_id == system_id)
            .ok_or_else(|| format!("GLOBAL_CAPABILITY_DRAFT_MISSING: {system_id}"))?;
        store.authorize_task_scope(
            project_id,
            scope_token,
            Some(system_id),
            Some(system_id),
            Some(&binding.draft_id),
        )?;
        let operation_id = step
            .get("operation")
            .and_then(Value::as_str)
            .ok_or_else(|| "CAPABILITY_STEP_INVALID: operation missing".to_string())?;
        let manifest = store.draft_domain_manifest(project_id, &binding.draft_id)?;
        let operation = manifest
            .operations
            .iter()
            .find(|operation| operation.id == operation_id)
            .ok_or_else(|| format!("CAPABILITY_OPERATION_NOT_REGISTERED: {operation_id}"))?;
        let parameter_key = step
            .get("parameterKey")
            .and_then(Value::as_str)
            .ok_or_else(|| "GLOBAL_CAPABILITY_PARAMETER_KEY_REQUIRED: missing key".to_string())?;
        let operation_index = step
            .get("operationIndex")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                "GLOBAL_CAPABILITY_OPERATION_INDEX_REQUIRED: missing index".to_string()
            })?;
        let system_params = params
            .get(parameter_key)
            .ok_or_else(|| format!("CAPABILITY_PARAMETER_REQUIRED: {parameter_key}"))?;
        let supplied = if parameter_counts
            .get(parameter_key)
            .copied()
            .unwrap_or_default()
            == 1
        {
            system_params.clone()
        } else {
            system_params
                .get(format!("step{operation_index}"))
                .cloned()
                .ok_or_else(|| {
                    format!("CAPABILITY_PARAMETER_REQUIRED: {parameter_key}.step{operation_index}")
                })?
        };
        let mut operation_params = supplied.as_object().cloned().ok_or_else(|| {
            format!("CAPABILITY_PARAMETER_INVALID: {parameter_key} must be an object")
        })?;
        operation_params.insert(
            "operation".to_string(),
            Value::String(operation_id.to_string()),
        );
        operation_params.insert(
            "expectedRevision".to_string(),
            Value::from(store.get_draft(project_id, &binding.draft_id)?.revision),
        );
        let operation_params = Value::Object(operation_params);
        validate_json_schema(&operation.parameter_schema, &operation_params, "params")?;
        prepared.push((
            binding.draft_id.clone(),
            system_id.to_string(),
            operation_id.to_string(),
            operation.steps.clone(),
            operation_params,
        ));
    }
    let (results, drafts) =
        store.with_composite_draft_transaction(project_id, &composite_id, |_| {
            let mut results = Vec::with_capacity(prepared.len());
            for (draft_id, system_id, operation_id, steps, mut operation_params) in prepared {
                operation_params["expectedRevision"] =
                    Value::from(store.get_draft(project_id, &draft_id)?.revision);
                let result = execute_manifest_operation(
                    store,
                    project_id,
                    &draft_id,
                    &operation_id,
                    &system_id,
                    &steps,
                    &operation_params,
                )?;
                results.push(compact_global_operation_result(result));
            }
            let drafts = store
                .list_composite_draft_bindings(project_id, &composite_id)?
                .into_iter()
                .map(|binding| {
                    let validation = store.validate_domain_draft(project_id, &binding.draft_id)?;
                    Ok(json!({
                        "draftId":binding.draft_id,
                        "systemId":binding.system_id,
                        "pluginVersion":binding.plugin_version,
                        "revision":binding.revision,
                        "validation":{
                            "valid":validation.valid,
                            "diagnostics":validation.diagnostics,
                            "validatorCount":validation.validators.len(),
                        },
                    }))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok((results, drafts))
        })?;
    Ok(json!({
        "capabilityId":capability_id,
        "version":requested_version,
        "compositeId":composite_id,
        "writeSystems":capability.write_systems,
        "drafts":drafts,
        "results":results,
    }))
}

/// 全局工作流只返回编排所需的 Draft 交接摘要；完整 Diff 与校验报告由稳定工具按 Draft 查询。
/// 避免 33 系统同时执行时重复嵌套每个 primitive 的完整 Preview，挤爆 MCP 上下文预算。
fn compact_global_operation_result(result: Value) -> Value {
    json!({
        "operation":result.get("operation"),
        "draftId":result.get("draftId"),
        "systemId":result.get("systemId"),
        "revision":result.get("revision"),
        "changedFiles":result.get("changedFiles"),
        "changedResources":result.get("changedResources"),
        "valid":result.pointer("/validation/valid"),
        "primitiveCount":result
            .get("results")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default(),
    })
}

fn system_list_payload(systems: Vec<DomainManifest>) -> Value {
    json!({"systems": systems.into_iter().map(|system| json!({
        "systemId": system.system_id,
        "version": system.version,
        "category": system.category,
        "renderer": system.renderer,
        "dependencies": system.dependencies,
        "capabilityCount": system.capabilities.len(),
    })).collect::<Vec<_>>()})
}

fn tool_definitions() -> Vec<Value> {
    vec![
        tool(
            "mir3_gui_context",
            "读取 Studio 已同步到应用私有目录的当前 GUI 工作副本、控件树和选择上下文；不会读取项目源文件。",
            gui_path_schema(false),
        ),
        tool(
            "mir3_gui_operate",
            "使用 expectedRevision 对私有 GUI 工作副本执行属性、位置、核心组件或行为操作；不会写入项目文件。",
            json!({
                "type":"object",
                "properties":{
                    "workspaceToken":{"type":"string","minLength":32},
                    "path":{"type":"string","minLength":1},
                    "expectedRevision":{"type":"integer","minimum":0},
                    "operation":{
                        "type":"object",
                        "properties":{
                            "type":{"type":"string","enum":["setPosition","setProperty","addNode","addBehavior"]},
                            "nodeId":{"type":"string","minLength":1},
                            "parentNodeId":{"type":"string","minLength":1},
                            "nodeType":{"type":"string","enum":["Panel","Image","Text","Button"]},
                            "name":{"type":"string","minLength":1},
                            "x":{"type":"number"},
                            "y":{"type":"number"},
                            "property":{"type":"string","minLength":1},
                            "value":{},
                            "behavior":{"type":"string","enum":["timeline","action"]}
                        },
                        "required":["type"],
                        "additionalProperties":false
                    }
                },
                "required":["workspaceToken","path","expectedRevision","operation"],
                "additionalProperties":false
            }),
        ),
        tool(
            "mir3_gui_asset_query",
            "列出私有 GUI 工作副本静态引用的素材逻辑路径；不读取或导出游戏加密资源。",
            json!({"type":"object","properties":{"workspaceToken":{"type":"string","minLength":32},"path":{"type":"string","minLength":1},"query":{"type":"string"}},"required":["workspaceToken","path"],"additionalProperties":false}),
        ),
        tool(
            "mir3_gui_validate",
            "按 expectedRevision 校验私有 GUI 工作副本并返回静态诊断；不会保存到项目。",
            gui_path_schema(true),
        ),
        tool(
            "mir3_system_list",
            "列出当前项目可读的 33 个领域系统；普通工作台无需令牌，受管任务可附带作用域令牌收窄结果。",
            with_project_read(empty_schema()),
        ),
        tool(
            "mir3_system_describe",
            "返回一个领域包的版本、真实文件覆盖、依赖、视图和安全能力。",
            with_project_read(system_schema()),
        ),
        tool(
            "mir3_resource_query",
            "按领域分页查询表格行或文本记录资源，同时返回真实文件来源和依赖诊断。",
            with_project_read(
                json!({"type":"object","properties":{"systemId":{"type":"string"},"text":{"type":"string"},"resourceType":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":MCP_MAX_QUERY_ITEMS},"offset":{"type":"integer","minimum":0}},"required":["systemId"],"additionalProperties":false}),
            ),
        ),
        tool(
            "mir3_resource_get",
            "通过稳定资源 ID 读取领域资源及安全解码后的文件内容；996 的 GBK/GB18030 脚本应使用此工具，不要使用仅支持 UTF-8 的通用 read。",
            with_project_read(
                json!({"type":"object","properties":{"systemId":{"type":"string"},"resourceId":{"type":"string"}},"required":["systemId","resourceId"],"additionalProperties":false}),
            ),
        ),
        tool(
            "mir3_dependency_resolve",
            "返回当前系统可读依赖范围。",
            with_project_read(system_schema()),
        ),
        tool(
            "mir3_draft_open",
            "为指定系统创建外置 Draft；不会修改正式项目。",
            with_scope(
                json!({"type":"object","properties":{"systemId":{"type":"string"},"intent":{"type":"string","minLength":1}},"required":["systemId","intent"],"additionalProperties":false}),
            ),
        ),
        tool(
            "mir3_draft_diff",
            "返回作用域内 Draft 的稳定预览、统一 Diff、内容哈希和当前 revision。",
            with_scope(
                json!({"type":"object","properties":{"draftId":{"type":"string","minLength":1}},"required":["draftId"],"additionalProperties":false}),
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
                json!({"type":"object","properties":{"capabilityId":{"type":"string"},"version":{"type":"string"}},"required":["capabilityId"],"additionalProperties":false}),
            ),
        ),
        tool(
            "mir3_capability_invoke",
            "在外置 Draft 中调用一个安全领域能力。",
            with_scope(
                json!({"type":"object","properties":{"capabilityId":{"type":"string"},"version":{"type":"string"},"systemId":{"type":"string"},"draftId":{"type":"string"},"compositeId":{"type":"string"},"params":{"type":"object"}},"required":["capabilityId","params"],"additionalProperties":false}),
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

fn gui_path_schema(require_revision: bool) -> Value {
    let mut schema = json!({
        "type":"object",
        "properties":{
            "workspaceToken":{"type":"string","minLength":32},
            "path":{"type":"string","minLength":1},
            "expectedRevision":{"type":"integer","minimum":0}
        },
        "required":["workspaceToken","path"],
        "additionalProperties":false
    });
    if require_revision {
        schema["required"] = json!(["workspaceToken", "path", "expectedRevision"]);
    }
    schema
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

fn with_project_read(mut schema: Value) -> Value {
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "scopeToken".to_string(),
            json!({"type":"string","minLength":32}),
        );
    }
    schema
}

fn authorize_project_read(
    store: &DomainStore,
    project_id: &str,
    scope_token: &str,
    system_id: Option<&str>,
) -> Result<Option<mir3_domain::TaskScopeLease>, String> {
    if scope_token.is_empty() {
        store.get_project(project_id)?;
        return Ok(None);
    }
    store
        .authorize_task_scope(project_id, scope_token, system_id, None, None)
        .map(Some)
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
    if let Some(branches) = schema.get("oneOf").and_then(Value::as_array) {
        let matches = branches
            .iter()
            .filter(|branch| validate_json_schema(branch, value, path).is_ok())
            .count();
        if matches != 1 {
            return Err(format!("CAPABILITY_PARAMETER_ONE_OF_INVALID: {path}"));
        }
    }
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
                "^[A-Za-z0-9_\\-]+$" => {
                    !string.is_empty()
                        && string.chars().all(|value| {
                            value.is_ascii_alphanumeric() || matches!(value, '_' | '-')
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
    validate_manifest_operation_steps(operation_id, steps)?;
    let revision_before = store.get_draft(project_id, draft_id)?.revision;
    let mut result = execute_manifest_operation_unrecorded(
        store,
        project_id,
        draft_id,
        operation_id,
        system_id,
        steps,
        params,
    )?;
    let revision_after = store.get_draft(project_id, draft_id)?.revision;
    let evidence = store.record_draft_operation_evidence(
        project_id,
        draft_id,
        operation_id,
        params,
        revision_before,
        revision_after,
    )?;
    let replay_change_hash = replay_manifest_operation_chain(store, project_id, draft_id)?;
    store.seal_draft_operation_replay(
        project_id,
        draft_id,
        evidence.sequence,
        &replay_change_hash,
    )?;
    enrich_draft_handoff_result(store, project_id, draft_id, system_id, &mut result);
    Ok(result)
}

/// 工具结果显式携带可验证的 Draft 交接字段，避免 Studio 从自由文本或内部步骤猜测状态。
fn enrich_draft_handoff_result(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    system_id: &str,
    result: &mut Value,
) {
    let revision = store
        .get_draft(project_id, draft_id)
        .map(|draft| draft.revision)
        .unwrap_or_default();
    let preview = store.preview_draft(project_id, draft_id).ok();
    let changed_files = preview
        .as_ref()
        .map(|preview| {
            preview
                .changes
                .iter()
                .map(|change| change.path.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let changed_resources = changed_resource_ids(store, project_id, system_id, &changed_files);
    let validation = store.validate_domain_draft(project_id, draft_id).ok();
    if !result.is_object() {
        let original = result.take();
        *result = json!({ "result": original });
    }
    let object = result
        .as_object_mut()
        .expect("new result envelope is an object");
    object.insert("draftId".into(), Value::String(draft_id.to_string()));
    object.insert("systemId".into(), Value::String(system_id.to_string()));
    object.insert("revision".into(), Value::from(revision));
    object.insert("changedFiles".into(), json!(changed_files));
    object.insert("changedResources".into(), json!(changed_resources));
    object.insert("validation".into(), json!(validation));
}

fn changed_resource_ids(
    store: &DomainStore,
    project_id: &str,
    system_id: &str,
    changed_files: &[String],
) -> Vec<String> {
    store
        .query_domain_resources(
            project_id,
            system_id,
            &DomainResourceQuery {
                text: String::new(),
                resource_type: None,
                limit: Some(10_000),
                offset: None,
            },
        )
        .unwrap_or_default()
        .into_iter()
        .filter(|resource| {
            resource
                .files
                .iter()
                .any(|file| changed_files.iter().any(|path| path == &file.path))
        })
        .map(|resource| resource.id)
        .collect()
}

fn replay_manifest_operation_chain(
    store: &DomainStore,
    project_id: &str,
    source_draft_id: &str,
) -> Result<String, String> {
    let manifest = store.draft_domain_manifest(project_id, source_draft_id)?;
    let evidence = store.list_draft_operation_evidence(project_id, source_draft_id)?;
    let replay = store.open_draft(project_id, "capability operation replay")?;
    store.bind_draft_domain(
        project_id,
        &replay.id,
        &manifest.system_id,
        &manifest.version,
        None,
    )?;
    let replay_result = (|| -> Result<String, String> {
        for item in &evidence {
            let operation = manifest
                .operations
                .iter()
                .find(|operation| operation.id == item.operation_id)
                .ok_or_else(|| {
                    format!("CAPABILITY_OPERATION_NOT_REGISTERED: {}", item.operation_id)
                })?;
            let mut parameters = item.parameters.as_object().cloned().ok_or_else(|| {
                "CAPABILITY_REPLAY_PARAMETERS_INVALID: expected an object".to_string()
            })?;
            parameters.insert(
                "expectedRevision".to_string(),
                Value::from(store.get_draft(project_id, &replay.id)?.revision),
            );
            let parameters = Value::Object(parameters);
            validate_json_schema(&operation.parameter_schema, &parameters, "params")?;
            execute_manifest_operation_unrecorded(
                store,
                project_id,
                &replay.id,
                &item.operation_id,
                &manifest.system_id,
                &operation.steps,
                &parameters,
            )?;
        }
        let source_report = store.validate_domain_draft(project_id, source_draft_id)?;
        let replay_report = store.validate_domain_draft(project_id, &replay.id)?;
        if source_report.valid != replay_report.valid
            || normalized_replay_diagnostics(&source_report.diagnostics)
                != normalized_replay_diagnostics(&replay_report.diagnostics)
        {
            return Err(
                "CAPABILITY_REPLAY_VALIDATION_MISMATCH: isolated replay produced different validation diagnostics"
                    .to_string(),
            );
        }
        let source_hash = store.draft_change_evidence_hash(project_id, source_draft_id)?;
        let replay_hash = store.draft_change_evidence_hash(project_id, &replay.id)?;
        if source_hash != replay_hash {
            return Err(
                "CAPABILITY_REPLAY_DIFF_MISMATCH: replay output differs from source Draft"
                    .to_string(),
            );
        }
        Ok(replay_hash)
    })();
    store.discard_draft(project_id, &replay.id).ok();
    replay_result
}

fn normalized_replay_diagnostics(diagnostics: &[String]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            if diagnostic.starts_with("DOMAIN_DRAFT_OVERLAY_VALIDATED:") {
                "DOMAIN_DRAFT_OVERLAY_VALIDATED".to_string()
            } else {
                diagnostic.clone()
            }
        })
        .collect()
}

fn validate_manifest_operation_steps(
    operation_id: &str,
    steps: &[mir3_domain::DomainCapabilityStep],
) -> Result<(), String> {
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
    Ok(())
}

fn execute_manifest_operation_unrecorded(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    operation_id: &str,
    system_id: &str,
    steps: &[mir3_domain::DomainCapabilityStep],
    params: &Value,
) -> Result<Value, String> {
    validate_manifest_operation_steps(operation_id, steps)?;
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
    let manifest = store.draft_domain_manifest(project_id, draft_id)?;
    if manifest.system_id != system_id {
        return Err(format!(
            "DOMAIN_COMPILER_DRAFT_SYSTEM_MISMATCH: requested {system_id}, draft is scoped to {}",
            manifest.system_id
        ));
    }
    if !manifest
        .operations
        .iter()
        .any(|operation| operation.id == operation_id)
    {
        return Err(format!(
            "DOMAIN_OPERATION_NOT_REGISTERED: {system_id}:{operation_id}"
        ));
    }
    if let Some(result) = execute_shaped_operation(
        store,
        project_id,
        draft_id,
        operation_id,
        &manifest,
        params,
        revision,
    )? {
        return Ok(result);
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
                    draft_id,
                    &resource,
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

fn execute_shaped_operation(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    operation_id: &str,
    manifest: &DomainManifest,
    params: &Value,
    mut revision: i64,
) -> Result<Option<Value>, String> {
    let family = operation_id.split('-').next().unwrap_or_default();
    if operation_id == "edit-map-region" {
        let resource_id = required_string(params, "resourceId")?;
        let resource = writable_resource(store, project_id, &manifest.system_id, &resource_id)?;
        let file = resource
            .files
            .first()
            .ok_or_else(|| "DOMAIN_COMPILER_RESOURCE_FILE_REQUIRED: map resource".to_string())?;
        if !file
            .extension
            .as_deref()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("map"))
        {
            return Err("DOMAIN_COMPILER_MAP_REQUIRED: edit-map-region requires .map".to_string());
        }
        let opened = store.map_resource_open(project_id, &file.path, Some(draft_id), None)?;
        let operation = serde_json::from_value::<MapDraftOperation>(json!({
            "path": file.path,
            "expectedSha256": opened.header.source_sha256,
            "operations": params.get("operations").cloned().ok_or_else(|| "MCP_ARGUMENT_INVALID: operations is required".to_string())?
        }))
        .map_err(|error| format!("DOMAIN_COMPILER_MAP_INVALID: {error}"))?;
        let result = apply_safe_operation(
            store,
            project_id,
            draft_id,
            revision,
            &serde_json::to_value(operation)
                .map(|mut value| {
                    value["type"] = json!("map.edit");
                    value
                })
                .map_err(|error| format!("DOMAIN_COMPILER_MAP_INVALID: {error}"))?,
        )?;
        revision = primitive_revision(&result)?;
        return Ok(Some(compiled_result(operation_id, revision, vec![result])));
    }

    if !is_shaped_operation(operation_id) {
        return Ok(None);
    }

    let mut results = Vec::new();
    let mut parameter_semantics = serde_json::Map::new();
    match family {
        "clone" => {
            let source_id = required_string(params, "sourceResourceId")?;
            let new_id = required_string(params, "newResourceId")?;
            let source = writable_resource(store, project_id, &manifest.system_id, &source_id)?;
            let mut changes = required_object_or_empty(params, "overrides")?;
            let primary = manifest
                .resources
                .unique_key
                .first()
                .ok_or_else(|| "DOMAIN_COMPILER_UNIQUE_KEY_REQUIRED: clone".to_string())?;
            changes.insert(primary.clone(), Value::String(new_id.clone()));
            let compiled = clone_text_resource(
                store,
                project_id,
                draft_id,
                CloneTextResourceInput {
                    source: &source,
                    target_id: &new_id,
                    target_path_id: None,
                    changes: &changes,
                    revision,
                },
            )?;
            revision = compiled.0;
            results.extend(compiled.1);
        }
        "generate" => {
            let template_id = required_string(params, "templateResourceId")?;
            let first = required_u64(params, "firstOrdinal")?;
            let last = required_u64(params, "lastOrdinal")?;
            if first > last {
                return Err(
                    "DOMAIN_COMPILER_RANGE_INVALID: firstOrdinal exceeds lastOrdinal".to_string(),
                );
            }
            let source = writable_resource(store, project_id, &manifest.system_id, &template_id)?;
            let patch = required_object_or_empty(params, "generatedPatch")?;
            let interpolation = params
                .get("interpolation")
                .and_then(Value::as_str)
                .unwrap_or("linear");
            if !matches!(interpolation, "linear" | "geometric" | "step") {
                return Err(format!(
                    "DOMAIN_COMPILER_INTERPOLATION_INVALID: {interpolation}"
                ));
            }
            parameter_semantics.insert(
                "interpolation".to_string(),
                Value::String(interpolation.to_string()),
            );
            let primary = manifest
                .resources
                .unique_key
                .first()
                .ok_or_else(|| "DOMAIN_COMPILER_UNIQUE_KEY_REQUIRED: generate".to_string())?;
            for ordinal in first..=last {
                let target_id = format!("generated-{ordinal}");
                let ratio = if first == last {
                    1.0
                } else {
                    (ordinal - first) as f64 / (last - first) as f64
                };
                let mut changes = interpolated_generated_patch(
                    store,
                    project_id,
                    draft_id,
                    &source,
                    &patch,
                    interpolation,
                    ratio,
                )?;
                changes.insert(
                    primary.clone(),
                    generated_key_value(
                        store, project_id, draft_id, &source, primary, &target_id, ordinal,
                    )?,
                );
                let compiled = clone_text_resource(
                    store,
                    project_id,
                    draft_id,
                    CloneTextResourceInput {
                        source: &source,
                        target_id: &target_id,
                        target_path_id: None,
                        changes: &changes,
                        revision,
                    },
                )?;
                revision = compiled.0;
                results.extend(compiled.1);
            }
        }
        "scale" | "tune" => {
            let resource_ids = required_string_array(params, "resourceIds")?;
            let fields = required_string_array(params, "fields")?;
            for resource_id in resource_ids {
                let resource =
                    writable_resource(store, project_id, &manifest.system_id, &resource_id)?;
                let mut changes = serde_json::Map::new();
                for field in &fields {
                    let current =
                        read_numeric_field(store, project_id, draft_id, &resource, field)?;
                    let changed = if family == "scale" {
                        let factor = required_f64(params, "factor")?;
                        rounded_number(
                            current * factor,
                            params.get("rounding").and_then(Value::as_str),
                        )?
                    } else {
                        let amount = required_f64(params, "amount")?;
                        match required_string(params, "adjustmentMode")?.as_str() {
                            "absolute" => number_value(amount)?,
                            "delta" => number_value(current + amount)?,
                            "percentage" => number_value(current * (1.0 + amount / 100.0))?,
                            mode => return Err(format!("DOMAIN_COMPILER_MODE_INVALID: {mode}")),
                        }
                    };
                    changes.insert(field.clone(), changed);
                }
                let result = apply_resource_changes(
                    store, project_id, draft_id, &resource, &changes, None, revision,
                )?;
                revision = primitive_revision(&result)?;
                results.push(result);
            }
        }
        "interpolate" => {
            let anchors = required_string_array(params, "anchorResourceIds")?;
            if anchors.is_empty() {
                return Err(
                    "DOMAIN_COMPILER_ANCHORS_INVALID: at least one anchor is required".to_string(),
                );
            }
            let first = required_u64(params, "firstOrdinal")?;
            let last = required_u64(params, "lastOrdinal")?;
            if first > last {
                return Err(
                    "DOMAIN_COMPILER_RANGE_INVALID: firstOrdinal exceeds lastOrdinal".to_string(),
                );
            }
            let fields = required_string_array(params, "numericFields")?;
            let anchors = anchors
                .iter()
                .map(|anchor| writable_resource(store, project_id, &manifest.system_id, anchor))
                .collect::<Result<Vec<_>, _>>()?;
            for ordinal in first..=last {
                let ratio = if first == last {
                    0.0
                } else {
                    (ordinal - first) as f64 / (last - first) as f64
                };
                let anchor_position = ratio * anchors.len().saturating_sub(1) as f64;
                let lower = anchor_position.floor() as usize;
                let upper = anchor_position.ceil() as usize;
                let local_ratio = anchor_position - lower as f64;
                let source = &anchors[lower];
                let mut changes = serde_json::Map::new();
                for field in &fields {
                    let from = read_numeric_field(store, project_id, draft_id, source, field)?;
                    let to =
                        read_numeric_field(store, project_id, draft_id, &anchors[upper], field)?;
                    changes.insert(
                        field.clone(),
                        number_value(from + (to - from) * local_ratio)?,
                    );
                }
                let primary = manifest.resources.unique_key.first().ok_or_else(|| {
                    "DOMAIN_COMPILER_UNIQUE_KEY_REQUIRED: interpolate".to_string()
                })?;
                changes.insert(
                    primary.clone(),
                    generated_key_value(
                        store,
                        project_id,
                        draft_id,
                        source,
                        primary,
                        &format!("interpolated-{ordinal}"),
                        ordinal,
                    )?,
                );
                let target_id = format!("interpolated-{ordinal}");
                let compiled = clone_text_resource(
                    store,
                    project_id,
                    draft_id,
                    CloneTextResourceInput {
                        source,
                        target_id: &target_id,
                        target_path_id: None,
                        changes: &changes,
                        revision,
                    },
                )?;
                revision = compiled.0;
                results.extend(compiled.1);
            }
        }
        "add" | "insert" => {
            let anchor_key = if family == "add" {
                "insertAfterResourceId"
            } else {
                "parentResourceId"
            };
            let source = match params.get(anchor_key) {
                Some(_) => {
                    let anchor_id = required_string(params, anchor_key)?;
                    writable_resource(store, project_id, &manifest.system_id, &anchor_id)?
                }
                None => default_writable_resource(store, project_id, &manifest.system_id)?,
            };
            let record = required_object(params, "record")?;
            let primary = manifest
                .resources
                .unique_key
                .first()
                .ok_or_else(|| format!("DOMAIN_COMPILER_UNIQUE_KEY_REQUIRED: {family}"))?;
            let target_id = portable_scalar_id(
                record
                    .get(primary)
                    .ok_or_else(|| format!("DOMAIN_COMPILER_PRIMARY_KEY_REQUIRED: {primary}"))?,
            )?;
            let target_path_id = if family == "insert" {
                Some(format!(
                    "insert-{:06}-{target_id}",
                    required_u64(params, "insertionIndex")?
                ))
            } else {
                None
            };
            let compiled = clone_text_resource(
                store,
                project_id,
                draft_id,
                CloneTextResourceInput {
                    source: &source,
                    target_id: &target_id,
                    target_path_id: target_path_id.as_deref(),
                    changes: &record,
                    revision,
                },
            )?;
            revision = compiled.0;
            results.extend(compiled.1);
        }
        "fill" => {
            let cycle_id = required_string(params, "cycleResourceId")?;
            let first = required_u64(params, "firstSlot")?;
            let last = required_u64(params, "lastSlot")?;
            if first > last {
                return Err("DOMAIN_COMPILER_RANGE_INVALID: firstSlot exceeds lastSlot".to_string());
            }
            let source = writable_resource(store, project_id, &manifest.system_id, &cycle_id)?;
            let patch = required_object(params, "rewardTemplate")?;
            for slot in first..=last {
                let mut changes = patch.clone();
                changes.insert("dayIndex".to_string(), ordinal_value(slot));
                let target_id = format!("slot-{slot}");
                let compiled = clone_text_resource(
                    store,
                    project_id,
                    draft_id,
                    CloneTextResourceInput {
                        source: &source,
                        target_id: &target_id,
                        target_path_id: None,
                        changes: &changes,
                        revision,
                    },
                )?;
                revision = compiled.0;
                results.extend(compiled.1);
            }
        }
        "bind" => {
            let resource_id = required_string(params, "resourceId")?;
            let field = required_string(params, "referenceField")?;
            let target = required_string(params, "targetReference")?;
            let resource = writable_resource(store, project_id, &manifest.system_id, &resource_id)?;
            parameter_semantics.insert(
                "replaceExisting".to_string(),
                Value::Bool(
                    params
                        .get("replaceExisting")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                ),
            );
            let changes = serde_json::Map::from_iter([(field, Value::String(target))]);
            let result = apply_resource_changes(
                store, project_id, draft_id, &resource, &changes, None, revision,
            )?;
            revision = primitive_revision(&result)?;
            results.push(result);
        }
        "move" => {
            let resource_id = required_string(params, "resourceId")?;
            let resource = writable_resource(store, project_id, &manifest.system_id, &resource_id)?;
            let changes = serde_json::Map::from_iter([
                (
                    "mapId".to_string(),
                    Value::String(required_string(params, "destinationMapId")?),
                ),
                (
                    "coordinateX".to_string(),
                    ordinal_value(required_u64(params, "coordinateX")?),
                ),
                (
                    "coordinateY".to_string(),
                    ordinal_value(required_u64(params, "coordinateY")?),
                ),
            ]);
            let result = apply_resource_changes(
                store, project_id, draft_id, &resource, &changes, None, revision,
            )?;
            revision = primitive_revision(&result)?;
            results.push(result);
        }
        "schedule" => {
            let ids = required_string_array(params, "resourceIds")?;
            let timezone = required_string(params, "timezone")?;
            parameter_semantics.insert("timezone".to_string(), Value::String(timezone));
            parameter_semantics.insert(
                "storage".to_string(),
                Value::String("epoch-seconds".to_string()),
            );
            let changes = serde_json::Map::from_iter([
                (
                    "startEpochSeconds".to_string(),
                    ordinal_value(required_u64(params, "startEpochSeconds")?),
                ),
                (
                    "endEpochSeconds".to_string(),
                    ordinal_value(required_u64(params, "endEpochSeconds")?),
                ),
            ]);
            for id in ids {
                let resource = writable_resource(store, project_id, &manifest.system_id, &id)?;
                let result = apply_resource_changes(
                    store, project_id, draft_id, &resource, &changes, None, revision,
                )?;
                revision = primitive_revision(&result)?;
                results.push(result);
            }
        }
        "shift" => {
            let ids = required_string_array(params, "resourceIds")?;
            let offset_seconds = required_i64(params, "offsetSeconds")?;
            for id in ids {
                let resource = writable_resource(store, project_id, &manifest.system_id, &id)?;
                let changes = if operation_id == "shift-launch-schedule" {
                    if offset_seconds % 86_400 != 0 {
                        return Err(
                            "DOMAIN_COMPILER_LAUNCH_OFFSET_INVALID: offsetSeconds must be whole days"
                                .to_string(),
                        );
                    }
                    let open_server_day = read_numeric_field(
                        store,
                        project_id,
                        draft_id,
                        &resource,
                        "openServerDay",
                    )?;
                    serde_json::Map::from_iter([(
                        "openServerDay".to_string(),
                        number_value(open_server_day + offset_seconds as f64 / 86_400.0)?,
                    )])
                } else {
                    let offset = offset_seconds as f64;
                    let start = read_numeric_field(
                        store,
                        project_id,
                        draft_id,
                        &resource,
                        "startEpochSeconds",
                    )?;
                    let end = read_numeric_field(
                        store,
                        project_id,
                        draft_id,
                        &resource,
                        "endEpochSeconds",
                    )?;
                    serde_json::Map::from_iter([
                        (
                            "startEpochSeconds".to_string(),
                            number_value(start + offset)?,
                        ),
                        ("endEpochSeconds".to_string(), number_value(end + offset)?),
                    ])
                };
                let result = apply_resource_changes(
                    store, project_id, draft_id, &resource, &changes, None, revision,
                )?;
                revision = primitive_revision(&result)?;
                results.push(result);
            }
        }
        _ => unreachable!(),
    }
    let mut compiled = compiled_result(operation_id, revision, results);
    if !parameter_semantics.is_empty() {
        compiled["parameterSemantics"] = Value::Object(parameter_semantics);
    }
    Ok(Some(compiled))
}

fn is_shaped_operation(operation_id: &str) -> bool {
    if operation_id == "edit-map-region" {
        return true;
    }
    matches!(
        operation_id.split('-').next().unwrap_or_default(),
        "clone"
            | "generate"
            | "scale"
            | "interpolate"
            | "add"
            | "insert"
            | "fill"
            | "tune"
            | "bind"
            | "move"
            | "schedule"
            | "shift"
    )
}

struct CloneTextResourceInput<'a> {
    source: &'a DomainResourceRecord,
    target_id: &'a str,
    target_path_id: Option<&'a str>,
    changes: &'a serde_json::Map<String, Value>,
    revision: i64,
}

fn clone_text_resource(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    input: CloneTextResourceInput<'_>,
) -> Result<(i64, Vec<Value>), String> {
    let file = input
        .source
        .files
        .first()
        .ok_or_else(|| "DOMAIN_COMPILER_RESOURCE_FILE_REQUIRED: clone".to_string())?;
    let extension = file.extension.as_deref().unwrap_or_default();
    if !matches!(extension.to_ascii_lowercase().as_str(), "txt" | "lua") {
        return Err("DOMAIN_COMPILER_CLONE_TYPE_UNSUPPORTED: expected TXT or Lua".to_string());
    }
    let target_path =
        sibling_resource_path(&file.path, input.target_path_id.unwrap_or(input.target_id))?;
    let opened = store.safe_text_open(project_id, &file.path, Some(draft_id))?;
    let replacements = input
        .changes
        .iter()
        .map(|(field, value)| {
            let aliases = manifest_field_aliases(store, project_id, draft_id, field)?;
            let (old, new, _) =
                field_line_replacement_with_aliases(&opened.content, field, &aliases, value)?;
            Ok(json!({"old":old,"new":new,"expectedCount":1}))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let cloned = apply_safe_operation(
        store,
        project_id,
        draft_id,
        input.revision,
        &json!({
            "type":"resource.clone",
            "sourcePath":file.path,
            "targetPath":target_path,
            "expectedSha256":opened.sha256,
            "replacements":replacements
        }),
    )?;
    let revision = primitive_revision(&cloned)?;
    Ok((revision, vec![cloned]))
}

fn apply_resource_changes(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    resource: &DomainResourceRecord,
    changes: &serde_json::Map<String, Value>,
    expected_reference: Option<&(String, String)>,
    revision: i64,
) -> Result<Value, String> {
    if changes.is_empty() {
        return Err("DOMAIN_COMPILER_CHANGES_REQUIRED: no field changes".to_string());
    }
    let file = resource
        .files
        .first()
        .ok_or_else(|| "DOMAIN_COMPILER_RESOURCE_FILE_REQUIRED: changes".to_string())?;
    let extension = file.extension.as_deref().unwrap_or_default();
    let primitive = if matches!(extension.to_ascii_lowercase().as_str(), "txt" | "lua") {
        compile_text_field_changes(
            store,
            project_id,
            draft_id,
            &file.path,
            changes,
            expected_reference,
        )?
    } else if extension.eq_ignore_ascii_case("xls") {
        compile_xls_field_changes(
            store,
            project_id,
            draft_id,
            resource,
            changes,
            expected_reference,
        )?
    } else {
        return Err(format!("DOMAIN_COMPILER_WRITER_UNSUPPORTED: {}", file.path));
    };
    apply_safe_operation(store, project_id, draft_id, revision, &primitive)
}

fn writable_resource(
    store: &DomainStore,
    project_id: &str,
    system_id: &str,
    resource_id: &str,
) -> Result<DomainResourceRecord, String> {
    let resource = store.get_domain_resource(project_id, system_id, resource_id)?;
    if !resource.writable || resource.files.len() != 1 {
        return Err(format!("DOMAIN_COMPILER_RESOURCE_READONLY: {resource_id}"));
    }
    Ok(resource)
}

fn default_writable_resource(
    store: &DomainStore,
    project_id: &str,
    system_id: &str,
) -> Result<DomainResourceRecord, String> {
    let file = store
        .query_domain_files(
            project_id,
            system_id,
            &DomainFileQuery {
                text: String::new(),
                limit: Some(10_000),
                offset: None,
            },
        )?
        .into_iter()
        .find(|file| file.ownership != "dependency" && file.access != "readonly")
        .ok_or_else(|| {
            format!("DOMAIN_COMPILER_TEMPLATE_REQUIRED: {system_id} has no writable resource")
        })?;
    writable_resource(store, project_id, system_id, &file.resource_id)
}

fn read_numeric_field(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    resource: &DomainResourceRecord,
    field: &str,
) -> Result<f64, String> {
    read_scalar_field(store, project_id, draft_id, resource, field)?
        .parse::<f64>()
        .map_err(|_| format!("DOMAIN_COMPILER_NUMERIC_FIELD_INVALID: {field}"))
}

fn read_scalar_field(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    resource: &DomainResourceRecord,
    field: &str,
) -> Result<String, String> {
    let file = resource
        .files
        .first()
        .ok_or_else(|| "DOMAIN_COMPILER_RESOURCE_FILE_REQUIRED: numeric field".to_string())?;
    if !file
        .extension
        .as_deref()
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "txt" | "lua"))
    {
        return Err("DOMAIN_COMPILER_NUMERIC_TYPE_UNSUPPORTED: expected TXT or Lua".to_string());
    }
    let opened = store.safe_text_open(project_id, &file.path, Some(draft_id))?;
    let aliases = manifest_field_aliases(store, project_id, draft_id, field)?;
    let (_, _, value) =
        field_line_replacement_with_aliases(&opened.content, field, &aliases, &Value::Null)?;
    Ok(value)
}

fn generated_key_value(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    resource: &DomainResourceRecord,
    field: &str,
    target_id: &str,
    ordinal: u64,
) -> Result<Value, String> {
    let current = read_scalar_field(store, project_id, draft_id, resource, field)?;
    if current.parse::<f64>().is_ok() {
        Ok(ordinal_value(ordinal))
    } else {
        Ok(Value::String(target_id.to_string()))
    }
}

fn interpolated_generated_patch(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    source: &DomainResourceRecord,
    patch: &serde_json::Map<String, Value>,
    interpolation: &str,
    ratio: f64,
) -> Result<serde_json::Map<String, Value>, String> {
    let mut generated = patch.clone();
    for (field, target) in patch.iter().filter(|(_, value)| value.is_number()) {
        let from = read_numeric_field(store, project_id, draft_id, source, field)?;
        let to = target
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("DOMAIN_COMPILER_NUMBER_INVALID: {field}"))?;
        let value = match interpolation {
            "linear" => from + (to - from) * ratio,
            "geometric" if from > 0.0 && to > 0.0 => from * (to / from).powf(ratio),
            "geometric" => from + (to - from) * ratio,
            "step" if ratio < 1.0 => from,
            "step" => to,
            mode => return Err(format!("DOMAIN_COMPILER_INTERPOLATION_INVALID: {mode}")),
        };
        let value = if target.as_i64().is_some() || target.as_u64().is_some() {
            value.round()
        } else {
            value
        };
        generated.insert(field.clone(), number_value(value)?);
    }
    Ok(generated)
}

fn sibling_resource_path(source: &str, target_id: &str) -> Result<String, String> {
    let target_id = safe_path_id(target_id);
    let source_path = std::path::Path::new(source);
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "DOMAIN_COMPILER_SOURCE_EXTENSION_REQUIRED: clone".to_string())?;
    let file_name = format!("{target_id}.{extension}");
    let target = source_path
        .parent()
        .map(|parent| parent.join(&file_name))
        .unwrap_or_else(|| PathBuf::from(&file_name));
    Ok(target.to_string_lossy().replace('\\', "/"))
}

fn safe_path_id(value: &str) -> String {
    let portable = !value.is_empty()
        && value.len() <= 96
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if portable {
        return value.to_string();
    }
    let hash = value
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    format!("resource-{hash:016x}")
}

fn portable_scalar_id(value: &Value) -> Result<String, String> {
    let value = match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => {
            return Err(
                "DOMAIN_COMPILER_PRIMARY_KEY_INVALID: expected string or number".to_string(),
            )
        }
    };
    Ok(value)
}

fn required_object(params: &Value, key: &str) -> Result<serde_json::Map<String, Value>, String> {
    params
        .get(key)
        .and_then(Value::as_object)
        .cloned()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("MCP_ARGUMENT_INVALID: {key} must be a non-empty object"))
}

fn required_object_or_empty(
    params: &Value,
    key: &str,
) -> Result<serde_json::Map<String, Value>, String> {
    match params.get(key) {
        Some(value) => value
            .as_object()
            .cloned()
            .ok_or_else(|| format!("MCP_ARGUMENT_INVALID: {key} must be an object")),
        None => Ok(serde_json::Map::new()),
    }
}

fn required_string_array(params: &Value, key: &str) -> Result<Vec<String>, String> {
    let values = params
        .get(key)
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty())
        .ok_or_else(|| format!("MCP_ARGUMENT_INVALID: {key} must be a non-empty array"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("MCP_ARGUMENT_INVALID: {key} entries must be strings"))
        })
        .collect()
}

fn required_u64(params: &Value, key: &str) -> Result<u64, String> {
    params
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("MCP_ARGUMENT_INVALID: {key} is required"))
}

fn required_i64(params: &Value, key: &str) -> Result<i64, String> {
    params
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("MCP_ARGUMENT_INVALID: {key} is required"))
}

fn required_f64(params: &Value, key: &str) -> Result<f64, String> {
    params
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("MCP_ARGUMENT_INVALID: {key} is required"))
}

fn rounded_number(value: f64, rounding: Option<&str>) -> Result<Value, String> {
    let value = match rounding.unwrap_or("nearest") {
        "nearest" => value.round(),
        "floor" => value.floor(),
        "ceil" => value.ceil(),
        mode => return Err(format!("DOMAIN_COMPILER_ROUNDING_INVALID: {mode}")),
    };
    number_value(value)
}

fn number_value(value: f64) -> Result<Value, String> {
    if value.fract() == 0.0 && value >= i64::MIN as f64 && value <= i64::MAX as f64 {
        return Ok(Value::from(value as i64));
    }
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .ok_or_else(|| "DOMAIN_COMPILER_NUMBER_INVALID: non-finite result".to_string())
}

fn ordinal_value(value: u64) -> Value {
    Value::Number(value.into())
}

fn primitive_revision(result: &Value) -> Result<i64, String> {
    result
        .get("revision")
        .and_then(Value::as_i64)
        .or_else(|| {
            result
                .pointer("/preview/draft/revision")
                .and_then(Value::as_i64)
        })
        .ok_or_else(|| "DOMAIN_COMPILER_REVISION_MISSING: primitive result".to_string())
}

fn compiled_result(operation_id: &str, revision: i64, results: Vec<Value>) -> Value {
    json!({
        "operation": operation_id,
        "revision": revision,
        "results": results
    })
}

fn manifest_field_aliases(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    field: &str,
) -> Result<Vec<String>, String> {
    let manifest = store.draft_domain_manifest(project_id, draft_id)?;
    manifest
        .resources
        .field_mappings
        .into_iter()
        .find(|mapping| mapping.field == field)
        .map(|mapping| mapping.aliases)
        .filter(|aliases| !aliases.is_empty())
        .ok_or_else(|| {
            format!(
                "DOMAIN_FIELD_MAPPING_MISSING: {}:{field}",
                manifest.system_id
            )
        })
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
        let aliases = manifest_field_aliases(store, project_id, draft_id, field)?;
        let (old, new, old_value) =
            field_line_replacement_with_aliases(&opened.content, field, &aliases, value)?;
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

fn field_line_replacement_with_aliases(
    content: &str,
    field: &str,
    aliases: &[String],
    value: &Value,
) -> Result<(String, String, String), String> {
    let rendered = scalar_text(value)?;
    let mut matches = Vec::new();
    let normalized_aliases = aliases
        .iter()
        .map(|alias| canonical_xls_field(alias))
        .collect::<std::collections::BTreeSet<_>>();
    for line in content.lines() {
        let leading = line.len().saturating_sub(line.trim_start().len());
        let trimmed = line.trim_start();
        for separator in ['=', ':', '\t'] {
            let Some((source_field, old_value)) = trimmed.split_once(separator) else {
                continue;
            };
            if source_field.starts_with('"')
                || source_field.starts_with('\'')
                || !normalized_aliases.contains(&canonical_xls_field(source_field.trim()))
            {
                continue;
            }
            let marker = format!("{source_field}{separator}");
            matches.push((
                line.to_string(),
                format!("{}{}{}", &line[..leading], marker, rendered),
                old_value.trim().trim_matches(['\"', '\'']).to_string(),
            ));
            break;
        }
        if let Some(rest) = trimmed.strip_prefix('"') {
            if let Some((source_field, rest)) = rest.split_once('"') {
                if normalized_aliases.contains(&canonical_xls_field(source_field)) {
                    if let Some(rest) = rest.trim_start().strip_prefix(':') {
                        let comma = rest.trim_end().ends_with(',');
                        let old_value = rest.trim().trim_end_matches(',').trim();
                        let json_marker = format!("\"{source_field}\"");
                        matches.push((
                            line.to_string(),
                            format!(
                                "{}{}: {}{}",
                                &line[..leading],
                                json_marker,
                                serde_json::to_string(value).map_err(|error| format!(
                                    "DOMAIN_FIELD_VALUE_INVALID: {error}"
                                ))?,
                                if comma { "," } else { "" }
                            ),
                            old_value.trim_matches(['\"', '\'']).to_string(),
                        ));
                    }
                }
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
    draft_id: &str,
    resource: &DomainResourceRecord,
    changes: &serde_json::Map<String, Value>,
    expected_reference: Option<&(String, String)>,
) -> Result<Value, String> {
    let path = resource
        .files
        .first()
        .map(|file| file.path.as_str())
        .ok_or_else(|| "DOMAIN_COMPILER_RESOURCE_FILE_REQUIRED: XLS changes".to_string())?;
    let workbook = store.safe_xls_open(project_id, path)?;
    if let (Some(sheet), Some(row)) = (&resource.source.sheet, resource.source.row) {
        return compile_xls_record_changes(
            store,
            project_id,
            draft_id,
            XlsRecordTarget {
                path,
                workbook_sha256: &workbook.sha256,
                sheet,
                row,
            },
            changes,
            expected_reference,
        );
    }
    let mut updates = Vec::with_capacity(changes.len());
    for (field, value) in changes {
        let aliases = manifest_field_aliases(store, project_id, draft_id, field)?;
        let mut matches = Vec::new();
        for sheet in &workbook.sheets {
            let data =
                store.safe_xls_sheet_read(project_id, path, &sheet.name, &workbook.sha256)?;
            if data.rows.len() != 2 {
                continue;
            }
            for (column, header) in data.rows[0].iter().enumerate() {
                if aliases
                    .iter()
                    .any(|alias| canonical_xls_field(header) == canonical_xls_field(alias))
                {
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

struct XlsRecordTarget<'a> {
    path: &'a str,
    workbook_sha256: &'a str,
    sheet: &'a str,
    row: usize,
}

fn compile_xls_record_changes(
    store: &DomainStore,
    project_id: &str,
    draft_id: &str,
    target: XlsRecordTarget<'_>,
    changes: &serde_json::Map<String, Value>,
    expected_reference: Option<&(String, String)>,
) -> Result<Value, String> {
    let data = store.safe_xls_sheet_read(
        project_id,
        target.path,
        target.sheet,
        target.workbook_sha256,
    )?;
    let physical_row = target
        .row
        .checked_sub(1)
        .ok_or_else(|| format!("DOMAIN_XLS_ROW_INVALID: {}:{}", target.sheet, target.row))?;
    let headers = data
        .rows
        .first()
        .ok_or_else(|| format!("DOMAIN_XLS_HEADERS_MISSING: {}", target.sheet))?;
    let source = data
        .rows
        .get(physical_row)
        .ok_or_else(|| format!("DOMAIN_XLS_ROW_MISSING: {}:{}", target.sheet, target.row))?;
    let mut updates = Vec::with_capacity(changes.len());
    for (field, value) in changes {
        let aliases = manifest_field_aliases(store, project_id, draft_id, field)?;
        let matches = headers
            .iter()
            .enumerate()
            .filter(|(_, header)| {
                aliases
                    .iter()
                    .any(|alias| canonical_xls_field(header) == canonical_xls_field(alias))
            })
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!(
                "DOMAIN_XLS_FIELD_AMBIGUOUS: {field} matched {} columns in {}",
                matches.len(),
                target.sheet
            ));
        }
        let column = matches[0].0;
        let old_value = source.get(column).cloned().unwrap_or_default();
        if expected_reference.is_some_and(|(expected_field, expected)| {
            canonical_xls_field(expected_field) == canonical_xls_field(field)
                && expected != &old_value
        }) {
            return Err(format!("DOMAIN_REFERENCE_SOURCE_MISMATCH: {field}"));
        }
        updates.push(json!({
            "sheet":target.sheet,
            "row":physical_row,
            "column":column,
            "expectedValue":old_value,
            "value":value
        }));
    }
    Ok(json!({
        "type":"xls.update_cells",
        "path":target.path,
        "expectedSha256":target.workbook_sha256,
        "updates":updates
    }))
}

fn canonical_xls_field(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
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

fn apply_exact_replacements(content: &str, replacements: &Value) -> Result<String, String> {
    let replacements = replacements
        .as_array()
        .filter(|items| items.len() <= 10_000)
        .ok_or_else(|| {
            "MCP_ARGUMENT_INVALID: replacements must contain 0..10000 items".to_string()
        })?;
    let mut output = content.to_string();
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
        if old.is_empty() || expected_count == 0 || actual != expected_count {
            return Err(format!(
                "SAFE_TEXT_ANCHOR_COUNT_CONFLICT: replacements[{index}] expected {expected_count}, got {actual}"
            ));
        }
        output = output.replace(&old, new);
    }
    Ok(output)
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
        let content = match operation.get("replacements") {
            Some(replacements) => apply_exact_replacements(&source.content, replacements)?,
            None => source.content,
        };
        let preview = store.patch_draft(
            project_id,
            draft_id,
            revision,
            &[DraftChangeInput {
                path: target_path,
                content: Some(content),
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
            let replacements = operation.get("replacements").ok_or_else(|| {
                "MCP_ARGUMENT_INVALID: replacements must contain 1..10000 items".to_string()
            })?;
            if replacements.as_array().is_none_or(Vec::is_empty) {
                return Err(
                    "MCP_ARGUMENT_INVALID: replacements must contain 1..10000 items".to_string(),
                );
            }
            apply_exact_replacements(&opened.content, replacements)?
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
    let compact_size = match serde_json::to_vec(&value) {
        Ok(value) => value.len(),
        Err(error) => return tool_failure(&format!("MCP_RESULT_SERIALIZE_FAILED: {error}")),
    };
    if compact_size > MCP_MAX_RESULT_BYTES {
        return tool_failure(&format!(
            "MCP_RESULT_BUDGET_EXCEEDED: {compact_size} bytes exceeds {MCP_MAX_RESULT_BYTES}; narrow the system, query, or page"
        ));
    }
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
    use easyexcel_xls::biff8::{Biff8Book, Biff8Cell, Biff8Sheet, Biff8Value};
    use std::collections::BTreeMap;
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
        assert!(!schemas.contains("\"action\":"));
        assert!(!schemas.contains("\"field\""));
        let project_read_tools = [
            "mir3_system_list",
            "mir3_system_describe",
            "mir3_resource_query",
            "mir3_resource_get",
            "mir3_dependency_resolve",
            "mir3_gui_context",
            "mir3_gui_operate",
            "mir3_gui_asset_query",
            "mir3_gui_validate",
        ];
        for definition in &definitions {
            let name = definition["name"].as_str().unwrap();
            let requires_scope = definition
                .pointer("/inputSchema/required")
                .and_then(Value::as_array)
                .is_some_and(|required| required.iter().any(|value| value == "scopeToken"));
            assert_eq!(requires_scope, !project_read_tools.contains(&name));
            if name.starts_with("mir3_gui_") {
                assert!(definition
                    .pointer("/inputSchema/required")
                    .and_then(Value::as_array)
                    .is_some_and(|required| required
                        .iter()
                        .any(|value| value == "workspaceToken")));
            }
        }
        let schema_bytes = serde_json::to_vec(&definitions).unwrap().len();
        assert!(
            schema_bytes <= MCP_MAX_SCHEMA_BYTES,
            "MCP schemas use {schema_bytes} bytes, budget is {MCP_MAX_SCHEMA_BYTES}"
        );
        assert_eq!(
            definitions
                .iter()
                .find(|definition| definition["name"] == "mir3_resource_query")
                .and_then(|definition| {
                    definition.pointer("/inputSchema/properties/limit/maximum")
                })
                .and_then(Value::as_u64),
            Some(MCP_MAX_QUERY_ITEMS as u64)
        );
        let draft_diff = definitions
            .iter()
            .find(|definition| definition["name"] == "mir3_draft_diff")
            .unwrap();
        assert_eq!(
            draft_diff.pointer("/inputSchema/additionalProperties"),
            Some(&json!(false))
        );
        assert_eq!(
            draft_diff.pointer("/inputSchema/required"),
            Some(&json!(["draftId", "scopeToken"]))
        );
    }

    #[test]
    fn mcp_context_payloads_have_quantified_fail_closed_budgets() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-budget-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let store = DomainStore::new(&base).unwrap();
        let systems = store.list_domain_systems().unwrap();
        let full_registry_bytes = serde_json::to_vec(&systems).unwrap().len();
        let summary = system_list_payload(systems);
        let summary_bytes = serde_json::to_vec(&summary).unwrap().len();
        assert_eq!(summary["systems"].as_array().unwrap().len(), 33);
        assert!(full_registry_bytes > MCP_MAX_RESULT_BYTES);
        assert!(summary_bytes < 32 * 1024);
        assert_eq!(tool_success(summary)["isError"], false);

        let oversized = json!({"content": "x".repeat(MCP_MAX_RESULT_BYTES + 1)});
        let rejected = tool_success(oversized);
        assert_eq!(rejected["isError"], true);
        assert!(rejected["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("MCP_RESULT_BUDGET_EXCEEDED:"));
        drop(store);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn map_edit_schema_closes_each_structured_variant() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-map-schema-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let store = DomainStore::new(base.join("data")).unwrap();
        let schema = store
            .list_domain_systems()
            .unwrap()
            .into_iter()
            .find(|manifest| manifest.system_id == "map")
            .unwrap()
            .capabilities
            .into_iter()
            .find(|capability| capability.id == "edit-map-region")
            .unwrap()
            .parameter_schema;
        for operation in [
            json!({"type":"setSprite","x":1,"y":2,"layer":"front","library":3,"image":4}),
            json!({"type":"clearSprite","x":1,"y":2,"layer":"middle"}),
            json!({"type":"setCollision","x":1,"y":2,"walkable":true,"frontBlocked":false}),
            json!({"type":"setDoor","x":1,"y":2,"doorIndex":3,"doorOffset":4}),
            json!({"type":"setAnimation","x":1,"y":2,"middleFrames":3,"frontFrames":4}),
        ] {
            validate_json_schema(
                &schema,
                &json!({
                    "operation":"edit-map-region",
                    "resourceId":"map:fixture",
                    "operations":[operation],
                    "expectedRevision":0
                }),
                "params",
            )
            .unwrap();
        }
        assert!(validate_json_schema(
            &schema,
            &json!({
                "operation":"edit-map-region",
                "resourceId":"map:fixture",
                "operations":[{"type":"setCollision","x":1,"y":2,"walkable":true}],
                "expectedRevision":0
            }),
            "params",
        )
        .unwrap_err()
        .starts_with("CAPABILITY_PARAMETER_ONE_OF_INVALID:"));
        fs::remove_dir_all(base).ok();
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
    fn xls_record_resource_compiles_changes_for_its_exact_source_row() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-xls-record-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let root = base.join("项目/记录级修改");
        let path = root.join("引擎/Mir200/Envir/Shop/cfg_store.xls");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        let mut sheet = Biff8Sheet::new("商品");
        for (row_index, row) in [
            ["offerId", "itemId", "currencyItemId", "price"],
            ["OFFER_A", "ITEM_A", "ITEM_A", "10"],
            ["OFFER_B", "ITEM_B", "ITEM_B", "20"],
        ]
        .iter()
        .enumerate()
        {
            for (column_index, value) in row.iter().enumerate() {
                sheet
                    .set(
                        row_index as u32,
                        column_index,
                        Biff8Cell::general(Biff8Value::Text((*value).to_string())),
                    )
                    .unwrap();
            }
        }
        let mut book = Biff8Book::default();
        book.sheets.push(sheet);
        fs::write(&path, book.to_cfb_bytes().unwrap()).unwrap();
        let store = DomainStore::new(base.join("data")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let record = store
            .query_domain_resources(
                &project.id,
                "shop",
                &DomainResourceQuery {
                    text: "OFFER_B".to_string(),
                    resource_type: None,
                    limit: Some(10),
                    offset: None,
                },
            )
            .unwrap()
            .into_iter()
            .find(|resource| resource.label == "OFFER_B")
            .unwrap();
        assert_eq!(record.source.row, Some(3));
        let draft = store.open_draft(&project.id, "xls alias edit").unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "shop", "1.3.1", None)
            .unwrap();
        let primitive = compile_xls_field_changes(
            &store,
            &project.id,
            &draft.id,
            &record,
            &serde_json::Map::from_iter([("price".to_string(), json!(25))]),
            None,
        )
        .unwrap();
        assert_eq!(primitive["updates"][0]["sheet"], "商品");
        assert_eq!(primitive["updates"][0]["row"], 2);
        assert_eq!(primitive["updates"][0]["column"], 3);
        assert_eq!(primitive["updates"][0]["expectedValue"], "20");
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn shaped_compiler_routes_every_registered_shaped_operation() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-shaped-routes-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let store = DomainStore::new(base.join("data")).unwrap();
        let operations = store
            .list_domain_systems()
            .unwrap()
            .into_iter()
            .flat_map(|manifest| {
                manifest
                    .operations
                    .into_iter()
                    .filter(|operation| is_shaped_operation(&operation.id))
                    .map(move |operation| format!("{}:{}", manifest.system_id, operation.id))
            })
            .collect::<Vec<_>>();
        assert_eq!(operations.len(), 78, "{operations:#?}");
        assert!(operations
            .iter()
            .any(|value| value == "map:edit-map-region"));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn every_writable_official_operation_compiles_into_a_scoped_draft() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-shaped-e2e-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let root = base.join("项目/参数编译矩阵");
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Envir/domains")).unwrap();
        fs::write(root.join("引擎/mir_version.txt"), "1.2.0\n").unwrap();
        let pack_root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/mir3-domain-packs");
        let bootstrap = DomainStore::new(base.join("bootstrap")).unwrap();
        let manifests = bootstrap.list_domain_systems().unwrap();
        let fixture_records = normalized_fixture_records(&pack_root, &manifests);
        for manifest in &manifests {
            let records = &fixture_records[&manifest.system_id];
            let owned_selector = manifest.file_projection.owned_selectors.first().unwrap();
            for projection_root in ["客户端/dev/domains", "引擎/Mir200/Envir/domains"] {
                let directory = root.join(projection_root).join(&manifest.system_id);
                fs::create_dir_all(&directory).unwrap();
                for (index, record) in records.iter().take(2).enumerate() {
                    fs::write(
                        directory.join(format!("{owned_selector}_source_{}.txt", index + 1)),
                        record_text(record),
                    )
                    .unwrap();
                }
            }
            if manifest.capabilities.iter().any(|capability| {
                !capability.write_systems.is_empty()
                    && !is_shaped_operation(&capability.id)
                    && capability
                        .steps
                        .first()
                        .is_some_and(|step| step.primitive == "xls")
            }) {
                let xls_records = xls_fixture_records(records, manifest);
                let headers = xls_records[0]
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>();
                let mut sheet = Biff8Sheet::new("records");
                for (column, header) in headers.iter().enumerate() {
                    sheet
                        .set(
                            0,
                            column,
                            Biff8Cell::general(Biff8Value::Text(header.clone())),
                        )
                        .unwrap();
                }
                for (row_index, record) in xls_records.iter().take(2).enumerate() {
                    for (column, header) in headers.iter().enumerate() {
                        let value = record[header]
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| record[header].to_string());
                        sheet
                            .set(
                                row_index as u32 + 1,
                                column,
                                Biff8Cell::general(Biff8Value::Text(value)),
                            )
                            .unwrap();
                    }
                }
                let mut book = Biff8Book::default();
                book.sheets.push(sheet);
                let bytes = book.to_cfb_bytes().unwrap();
                for projection_root in ["客户端/dev/domains", "引擎/Mir200/Envir/domains"] {
                    fs::write(
                        root.join(projection_root)
                            .join(&manifest.system_id)
                            .join(format!("{owned_selector}_records.xls")),
                        &bytes,
                    )
                    .unwrap();
                }
            }
        }
        let map_directory = root.join("引擎/Mir200/map");
        fs::create_dir_all(&map_directory).unwrap();
        let mut map_bytes = vec![0_u8; 28 + 4 * 3 + 16 * 14];
        map_bytes[22..24].copy_from_slice(&4_u16.to_le_bytes());
        map_bytes[24..26].copy_from_slice(&4_u16.to_le_bytes());
        fs::write(map_directory.join("region.map"), map_bytes).unwrap();

        let store = DomainStore::new(base.join("data")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let manifests = store.list_domain_systems().unwrap();
        let mut coverage = BTreeMap::<String, CapabilityLifecycleCoverage>::new();
        for manifest in manifests {
            let records = &fixture_records[&manifest.system_id];
            let files = store
                .query_domain_files(
                    &project.id,
                    &manifest.system_id,
                    &DomainFileQuery {
                        text: format!("domains/{}/", manifest.system_id),
                        limit: Some(100),
                        offset: None,
                    },
                )
                .unwrap();
            let text_source_ids = files
                .iter()
                .filter(|file| file.path.ends_with("_source_1.txt") && file.access != "readonly")
                .map(|file| file.resource_id.clone())
                .collect::<Vec<_>>();
            assert_eq!(
                text_source_ids.len(),
                2,
                "{}: {files:#?}",
                manifest.system_id
            );
            let xls_source_ids = store
                .query_domain_resources(
                    &project.id,
                    &manifest.system_id,
                    &DomainResourceQuery {
                        text: format!("domains/{}/", manifest.system_id),
                        resource_type: None,
                        limit: Some(100),
                        offset: None,
                    },
                )
                .unwrap()
                .into_iter()
                .filter(|resource| {
                    resource.source.path.ends_with(".xls")
                        && resource.source.row == Some(2)
                        && resource.writable
                })
                .map(|resource| resource.id)
                .collect::<Vec<_>>();

            for capability in manifest
                .capabilities
                .iter()
                .filter(|capability| !capability.write_systems.is_empty())
            {
                let text_only_structural = is_shaped_operation(&capability.id);
                let source_ids = if !text_only_structural
                    && capability
                        .steps
                        .first()
                        .is_some_and(|step| step.primitive == "xls")
                {
                    assert_eq!(
                        xls_source_ids.len(),
                        2,
                        "{}:{} must expose one mirrored XLS record per projection",
                        manifest.system_id,
                        capability.id
                    );
                    &xls_source_ids
                } else {
                    &text_source_ids
                };
                let params = shaped_test_params(
                    &capability.id,
                    &capability.parameter_schema,
                    source_ids,
                    records,
                    &store,
                    &project.id,
                    &manifest.system_id,
                );
                let missing_key = capability.parameter_schema["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(Value::as_str)
                    .find(|key| *key != "operation")
                    .unwrap();
                let mut missing = params.clone();
                missing.as_object_mut().unwrap().remove(missing_key);
                assert!(
                    validate_json_schema(&capability.parameter_schema, &missing, "params")
                        .unwrap_err()
                        .starts_with("CAPABILITY_PARAMETER_REQUIRED:"),
                    "{} accepted missing {missing_key}",
                    capability.id
                );
                validate_json_schema(&capability.parameter_schema, &params, "params").unwrap();
                let draft = store
                    .open_draft(&project.id, &format!("compile {}", capability.id))
                    .unwrap();
                store
                    .bind_draft_domain(
                        &project.id,
                        &draft.id,
                        &manifest.system_id,
                        &manifest.version,
                        None,
                    )
                    .unwrap();
                let result = execute_manifest_operation(
                    &store,
                    &project.id,
                    &draft.id,
                    &capability.id,
                    &manifest.system_id,
                    &capability.steps,
                    &params,
                )
                .unwrap_or_else(|error| panic!("{} failed: {error}", capability.id));
                assert!(result["revision"]
                    .as_i64()
                    .is_some_and(|revision| revision > 0));
                assert_eq!(result["draftId"], draft.id, "{}", capability.id);
                assert_eq!(result["systemId"], manifest.system_id, "{}", capability.id);
                assert!(
                    result["changedFiles"]
                        .as_array()
                        .is_some_and(|files| !files.is_empty()),
                    "{}",
                    capability.id
                );
                assert!(result["changedResources"].is_array(), "{}", capability.id);
                let preview = store.preview_draft(&project.id, &draft.id).unwrap();
                assert_structured_draft_diff(
                    &manifest.system_id,
                    &capability.id,
                    &result,
                    &preview,
                );
                let original = capture_preview_bytes(&root, &preview);
                let validation = result
                    .get("validation")
                    .and_then(Value::as_object)
                    .unwrap_or_else(|| {
                        panic!(
                            "{}:{} did not return a structured validation report: {result:#}",
                            manifest.system_id, capability.id
                        )
                    });
                let lifecycle = coverage.entry(manifest.system_id.clone()).or_default();
                lifecycle.compiled.push(capability.id.clone());
                if validation.get("valid") == Some(&Value::Bool(true)) {
                    let snapshot = store
                        .apply_validated_domain_draft(
                            &project.id,
                            &draft.id,
                            preview.draft.revision,
                            &preview.diff_hash,
                        )
                        .unwrap_or_else(|error| {
                            panic!(
                                "{}:{} passed validation but Apply failed: {error}",
                                manifest.system_id, capability.id
                            )
                        });
                    assert_project_bytes_changed(
                        &root,
                        &original,
                        &manifest.system_id,
                        &capability.id,
                    );
                    store.restore_snapshot(&project.id, &snapshot.id).unwrap();
                    assert_project_bytes_restored(
                        &root,
                        &original,
                        &manifest.system_id,
                        &capability.id,
                    );
                    lifecycle.applied.push(capability.id.clone());
                } else {
                    assert!(
                        validation
                            .get("diagnostics")
                            .and_then(Value::as_array)
                            .is_some_and(|diagnostics| !diagnostics.is_empty()),
                        "{}:{} invalid fixture has no diagnostic classification: {result:#}",
                        manifest.system_id,
                        capability.id
                    );
                    let error = store
                        .apply_validated_domain_draft(
                            &project.id,
                            &draft.id,
                            preview.draft.revision,
                            &preview.diff_hash,
                        )
                        .unwrap_err();
                    assert!(
                        error.starts_with("DRAFT_VALIDATION_FAILED:"),
                        "{}:{} was not rejected by the validation gate: {error}",
                        manifest.system_id,
                        capability.id
                    );
                    assert_project_bytes_restored(
                        &root,
                        &original,
                        &manifest.system_id,
                        &capability.id,
                    );
                    lifecycle.rejected.push(format!(
                        "{}:{}",
                        capability.id,
                        validation
                            .get("diagnostics")
                            .and_then(Value::as_array)
                            .map(|diagnostics| diagnostics
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join("|"))
                            .unwrap_or_default()
                    ));
                }
            }
        }
        assert_capability_lifecycle_coverage(&coverage);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn project_status_and_domain_query_use_the_registered_project() {
        let base = std::env::temp_dir().join(format!("mir3-mcp-{}", std::process::id()));
        let root = base.join("项目/木立传奇");
        fs::create_dir_all(root.join("客户端/dev/Lua")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Envir/QuestDiary")).unwrap();
        fs::write(root.join("客户端/dev/Lua/main.lua"), "return 'MIR3'\n").unwrap();
        fs::write(
            root.join("引擎/Mir200/Envir/QuestDiary/quest.txt"),
            "questId=Q1\nquestType=main\nrequiredLevel=1\nrewardItemId=I1\nnextQuestId=Q2\n",
        )
        .unwrap();
        let store = DomainStore::new(base.join("data")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();

        let workbench_systems = call_tool(&store, &project.id, "mir3_system_list", json!({}));
        assert_eq!(
            workbench_systems
                .pointer("/structuredContent/systems")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(33)
        );
        let workbench_query = call_tool(
            &store,
            &project.id,
            "mir3_resource_query",
            json!({"systemId":"quest","text":"QuestDiary","limit":10}),
        );
        assert_eq!(workbench_query.get("isError"), Some(&Value::Bool(false)));
        let unscoped_write = call_tool(
            &store,
            &project.id,
            "mir3_draft_open",
            json!({"systemId":"quest","intent":"must fail"}),
        );
        assert!(tool_error(&unscoped_write).contains("scopeToken"));

        let lease = store
            .issue_task_scope(
                &project.id,
                "task-mcp-test",
                &["quest".to_string()],
                &["quest".to_string()],
                &[],
                json!({"quest":"1.3.1"}),
                mir3_domain::now_millis() + 60_000,
            )
            .unwrap();
        let systems = call_tool(
            &store,
            &project.id,
            "mir3_system_list",
            json!({"scopeToken":lease.token.clone()}),
        );
        let listed_systems = systems
            .pointer("/structuredContent/systems")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(listed_systems.len(), 1);
        assert_eq!(listed_systems[0]["systemId"], "quest");
        let capabilities = call_tool(
            &store,
            &project.id,
            "mir3_capability_list",
            json!({"scopeToken":lease.token.clone()}),
        );
        assert!(capabilities
            .pointer("/structuredContent/capabilities")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .all(|capability| capability["systemId"] == "quest"));
        let foreign_capability = call_tool(
            &store,
            &project.id,
            "mir3_capability_describe",
            json!({"scopeToken":lease.token.clone(),"capabilityId":"inspect-shop"}),
        );
        assert!(tool_error(&foreign_capability).starts_with("TASK_SCOPE_READ_DENIED:"));
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
                "scopeToken":lease.token.clone(),
                "systemId":"quest",
                "capabilityId":"inspect-quest",
                "params":{"operation":"inspect-quest"}
            }),
        );
        assert_eq!(inspect.get("isError"), Some(&Value::Bool(false)));
        let resource = store
            .query_domain_resources(
                &project.id,
                "quest",
                &DomainResourceQuery {
                    text: "Q1".to_string(),
                    resource_type: None,
                    limit: Some(10),
                    offset: None,
                },
            )
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let projected = call_tool(
            &store,
            &project.id,
            "mir3_capability_invoke",
            json!({
                "scopeToken":lease.token,
                "systemId":"quest",
                "capabilityId":"inspect-quest",
                "params":{
                    "operation":"inspect-quest",
                    "resourceId":resource.id,
                    "includeDependencies":false,
                    "projection":"engine"
                }
            }),
        );
        assert_eq!(projected["isError"], false);
        assert!(projected
            .pointer("/structuredContent/dependencies")
            .unwrap()
            .is_null());
        assert_eq!(projected["structuredContent"]["projection"], "engine");
        assert!(
            projected["structuredContent"]["resources"][0]["dependencies"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            validate_target_engine(&store, "quest", "1.2").unwrap()["compatible"]
                .as_bool()
                .unwrap()
        );
        assert!(validate_target_engine(&store, "quest", "0.1")
            .unwrap_err()
            .starts_with("DOMAIN_TARGET_ENGINE_INCOMPATIBLE:"));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn manifest_field_aliases_compile_canonical_changes_to_real_text_keys() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-field-alias-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let root = base.join("项目/字段别名");
        let relative = "引擎/Mir200/Envir/level.txt";
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Envir")).unwrap();
        fs::write(root.join("引擎/mir_version.txt"), "1.2.0\n").unwrap();
        fs::write(
            root.join(relative),
            "Level=1\nRequired_Experience=100\nStat_Points=1\nRecommended_Monster_Id=M1\n",
        )
        .unwrap();
        let store = test_store(&base);
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let draft = store.open_draft(&project.id, "alias edit").unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "level", "1.3.1", None)
            .unwrap();
        let primitive = compile_text_field_changes(
            &store,
            &project.id,
            &draft.id,
            relative,
            &serde_json::Map::from_iter([("requiredExperience".to_string(), json!(250))]),
            None,
        )
        .unwrap();
        let result = apply_safe_operation(&store, &project.id, &draft.id, 0, &primitive).unwrap();
        assert_eq!(primitive_revision(&result).unwrap(), 1);
        let opened = store
            .safe_text_open(&project.id, relative, Some(&draft.id))
            .unwrap();
        assert!(opened.content.contains("Required_Experience=250"));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn draft_diff_is_revisioned_and_rejects_foreign_scope_or_project() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-draft-diff-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let root = base.join("项目/DraftDiff");
        let quest_path = "引擎/Mir200/Envir/QuestDiary/quest.txt";
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Envir/QuestDiary")).unwrap();
        fs::write(root.join("引擎/mir_version.txt"), "1.2.0\n").unwrap();
        fs::write(root.join(quest_path), "quest=0\n").unwrap();
        let store = DomainStore::new(base.join("data")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let draft = store.open_draft(&project.id, "检查 Draft Diff").unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "quest", "1.3.1", None)
            .unwrap();
        let preview = store
            .patch_draft(
                &project.id,
                &draft.id,
                0,
                &[DraftChangeInput {
                    path: quest_path.to_string(),
                    content: Some("quest=1\n".to_string()),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap();
        let lease = store
            .issue_task_scope(
                &project.id,
                "task-draft-diff",
                &["quest".to_string()],
                &["quest".to_string()],
                std::slice::from_ref(&draft.id),
                json!({"quest":"1.3.1"}),
                mir3_domain::now_millis() + 60_000,
            )
            .unwrap();
        let result = call_tool(
            &store,
            &project.id,
            "mir3_draft_diff",
            json!({"scopeToken":lease.token.clone(),"draftId":draft.id}),
        );
        assert_eq!(result["isError"], false);
        assert_eq!(
            result.pointer("/structuredContent/revision"),
            Some(&json!(preview.draft.revision))
        );
        assert_eq!(
            result.pointer("/structuredContent/diffHash"),
            Some(&json!(preview.diff_hash))
        );
        assert!(result
            .pointer("/structuredContent/preview/changes/0/unifiedDiff")
            .and_then(Value::as_str)
            .is_some_and(|diff| diff.contains("quest=1")));

        let unrelated = store.open_draft(&project.id, "另一个 Draft").unwrap();
        store
            .bind_draft_domain(&project.id, &unrelated.id, "quest", "1.3.1", None)
            .unwrap();
        let denied = call_tool(
            &store,
            &project.id,
            "mir3_draft_diff",
            json!({"scopeToken":lease.token.clone(),"draftId":unrelated.id}),
        );
        assert_eq!(denied["isError"], true);
        assert!(denied["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("TASK_SCOPE_DRAFT_DENIED:"));

        let other_root = base.join("项目/OtherProject");
        fs::create_dir_all(other_root.join("客户端/dev")).unwrap();
        fs::create_dir_all(other_root.join("引擎/Mir200/Envir")).unwrap();
        let other_project = store.import_project(&other_root).unwrap();
        let foreign_project = call_tool(
            &store,
            &other_project.id,
            "mir3_draft_diff",
            json!({"scopeToken":lease.token,"draftId":draft.id}),
        );
        assert_eq!(foreign_project["isError"], true);
        assert!(foreign_project["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("TASK_SCOPE_NOT_FOUND:"));

        let unclosed = call_tool(
            &store,
            &project.id,
            "mir3_draft_diff",
            json!({"scopeToken":"x".repeat(32),"draftId":"draft","unexpected":true}),
        );
        assert_eq!(unclosed["isError"], true);
        assert!(unclosed["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("CAPABILITY_PARAMETER_UNKNOWN:"));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn validate_tool_reports_the_draft_overlay_instead_of_only_the_project_index() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-overlay-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let root = base.join("项目/覆盖校验");
        let level_path = "客户端/dev/Level/Level.txt";
        fs::create_dir_all(root.join("客户端/dev/Level")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Monster")).unwrap();
        fs::write(root.join("引擎/mir_version.txt"), "1.2.0\n").unwrap();
        fs::write(
            root.join(level_path),
            "level=1\nrequiredExperience=100\nstatPoints=1\nrecommendedMonsterId=M1\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Level")).unwrap();
        fs::write(
            root.join("引擎/Mir200/Level/Level.txt"),
            "level=1\nrequiredExperience=100\nstatPoints=1\nrecommendedMonsterId=M1\n",
        )
        .unwrap();
        fs::write(
            root.join("引擎/Mir200/Monster/Monster.txt"),
            "monsterId=M1\ncombatLevel=1\nhealthPoints=10\n",
        )
        .unwrap();
        let store = DomainStore::new(base.join("data")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let draft = store
            .open_draft(&project.id, "MCP overlay validation")
            .unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "level", "1.3.1", None)
            .unwrap();
        store
            .patch_draft(
                &project.id,
                &draft.id,
                0,
                &[DraftChangeInput {
                    path: level_path.to_string(),
                    content: Some(
                        "level=999\nrequiredExperience=100\nstatPoints=1\nrecommendedMonsterId=MISSING\n"
                            .to_string(),
                    ),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap();
        let lease = store
            .issue_task_scope(
                &project.id,
                "task-mcp-overlay",
                &["level".to_string()],
                &["level".to_string()],
                std::slice::from_ref(&draft.id),
                json!({"level":"1.3.1"}),
                mir3_domain::now_millis() + 60_000,
            )
            .unwrap();
        let result = call_tool(
            &store,
            &project.id,
            "mir3_validate",
            json!({
                "scopeToken":lease.token,
                "systemId":"level",
                "draftId":draft.id
            }),
        );
        assert_eq!(result.get("isError"), Some(&Value::Bool(false)));
        assert_eq!(
            result.pointer("/structuredContent/valid"),
            Some(&json!(false))
        );
        assert_eq!(
            result.pointer("/structuredContent/domain/valid"),
            Some(&json!(true))
        );
        assert_eq!(
            result.pointer("/structuredContent/draftValidation/valid"),
            Some(&json!(false))
        );
        assert!(result
            .pointer("/structuredContent/draftValidation/diagnostics")
            .and_then(Value::as_array)
            .is_some_and(|diagnostics| diagnostics.iter().any(|value| value
                .as_str()
                .is_some_and(|value| value.starts_with("DOMAIN_DRAFT_OVERLAY_VALIDATED:")))));
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
        fs::write(root.join("引擎/mir_version.txt"), "1.2.0\n").unwrap();
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
            .bind_draft_domain(&project.id, &draft.id, "map", "1.3.2", None)
            .unwrap();
        let lease = store
            .issue_task_scope(
                &project.id,
                "task-mcp-security",
                &["map".to_string()],
                &["map".to_string()],
                &[],
                json!({"map":"1.3.2"}),
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
        assert!(
            tool_error(&multiline_injection).starts_with("DOMAIN_FIELD_VALUE_INVALID:"),
            "{}",
            tool_error(&multiline_injection)
        );

        let shop_draft = store.open_draft(&project.id, "越权商城能力").unwrap();
        store
            .bind_draft_domain(&project.id, &shop_draft.id, "shop", "1.3.1", None)
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
            Some("1.3.2"),
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

    #[test]
    fn promoted_capability_invokes_in_a_new_session_with_exact_version() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-promoted-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let root = base.join("项目/能力重放");
        let shop_path = root.join("引擎/Mir200/Envir/shop.txt");
        fs::create_dir_all(shop_path.parent().unwrap()).unwrap();
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::write(root.join("引擎/mir_version.txt"), "1.2.0\n").unwrap();
        fs::write(&shop_path, shop_text("O1", 1)).unwrap();
        fs::write(root.join("客户端/dev/shop.txt"), shop_text("O1", 1)).unwrap();
        fs::write(root.join("引擎/Mir200/Envir/cfg_item.txt"), item_text(1)).unwrap();
        fs::write(root.join("引擎/Mir200/Envir/buff.txt"), "buffId=B1\n").unwrap();
        let store = test_store(&base);
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let source = store.open_draft(&project.id, "source task").unwrap();
        store
            .bind_draft_domain(&project.id, &source.id, "shop", "1.3.1", None)
            .unwrap();
        store
            .patch_draft(
                &project.id,
                &source.id,
                0,
                &[DraftChangeInput {
                    path: "引擎/Mir200/Envir/shop.txt".to_string(),
                    content: Some(shop_text("O1", 2)),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap();
        let source_evidence = store
            .record_draft_operation_evidence(
                &project.id,
                &source.id,
                "batch-price-shop",
                &json!({
                    "operation":"batch-price-shop",
                    "resourceIds":["shop:1"],
                    "changes":{"price":2},
                    "expectedRevision":0
                }),
                0,
                1,
            )
            .unwrap();
        let source_change_hash = store
            .draft_change_evidence_hash(&project.id, &source.id)
            .unwrap();
        store
            .seal_draft_operation_replay(
                &project.id,
                &source.id,
                source_evidence.sequence,
                &source_change_hash,
            )
            .unwrap();
        let preview = store.preview_draft(&project.id, &source.id).unwrap();
        let snapshot = store
            .apply_draft(
                &project.id,
                &source.id,
                preview.draft.revision,
                &preview.diff_hash,
            )
            .unwrap();
        let receipt = store
            .record_applied_draft_receipt(&project.id, &source.id, &preview.diff_hash, &snapshot)
            .unwrap()
            .unwrap();
        let capability = store
            .compile_user_capability(
                &project.id,
                &mir3_domain::CapabilityCompileRequest {
                    receipt_id: receipt.id,
                    id: "new-session-reprice".to_string(),
                    name: "New session reprice".to_string(),
                    description: String::new(),
                },
            )
            .unwrap();
        store
            .set_user_capability_status(&project.id, &capability.id, &capability.version, "active")
            .unwrap();
        store
            .promote_user_capability(
                &project.id,
                &mir3_domain::CapabilityPromotionRequest {
                    capability_id: capability.id.clone(),
                    version: capability.version.clone(),
                    target_scope: "personal".to_string(),
                },
            )
            .unwrap();
        store
            .set_user_capability_status(
                &project.id,
                &capability.id,
                &capability.version,
                "disabled",
            )
            .unwrap();
        let reuse_root = base.join("项目/共享能力新项目");
        fs::create_dir_all(reuse_root.join("客户端/dev")).unwrap();
        fs::create_dir_all(reuse_root.join("引擎/Mir200")).unwrap();
        let reuse_project = store.import_project(&reuse_root).unwrap();
        let reuse_lease = store
            .issue_task_scope(
                &reuse_project.id,
                "shared-capability-list",
                &["shop".to_string()],
                &["shop".to_string()],
                &[],
                json!({"shop":"1.3.1"}),
                mir3_domain::now_millis() + 60_000,
            )
            .unwrap();
        let shared_list = call_tool(
            &store,
            &reuse_project.id,
            "mir3_capability_list",
            json!({"scopeToken":reuse_lease.token,"systemId":"shop"}),
        );
        let shared = shared_list
            .pointer("/structuredContent/capabilities")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .find(|item| item["id"] == capability.id)
            .unwrap();
        assert_eq!(shared["resolvedScope"], "personal");

        let xls_path = root.join("引擎/Mir200/Envir/Shop/cfg_store.xls");
        fs::create_dir_all(xls_path.parent().unwrap()).unwrap();
        let mut sheet = Biff8Sheet::new("商品");
        for (row_index, row) in [
            ["offerId", "itemId", "currencyItemId", "price"],
            ["OFFER_A", "ITEM_A", "ITEM_A", "10"],
        ]
        .iter()
        .enumerate()
        {
            for (column_index, value) in row.iter().enumerate() {
                sheet
                    .set(
                        row_index as u32,
                        column_index,
                        Biff8Cell::general(Biff8Value::Text((*value).to_string())),
                    )
                    .unwrap();
            }
        }
        let mut book = Biff8Book::default();
        book.sheets.push(sheet);
        fs::write(&xls_path, book.to_cfb_bytes().unwrap()).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let resource_id = store
            .query_domain_resources(
                &project.id,
                "shop",
                &DomainResourceQuery {
                    text: "OFFER_A".to_string(),
                    resource_type: None,
                    limit: Some(10),
                    offset: None,
                },
            )
            .unwrap()
            .into_iter()
            .find(|resource| resource.label == "OFFER_A")
            .unwrap()
            .id;

        let target = store.open_draft(&project.id, "new session").unwrap();
        store
            .bind_draft_domain(&project.id, &target.id, "shop", "1.3.1", None)
            .unwrap();
        let target_lease = store
            .issue_task_scope(
                &project.id,
                "new-session-task",
                &["shop".to_string()],
                &["shop".to_string()],
                std::slice::from_ref(&target.id),
                json!({"shop":"1.3.1"}),
                mir3_domain::now_millis() + 60_000,
            )
            .unwrap();
        let described = call_tool(
            &store,
            &project.id,
            "mir3_capability_describe",
            json!({
                "scopeToken":target_lease.token.clone(),
                "capabilityId":capability.id,
                "version":capability.version
            }),
        );
        assert_eq!(
            described.pointer("/structuredContent/resolvedScope"),
            Some(&Value::String("personal".to_string()))
        );
        let missing_version = call_tool(
            &store,
            &project.id,
            "mir3_capability_invoke",
            json!({
                "scopeToken":target_lease.token.clone(),
                "capabilityId":capability.id,
                "draftId":target.id,
                "params":{"resourceIds":[resource_id],"changes":{"price":30}}
            }),
        );
        assert!(tool_error(&missing_version).starts_with("MCP_ARGUMENT_INVALID:"));
        let invoked = call_tool(
            &store,
            &project.id,
            "mir3_capability_invoke",
            json!({
                "scopeToken":target_lease.token,
                "capabilityId":capability.id,
                "version":"0.1.0",
                "draftId":target.id,
                "params":{"resourceIds":[resource_id],"changes":{"price":30}}
            }),
        );
        assert_eq!(invoked.get("isError"), Some(&Value::Bool(false)));
        assert_eq!(
            store.get_draft(&project.id, &target.id).unwrap().revision,
            1
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn global_workflow_invokes_all_scoped_composite_drafts() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-global-capability-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let root = base.join("项目/全局能力");
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Envir/Shop")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Envir/Item")).unwrap();
        fs::write(root.join("引擎/mir_version.txt"), "1.2.0\n").unwrap();
        fs::write(root.join("引擎/Mir200/Envir/shop.txt"), shop_text("O1", 1)).unwrap();
        fs::write(root.join("客户端/dev/shop.txt"), shop_text("O1", 1)).unwrap();
        fs::write(root.join("引擎/Mir200/Envir/cfg_item.txt"), item_text(1)).unwrap();
        fs::write(root.join("客户端/dev/cfg_item.txt"), item_text(1)).unwrap();
        fs::write(root.join("引擎/Mir200/Envir/buff.txt"), "buffId=B1\n").unwrap();
        fs::write(
            root.join("引擎/Mir200/Envir/shop-extra.txt"),
            shop_text("O2", 1),
        )
        .unwrap();
        fs::write(root.join("客户端/dev/shop-extra.txt"), shop_text("O2", 1)).unwrap();
        let store = test_store(&base);
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let source_composite = "source-global-workflow";
        let source_cases = [
            (
                "shop",
                "batch-price-shop",
                "引擎/Mir200/Envir/shop.txt",
                shop_text("O1", 2),
                json!({"operation":"batch-price-shop","resourceIds":["shop:source"],"changes":{"price":2},"expectedRevision":0}),
            ),
            (
                "item",
                "batch-edit-item",
                "引擎/Mir200/Envir/cfg_item.txt",
                item_text(2),
                json!({"operation":"batch-edit-item","resourceIds":["item:source"],"changes":{"stackLimit":2},"expectedRevision":0}),
            ),
            (
                "shop",
                "batch-price-shop",
                "引擎/Mir200/Envir/shop-extra.txt",
                shop_text("O2", 3),
                json!({"operation":"batch-price-shop","resourceIds":["shop:source-2"],"changes":{"price":3},"expectedRevision":0}),
            ),
        ];
        let mut confirmations = Vec::new();
        for (system_id, operation_id, path, content, parameters) in source_cases {
            let draft = store.open_draft(&project.id, operation_id).unwrap();
            store
                .bind_draft_domain(
                    &project.id,
                    &draft.id,
                    system_id,
                    "1.3.1",
                    Some(source_composite),
                )
                .unwrap();
            store
                .patch_draft(
                    &project.id,
                    &draft.id,
                    0,
                    &[DraftChangeInput {
                        path: path.to_string(),
                        content: Some(content),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )
                .unwrap();
            let evidence = store
                .record_draft_operation_evidence(
                    &project.id,
                    &draft.id,
                    operation_id,
                    &parameters,
                    0,
                    1,
                )
                .unwrap();
            let change_hash = store
                .draft_change_evidence_hash(&project.id, &draft.id)
                .unwrap();
            store
                .seal_draft_operation_replay(
                    &project.id,
                    &draft.id,
                    evidence.sequence,
                    &change_hash,
                )
                .unwrap();
            let preview = store.preview_draft(&project.id, &draft.id).unwrap();
            confirmations.push(mir3_domain::CompositeDraftConfirmation {
                draft_id: draft.id,
                expected_revision: preview.draft.revision,
                expected_diff_hash: preview.diff_hash,
            });
        }
        let source_task_id = "source-global-workflow-task";
        let source_drafts = confirmations
            .iter()
            .map(|confirmation| confirmation.draft_id.clone())
            .collect::<Vec<_>>();
        store
            .issue_task_scope(
                &project.id,
                source_task_id,
                &["shop".to_string(), "item".to_string()],
                &["shop".to_string(), "item".to_string()],
                &source_drafts,
                json!({"shop":"1.3.1","item":"1.3.1"}),
                mir3_domain::now_millis() + 60_000,
            )
            .unwrap();
        store
            .apply_validated_composite_drafts_with_governance(
                &project.id,
                source_composite,
                &confirmations,
            )
            .unwrap();
        let receipts = store
            .list_task_receipts(&project.id, None)
            .unwrap()
            .into_iter()
            .filter(|receipt| receipt.task_id == source_task_id)
            .collect::<Vec<_>>();
        assert_eq!(receipts.len(), confirmations.len());
        let capability = store
            .compile_global_workflow_capability(
                &project.id,
                &mir3_domain::GlobalCapabilityCompileRequest {
                    receipt_ids: receipts.iter().map(|receipt| receipt.id.clone()).collect(),
                    id: "global-shop-item-replay".to_string(),
                    name: "Global shop item replay".to_string(),
                    description: String::new(),
                },
            )
            .unwrap();
        store
            .set_user_capability_status(&project.id, &capability.id, &capability.version, "active")
            .unwrap();
        write_contract_xls_rows(
            &root.join("引擎/Mir200/Envir/Shop/cfg_store.xls"),
            "商品",
            &[
                "offerId",
                "shopId",
                "itemId",
                "currencyItemId",
                "price",
                "startEpochSeconds",
                "endEpochSeconds",
            ],
            &[
                &[
                    "OFFER_A",
                    "SHOP_A",
                    "ITEM_A",
                    "ITEM_A",
                    "10",
                    "0",
                    "4102444800",
                ],
                &[
                    "OFFER_B",
                    "SHOP_A",
                    "ITEM_A",
                    "ITEM_A",
                    "11",
                    "0",
                    "4102444800",
                ],
            ],
        );
        write_contract_xls(
            &root.join("引擎/Mir200/Envir/Item/cfg_item.xls"),
            "物品",
            &[
                "itemId",
                "itemType",
                "stackLimit",
                "clientIcon",
                "engineStdMode",
                "linkedBuffId",
            ],
            &[
                "ITEM_A",
                "material",
                "10",
                "item-a.png",
                "1",
                "buff:fixture-1",
            ],
        );
        store.scan_project(&project.id, || false).unwrap();
        let shop_resource = find_resource_id(&store, &project.id, "shop", "OFFER_A");
        let second_shop_resource = find_resource_id(&store, &project.id, "shop", "OFFER_B");
        let item_resource = find_resource_id(&store, &project.id, "item", "ITEM_A");
        let target_composite = "target-global-workflow";
        let mut target_drafts = Vec::new();
        for system_id in ["shop", "item"] {
            let draft = store.open_draft(&project.id, system_id).unwrap();
            store
                .bind_draft_domain(
                    &project.id,
                    &draft.id,
                    system_id,
                    "1.3.1",
                    Some(target_composite),
                )
                .unwrap();
            target_drafts.push(draft.id);
        }
        let lease = store
            .issue_task_scope(
                &project.id,
                "global-workflow-task",
                &["shop".to_string(), "item".to_string()],
                &["shop".to_string(), "item".to_string()],
                &target_drafts,
                json!({"shop":"1.3.1","item":"1.3.1"}),
                mir3_domain::now_millis() + 60_000,
            )
            .unwrap();
        let mut parameters = serde_json::Map::new();
        let mut shop_step_index = 0;
        for step in capability.steps.as_array().unwrap() {
            let key = step["parameterKey"].as_str().unwrap();
            let system_id = step["systemId"].as_str().unwrap();
            let value = if system_id == "shop" {
                let resource_id = if shop_step_index == 0 {
                    shop_resource.clone()
                } else {
                    second_shop_resource.clone()
                };
                shop_step_index += 1;
                json!({"resourceIds":[resource_id],"changes":{"price":20 + shop_step_index}})
            } else {
                json!({"resourceIds":[item_resource.clone()],"changes":{"stackLimit":20}})
            };
            parameters.insert(key.to_string(), value);
        }
        fs::remove_file(root.join("引擎/Mir200/Envir/Item/cfg_item.xls")).unwrap();
        let failed = call_tool(
            &store,
            &project.id,
            "mir3_capability_invoke",
            json!({
                "scopeToken":lease.token,
                "capabilityId":capability.id,
                "version":capability.version,
                "compositeId":target_composite,
                "params":parameters.clone(),
            }),
        );
        assert_eq!(
            failed.get("isError"),
            Some(&Value::Bool(true)),
            "{failed:#}"
        );
        for draft_id in &target_drafts {
            assert_eq!(store.get_draft(&project.id, draft_id).unwrap().revision, 0);
            assert!(store
                .list_draft_operation_evidence(&project.id, draft_id)
                .unwrap()
                .is_empty());
        }
        write_contract_xls(
            &root.join("引擎/Mir200/Envir/Item/cfg_item.xls"),
            "物品",
            &[
                "itemId",
                "itemType",
                "stackLimit",
                "clientIcon",
                "engineStdMode",
                "linkedBuffId",
            ],
            &[
                "ITEM_A",
                "material",
                "10",
                "item-a.png",
                "1",
                "buff:fixture-1",
            ],
        );
        let invoked = call_tool(
            &store,
            &project.id,
            "mir3_capability_invoke",
            json!({
                "scopeToken":lease.token,
                "capabilityId":capability.id,
                "version":capability.version,
                "compositeId":target_composite,
                "params":parameters,
            }),
        );
        assert_eq!(
            invoked.get("isError"),
            Some(&Value::Bool(false)),
            "{invoked:#}"
        );
        assert_eq!(
            invoked
                .pointer("/structuredContent/drafts")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        let bindings = store
            .list_composite_draft_bindings(&project.id, target_composite)
            .unwrap();
        assert_eq!(
            bindings
                .iter()
                .find(|binding| binding.system_id == "shop")
                .unwrap()
                .revision,
            2
        );
        assert_eq!(
            bindings
                .iter()
                .find(|binding| binding.system_id == "item")
                .unwrap()
                .revision,
            1
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn global_workflow_mcp_scales_across_three_eight_and_all_domains() {
        let base = std::env::temp_dir().join(format!(
            "mir3-mcp-global-matrix-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let root = base.join("项目/全领域组合能力");
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/Envir/domains")).unwrap();
        fs::write(root.join("引擎/mir_version.txt"), "1.2.0\n").unwrap();
        let pack_root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/mir3-domain-packs");
        let bootstrap = DomainStore::new(base.join("bootstrap-global")).unwrap();
        let manifests = bootstrap.list_domain_systems().unwrap();
        let fixture_records = normalized_fixture_records(&pack_root, &manifests);
        for manifest in &manifests {
            let selector = manifest.file_projection.owned_selectors.first().unwrap();
            for projection_root in ["客户端/dev/domains", "引擎/Mir200/Envir/domains"] {
                let directory = root.join(projection_root).join(&manifest.system_id);
                fs::create_dir_all(&directory).unwrap();
                for (index, record) in fixture_records[&manifest.system_id]
                    .iter()
                    .take(2)
                    .enumerate()
                {
                    fs::write(
                        directory.join(format!("{selector}_global_{}.txt", index + 1)),
                        record_text(record),
                    )
                    .unwrap();
                }
            }
            if manifest.capabilities.iter().any(|capability| {
                !capability.write_systems.is_empty()
                    && !is_shaped_operation(&capability.id)
                    && capability
                        .steps
                        .first()
                        .is_some_and(|step| step.primitive == "xls")
            }) {
                let xls_records =
                    xls_fixture_records(&fixture_records[&manifest.system_id], manifest);
                let headers = xls_records[0]
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>();
                let mut sheet = Biff8Sheet::new("records");
                for (column, header) in headers.iter().enumerate() {
                    sheet
                        .set(
                            0,
                            column,
                            Biff8Cell::general(Biff8Value::Text(header.clone())),
                        )
                        .unwrap();
                }
                for (row_index, record) in xls_records.iter().take(2).enumerate() {
                    for (column, header) in headers.iter().enumerate() {
                        let value = record[header]
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| record[header].to_string());
                        sheet
                            .set(
                                u32::try_from(row_index + 1).unwrap(),
                                column,
                                Biff8Cell::general(Biff8Value::Text(value)),
                            )
                            .unwrap();
                    }
                }
                let mut book = Biff8Book::default();
                book.sheets.push(sheet);
                let bytes = book.to_cfb_bytes().unwrap();
                for projection_root in ["客户端/dev/domains", "引擎/Mir200/Envir/domains"] {
                    fs::write(
                        root.join(projection_root)
                            .join(&manifest.system_id)
                            .join(format!("{selector}_global.xls")),
                        &bytes,
                    )
                    .unwrap();
                }
            }
        }
        let map_directory = root.join("引擎/Mir200/map");
        fs::create_dir_all(&map_directory).unwrap();
        let mut map_bytes = vec![0_u8; 28 + 4 * 3 + 16 * 14];
        map_bytes[22..24].copy_from_slice(&4_u16.to_le_bytes());
        map_bytes[24..26].copy_from_slice(&4_u16.to_le_bytes());
        fs::write(map_directory.join("global-region.map"), map_bytes).unwrap();
        let store = DomainStore::new(base.join("data-global")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();

        for count in [3_usize, 8, 33] {
            let source_composite = format!("global-matrix-source-{count}");
            let target_composite = format!("global-matrix-target-{count}");
            let selected = manifests.iter().take(count).collect::<Vec<_>>();
            let mut confirmations = Vec::with_capacity(count);
            let mut invocation_params = BTreeMap::<String, Value>::new();
            for manifest in &selected {
                let files = store
                    .query_domain_files(
                        &project.id,
                        &manifest.system_id,
                        &DomainFileQuery {
                            text: format!("domains/{}/", manifest.system_id),
                            limit: Some(100),
                            offset: None,
                        },
                    )
                    .unwrap();
                let text_source_ids = files
                    .iter()
                    .filter(|file| {
                        file.path.ends_with("_global_1.txt") && file.access != "readonly"
                    })
                    .map(|file| file.resource_id.clone())
                    .collect::<Vec<_>>();
                assert_eq!(text_source_ids.len(), 2, "{}", manifest.system_id);
                let xls_source_ids = store
                    .query_domain_resources(
                        &project.id,
                        &manifest.system_id,
                        &DomainResourceQuery {
                            text: format!("domains/{}/", manifest.system_id),
                            resource_type: None,
                            limit: Some(100),
                            offset: None,
                        },
                    )
                    .unwrap()
                    .into_iter()
                    .filter(|resource| {
                        resource.source.path.ends_with(".xls")
                            && resource.source.row == Some(2)
                            && resource.writable
                    })
                    .map(|resource| resource.id)
                    .collect::<Vec<_>>();
                let mut selected_capability = None;
                for capability in manifest
                    .capabilities
                    .iter()
                    .filter(|capability| !capability.write_systems.is_empty())
                {
                    let source_ids = if !is_shaped_operation(&capability.id)
                        && capability
                            .steps
                            .first()
                            .is_some_and(|step| step.primitive == "xls")
                    {
                        &xls_source_ids
                    } else {
                        &text_source_ids
                    };
                    let params = shaped_test_params(
                        &capability.id,
                        &capability.parameter_schema,
                        source_ids,
                        &fixture_records[&manifest.system_id],
                        &store,
                        &project.id,
                        &manifest.system_id,
                    );
                    validate_json_schema(&capability.parameter_schema, &params, "params").unwrap();
                    let draft = store
                        .open_draft(
                            &project.id,
                            &format!(
                                "global matrix source {} {}",
                                manifest.system_id, capability.id
                            ),
                        )
                        .unwrap();
                    store
                        .bind_draft_domain(
                            &project.id,
                            &draft.id,
                            &manifest.system_id,
                            &manifest.version,
                            None,
                        )
                        .unwrap();
                    let result = execute_manifest_operation(
                        &store,
                        &project.id,
                        &draft.id,
                        &capability.id,
                        &manifest.system_id,
                        &capability.steps,
                        &params,
                    )
                    .unwrap_or_else(|error| {
                        panic!(
                            "{}:{} source compilation failed: {error}",
                            manifest.system_id, capability.id
                        )
                    });
                    let validation = store.validate_domain_draft(&project.id, &draft.id).unwrap();
                    if !validation.valid {
                        continue;
                    }
                    let preview = store.preview_draft(&project.id, &draft.id).unwrap();
                    assert_structured_draft_diff(
                        &manifest.system_id,
                        &capability.id,
                        &result,
                        &preview,
                    );
                    store
                        .associate_draft_composite(
                            &project.id,
                            &draft.id,
                            &manifest.system_id,
                            &manifest.version,
                            &source_composite,
                        )
                        .unwrap();
                    confirmations.push(mir3_domain::CompositeDraftConfirmation {
                        draft_id: draft.id,
                        expected_revision: preview.draft.revision,
                        expected_diff_hash: preview.diff_hash,
                    });
                    let mut supplied = params.as_object().unwrap().clone();
                    supplied.remove("operation");
                    supplied.remove("expectedRevision");
                    invocation_params.insert(manifest.system_id.clone(), Value::Object(supplied));
                    selected_capability = Some(capability.id.clone());
                    break;
                }
                assert!(
                    selected_capability.is_some(),
                    "{} has no valid representative for a global workflow",
                    manifest.system_id
                );
            }
            let source_systems = selected
                .iter()
                .map(|manifest| manifest.system_id.clone())
                .collect::<Vec<_>>();
            let source_drafts = confirmations
                .iter()
                .map(|confirmation| confirmation.draft_id.clone())
                .collect::<Vec<_>>();
            let source_versions = selected
                .iter()
                .map(|manifest| {
                    (
                        manifest.system_id.clone(),
                        Value::String(manifest.version.clone()),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            let source_task_id = format!("global-matrix-source-task-{count}");
            store
                .issue_task_scope(
                    &project.id,
                    &source_task_id,
                    &source_systems,
                    &source_systems,
                    &source_drafts,
                    Value::Object(source_versions),
                    mir3_domain::now_millis() + 60_000,
                )
                .unwrap();
            let applied = store
                .apply_validated_composite_drafts_with_governance(
                    &project.id,
                    &source_composite,
                    &confirmations,
                )
                .unwrap();
            let receipts = store
                .list_task_receipts(&project.id, None)
                .unwrap()
                .into_iter()
                .filter(|receipt| receipt.task_id == source_task_id)
                .collect::<Vec<_>>();
            assert_eq!(receipts.len(), count);
            let capability = store
                .compile_global_workflow_capability(
                    &project.id,
                    &mir3_domain::GlobalCapabilityCompileRequest {
                        receipt_ids: receipts.iter().map(|receipt| receipt.id.clone()).collect(),
                        id: format!("global-matrix-{count}"),
                        name: format!("Global matrix {count}"),
                        description: String::new(),
                    },
                )
                .unwrap();
            store
                .set_user_capability_status(
                    &project.id,
                    &capability.id,
                    &capability.version,
                    "active",
                )
                .unwrap();
            store
                .restore_snapshot(&project.id, &applied.snapshot.id)
                .unwrap();
            store.scan_project(&project.id, || false).unwrap();

            let mut target_drafts = Vec::with_capacity(count);
            for manifest in &selected {
                let draft = store
                    .open_draft(
                        &project.id,
                        &format!("global matrix target {}", manifest.system_id),
                    )
                    .unwrap();
                store
                    .bind_draft_domain(
                        &project.id,
                        &draft.id,
                        &manifest.system_id,
                        &manifest.version,
                        Some(&target_composite),
                    )
                    .unwrap();
                target_drafts.push(draft.id);
            }
            let systems = selected
                .iter()
                .map(|manifest| manifest.system_id.clone())
                .collect::<Vec<_>>();
            let versions = selected
                .iter()
                .map(|manifest| {
                    (
                        manifest.system_id.clone(),
                        Value::String(manifest.version.clone()),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            let lease = store
                .issue_task_scope(
                    &project.id,
                    &format!("global-matrix-task-{count}"),
                    &systems,
                    &systems,
                    &target_drafts,
                    Value::Object(versions),
                    mir3_domain::now_millis() + 60_000,
                )
                .unwrap();
            let mut parameters = serde_json::Map::new();
            for step in capability.steps.as_array().unwrap() {
                let system_id = step["systemId"].as_str().unwrap();
                let parameter_key = step["parameterKey"].as_str().unwrap();
                parameters.insert(
                    parameter_key.to_string(),
                    invocation_params[system_id].clone(),
                );
            }
            let invoked = call_tool(
                &store,
                &project.id,
                "mir3_capability_invoke",
                json!({
                    "scopeToken":lease.token,
                    "capabilityId":capability.id,
                    "version":capability.version,
                    "compositeId":target_composite,
                    "params":parameters,
                }),
            );
            assert_eq!(
                invoked.get("isError"),
                Some(&Value::Bool(false)),
                "{count}-system workflow failed: {invoked:#}"
            );
            assert_eq!(
                invoked
                    .pointer("/structuredContent/results")
                    .and_then(Value::as_array)
                    .map(Vec::len),
                Some(count),
                "{invoked:#}"
            );
            assert_eq!(
                invoked
                    .pointer("/structuredContent/drafts")
                    .and_then(Value::as_array)
                    .map(Vec::len),
                Some(count),
                "{invoked:#}"
            );
            let bindings = store
                .list_composite_draft_bindings(&project.id, &target_composite)
                .unwrap();
            assert_eq!(bindings.len(), count);
            for binding in bindings {
                assert!(binding.revision > 0, "{}", binding.system_id);
                let preview = store.preview_draft(&project.id, &binding.draft_id).unwrap();
                assert!(!preview.changes.is_empty(), "{}", binding.system_id);
                assert!(!preview.diff_hash.is_empty(), "{}", binding.system_id);
                assert_eq!(
                    store
                        .list_draft_operation_evidence(&project.id, &binding.draft_id)
                        .unwrap()
                        .len(),
                    1,
                    "{}",
                    binding.system_id
                );
            }
        }
        fs::remove_dir_all(base).ok();
    }

    fn write_contract_xls(
        path: &std::path::Path,
        sheet_name: &str,
        headers: &[&str],
        row: &[&str],
    ) {
        write_contract_xls_rows(path, sheet_name, headers, &[row]);
    }

    fn write_contract_xls_rows(
        path: &std::path::Path,
        sheet_name: &str,
        headers: &[&str],
        rows: &[&[&str]],
    ) {
        let mut sheet = Biff8Sheet::new(sheet_name);
        for (column, value) in headers.iter().enumerate() {
            sheet
                .set(
                    0,
                    column,
                    Biff8Cell::general(Biff8Value::Text((*value).to_string())),
                )
                .unwrap();
        }
        for (row_index, row) in rows.iter().enumerate() {
            for (column, value) in row.iter().enumerate() {
                sheet
                    .set(
                        u32::try_from(row_index + 1).unwrap(),
                        column,
                        Biff8Cell::general(Biff8Value::Text((*value).to_string())),
                    )
                    .unwrap();
            }
        }
        let mut book = Biff8Book::default();
        book.sheets.push(sheet);
        fs::write(path, book.to_cfb_bytes().unwrap()).unwrap();
    }

    fn test_store(base: &std::path::Path) -> DomainStore {
        DomainStore::new(base.join("data")).unwrap()
    }

    fn find_resource_id(
        store: &DomainStore,
        project_id: &str,
        system_id: &str,
        label: &str,
    ) -> String {
        store
            .query_domain_resources(
                project_id,
                system_id,
                &DomainResourceQuery {
                    text: label.to_string(),
                    resource_type: None,
                    limit: Some(10),
                    offset: None,
                },
            )
            .unwrap()
            .into_iter()
            .find(|resource| resource.label == label)
            .unwrap()
            .id
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

    #[derive(Debug, Default)]
    struct CapabilityLifecycleCoverage {
        compiled: Vec<String>,
        applied: Vec<String>,
        rejected: Vec<String>,
    }

    fn normalized_fixture_records(
        pack_root: &std::path::Path,
        manifests: &[DomainManifest],
    ) -> BTreeMap<String, Vec<Value>> {
        let mut records = manifests
            .iter()
            .map(|manifest| {
                let fixture: Value = serde_json::from_slice(
                    &fs::read(
                        pack_root
                            .join(&manifest.system_id)
                            .join("fixtures/valid.json"),
                    )
                    .unwrap(),
                )
                .unwrap();
                (
                    manifest.system_id.clone(),
                    fixture["records"].as_array().unwrap().clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let reference_values = manifests
            .iter()
            .map(|manifest| {
                let primary = manifest.resources.unique_key.first().unwrap();
                let values = records[&manifest.system_id]
                    .iter()
                    .map(|record| record[primary].clone())
                    .collect::<Vec<_>>();
                (manifest.system_id.clone(), values)
            })
            .collect::<BTreeMap<_, _>>();
        for manifest in manifests {
            let domain_records = records.get_mut(&manifest.system_id).unwrap();
            for (edge_index, edge) in manifest.resources.dependency_edges.iter().enumerate() {
                let values = &reference_values[&edge.system_id];
                assert!(!values.is_empty(), "{}", edge.system_id);
                for (record_index, record) in domain_records.iter_mut().enumerate() {
                    record[&edge.field] =
                        values[(record_index + edge_index) % values.len()].clone();
                }
            }
        }
        records
    }

    fn xls_fixture_records(records: &[Value], manifest: &DomainManifest) -> Vec<Value> {
        let offset = i64::try_from(records.len()).unwrap();
        records
            .iter()
            .map(|record| {
                let mut record = record.clone();
                for key in manifest.resources.unique_key.iter().take(1) {
                    let value = record.get(key).unwrap().clone();
                    record[key] = match value {
                        Value::String(value) => Value::String(format!("xls-{value}")),
                        Value::Number(value) => value
                            .as_i64()
                            .map(|value| Value::from(value + offset))
                            .or_else(|| {
                                value.as_f64().and_then(|value| {
                                    serde_json::Number::from_f64(value + offset as f64)
                                        .map(Value::Number)
                                })
                            })
                            .unwrap(),
                        value => panic!("unsupported fixture unique key value: {value}"),
                    };
                }
                if manifest.system_id == "vip" {
                    if let Some(value) = record.get("requiredPoints").and_then(Value::as_i64) {
                        record["requiredPoints"] = Value::from(value + 1_000_000);
                    }
                }
                record
            })
            .collect()
    }

    fn assert_structured_draft_diff(
        system_id: &str,
        capability_id: &str,
        result: &Value,
        preview: &mir3_domain::DraftPreview,
    ) {
        assert!(
            result
                .get("results")
                .and_then(Value::as_array)
                .is_some_and(|steps| !steps.is_empty() && steps.iter().all(Value::is_object)),
            "{system_id}:{capability_id} returned no structured primitive changes: {result:#}"
        );
        assert!(
            !preview.changes.is_empty(),
            "{system_id}:{capability_id} produced an empty Diff"
        );
        for change in &preview.changes {
            assert!(
                change.deleted || change.base_sha256 != change.new_sha256,
                "{system_id}:{capability_id} contains a no-op change: {}",
                change.path
            );
            match &change.unified_diff {
                Some(diff) => assert!(
                    !diff.trim().is_empty() && diff.contains("--- a/") && diff.contains("+++ b/"),
                    "{system_id}:{capability_id} has an invalid text Diff: {}",
                    change.path
                ),
                None => assert!(
                    change.path.ends_with(".xls") || change.path.ends_with(".map"),
                    "{system_id}:{capability_id} omitted Diff for a supported text file: {}",
                    change.path
                ),
            }
        }
    }

    fn capture_preview_bytes(
        root: &std::path::Path,
        preview: &mir3_domain::DraftPreview,
    ) -> BTreeMap<String, Option<Vec<u8>>> {
        preview
            .changes
            .iter()
            .map(|change| (change.path.clone(), fs::read(root.join(&change.path)).ok()))
            .collect()
    }

    fn assert_project_bytes_changed(
        root: &std::path::Path,
        original: &BTreeMap<String, Option<Vec<u8>>>,
        system_id: &str,
        capability_id: &str,
    ) {
        assert!(
            original
                .iter()
                .any(|(path, bytes)| fs::read(root.join(path)).ok() != *bytes),
            "{system_id}:{capability_id} Apply did not change real project bytes"
        );
    }

    fn assert_project_bytes_restored(
        root: &std::path::Path,
        original: &BTreeMap<String, Option<Vec<u8>>>,
        system_id: &str,
        capability_id: &str,
    ) {
        for (path, bytes) in original {
            assert_eq!(
                fs::read(root.join(path)).ok(),
                *bytes,
                "{system_id}:{capability_id} did not restore {path} byte-for-byte"
            );
        }
    }

    fn assert_capability_lifecycle_coverage(
        coverage: &BTreeMap<String, CapabilityLifecycleCoverage>,
    ) {
        assert_eq!(coverage.len(), 33, "{coverage:#?}");
        let mut compiled = 0;
        let mut applied = 0;
        let mut rejected = 0;
        let mut missing_applied = Vec::new();
        for (system_id, lifecycle) in coverage {
            assert!(
                !lifecycle.compiled.is_empty(),
                "{system_id} has no writable capability coverage"
            );
            assert_eq!(
                lifecycle.compiled.len(),
                lifecycle.applied.len() + lifecycle.rejected.len(),
                "{system_id} has an unclassified capability: {lifecycle:#?}"
            );
            if lifecycle.applied.is_empty() {
                missing_applied.push((system_id, &lifecycle.rejected));
            }
            compiled += lifecycle.compiled.len();
            applied += lifecycle.applied.len();
            rejected += lifecycle.rejected.len();
        }
        assert_eq!(compiled, 155, "{coverage:#?}");
        assert_eq!(applied + rejected, 155, "{coverage:#?}");
        assert!(
            missing_applied.is_empty(),
            "systems without a representative Apply/restore lifecycle: {missing_applied:#?}"
        );
    }

    fn record_text(record: &Value) -> String {
        record
            .as_object()
            .unwrap()
            .iter()
            .map(|(field, value)| {
                format!(
                    "{field}={}\n",
                    value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| value.to_string())
                )
            })
            .collect()
    }

    fn shop_text(offer_id: &str, price: usize) -> String {
        format!(
            "offerId={offer_id}\nshopId=S1\nitemId=I1\ncurrencyItemId=I1\nprice={price}\nstartEpochSeconds=0\nendEpochSeconds=1\n"
        )
    }

    fn item_text(stack_limit: usize) -> String {
        format!(
            "itemId=I1\nitemType=consumable\nstackLimit={stack_limit}\nclientIcon=icons/I1.png\nengineStdMode=1\nlinkedBuffId=B1\n"
        )
    }

    fn shaped_test_params(
        operation_id: &str,
        schema: &Value,
        source_ids: &[String],
        records: &[Value],
        store: &DomainStore,
        project_id: &str,
        system_id: &str,
    ) -> Value {
        if operation_id == "edit-map-region" {
            let map = store
                .query_domain_files(
                    project_id,
                    system_id,
                    &DomainFileQuery {
                        text: "region.map".to_string(),
                        limit: Some(10),
                        offset: None,
                    },
                )
                .unwrap()
                .into_iter()
                .find(|file| file.path.ends_with("region.map"))
                .unwrap();
            return json!({
                "operation": operation_id,
                "resourceId": map.resource_id,
                "operations": [{
                    "type":"setCollision",
                    "x":1,
                    "y":1,
                    "walkable":true,
                    "frontBlocked":false
                }],
                "expectedRevision":0
            });
        }
        let family = operation_id.split('-').next().unwrap();
        let mut params = json!({"operation": operation_id, "expectedRevision": 0});
        match family {
            "clone" => {
                params["sourceResourceId"] = json!(source_ids[0]);
                params["newResourceId"] = json!(format!("clone-{}", operation_id));
            }
            "generate" => {
                params["templateResourceId"] = json!(source_ids[0]);
                params["firstOrdinal"] = json!(11);
                params["lastOrdinal"] = json!(11);
            }
            "scale" => {
                params["resourceIds"] = json!(source_ids);
                params["factor"] = json!(2);
                params["fields"] =
                    json!([first_schema_enum(schema, "/properties/fields/items/enum")]);
            }
            "interpolate" => {
                params["anchorResourceIds"] = json!([source_ids[0]]);
                params["firstOrdinal"] = json!(11);
                params["lastOrdinal"] = json!(11);
                params["numericFields"] = json!([first_schema_enum(
                    schema,
                    "/properties/numericFields/items/enum"
                )]);
            }
            "add" => {
                params["record"] = records[1].clone();
                params["insertAfterResourceId"] = json!(source_ids[0]);
            }
            "insert" => {
                params["parentResourceId"] = json!(source_ids[0]);
                params["insertionIndex"] = json!(1);
                params["record"] = records[1].clone();
            }
            "fill" => {
                params["cycleResourceId"] = json!(source_ids[0]);
                params["firstSlot"] = json!(2);
                params["lastSlot"] = json!(2);
                params["rewardTemplate"] =
                    first_patch(schema, "/properties/rewardTemplate/properties", &records[1]);
            }
            "tune" => {
                params["resourceIds"] = json!(source_ids);
                params["adjustmentMode"] = json!("delta");
                params["amount"] = json!(1);
                params["fields"] =
                    json!([first_schema_enum(schema, "/properties/fields/items/enum")]);
            }
            "bind" => {
                params["resourceId"] = json!(source_ids[0]);
                params["targetReference"] = json!("replacement-fixture");
                params["referenceField"] =
                    json!(first_schema_enum(schema, "/properties/referenceField/enum"));
            }
            "move" => {
                params["resourceId"] = json!(source_ids[0]);
                params["destinationMapId"] = json!("map-fixture");
                params["coordinateX"] = json!(12);
                params["coordinateY"] = json!(24);
            }
            "schedule" => {
                params["resourceIds"] = json!(source_ids);
                params["startEpochSeconds"] = json!(100);
                params["endEpochSeconds"] = json!(200);
                params["timezone"] = json!("Asia/Shanghai");
            }
            "shift" => {
                params["resourceIds"] = json!(source_ids);
                params["offsetSeconds"] = if operation_id == "shift-launch-schedule" {
                    json!(86_400)
                } else {
                    json!(60)
                };
            }
            "replace" => {
                let field = first_schema_enum(schema, "/properties/referenceField/enum");
                params["resourceIds"] = json!(source_ids);
                params["referenceField"] = json!(field);
                params["fromReference"] = records[0][&field].clone();
                params["toReference"] = records[1][&field].clone();
            }
            "batch" | "edit" => {
                params["resourceIds"] = json!(source_ids);
                params["changes"] =
                    first_patch(schema, "/properties/changes/properties", &records[1]);
            }
            family => panic!("unsupported test family {family}"),
        }
        params
    }

    fn first_schema_enum(schema: &Value, pointer: &str) -> String {
        schema
            .pointer(pointer)
            .and_then(Value::as_array)
            .and_then(|values| values.first())
            .and_then(Value::as_str)
            .unwrap()
            .to_string()
    }

    fn first_patch(schema: &Value, pointer: &str, record: &Value) -> Value {
        let field = schema
            .pointer(pointer)
            .and_then(Value::as_object)
            .and_then(|properties| properties.keys().next())
            .unwrap();
        json!({field: record.get(field).unwrap().clone()})
    }
}
