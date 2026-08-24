use std::collections::BTreeMap;

use serde_json::json;

use crate::engine::execute_scene;
use crate::mocks;
use crate::model::{
    CatalogResult, DataProvenance, DataProvenanceKind, DiagnosticSeverity, EventRequest,
    ReloadRequest, RuntimeCapabilities, RuntimeDiagnostic, RuntimeError, RuntimeOperation,
    RuntimeRequest, RuntimeResponse, RuntimeResult, SceneResult, StartRequest, StopResult,
    PROTOCOL_NAME, PROTOCOL_VERSION,
};

#[derive(Debug, Clone)]
struct RuntimeSession {
    request: StartRequest,
    sequence: u64,
}

#[derive(Debug, Default)]
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
        let request_id = request.request_id;
        if request.protocol_version != PROTOCOL_VERSION {
            return failure(
                request_id,
                "RUNTIME_PROTOCOL_VERSION",
                format!(
                    "仅支持协议版本 {PROTOCOL_VERSION}，收到 {}",
                    request.protocol_version
                ),
            );
        }
        match request.operation {
            RuntimeOperation::Catalog(_) => success(
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
                    },
                }),
            ),
            RuntimeOperation::Start(params) => self.start(request_id, params),
            RuntimeOperation::Event(params) => self.event(request_id, params),
            RuntimeOperation::Reload(params) => self.reload(request_id, params),
            RuntimeOperation::Stop(params) => {
                let stopped = self.sessions.remove(&params.session_id).is_some();
                success(
                    request_id,
                    RuntimeResult::Stopped(StopResult {
                        session_id: params.session_id,
                        stopped,
                    }),
                )
            }
        }
    }

    fn start(&mut self, request_id: String, request: StartRequest) -> RuntimeResponse {
        let session_id = format!("runtime-{}", self.next_session);
        self.next_session += 1;
        match render(&session_id, 1, &request) {
            Ok(result) => {
                self.sessions.insert(
                    session_id,
                    RuntimeSession {
                        request,
                        sequence: 1,
                    },
                );
                success(request_id, RuntimeResult::Scene(result))
            }
            Err(error) => failure_from_string(request_id, error),
        }
    }

    fn event(&mut self, request_id: String, event: EventRequest) -> RuntimeResponse {
        let Some(session) = self.sessions.get_mut(&event.session_id) else {
            return failure(
                request_id,
                "RUNTIME_SESSION_NOT_FOUND",
                format!("会话不存在：{}", event.session_id),
            );
        };
        session.sequence += 1;
        session
            .request
            .data_profile
            .values
            .insert("__eventName".to_string(), json!(event.name));
        session
            .request
            .data_profile
            .values
            .insert("__eventPayload".to_string(), event.payload);
        match render(&event.session_id, session.sequence, &session.request) {
            Ok(mut result) => {
                result.scene.provenance.push(DataProvenance {
                    kind: DataProvenanceKind::RuntimeDerived,
                    key: "event".to_string(),
                    description: "事件通过只读快照重新渲染，不执行真实游戏副作用".to_string(),
                });
                success(request_id, RuntimeResult::Scene(result))
            }
            Err(error) => failure_from_string(request_id, error),
        }
    }

    fn reload(&mut self, request_id: String, reload: ReloadRequest) -> RuntimeResponse {
        let Some(session) = self.sessions.get_mut(&reload.session_id) else {
            return failure(
                request_id,
                "RUNTIME_SESSION_NOT_FOUND",
                format!("会话不存在：{}", reload.session_id),
            );
        };
        session.sequence += 1;
        session.request.layout_path = reload.layout_path;
        session.request.modules = reload.modules;
        if let Some(data_profile) = reload.data_profile {
            session.request.data_profile = data_profile;
        }
        match render(&reload.session_id, session.sequence, &session.request) {
            Ok(result) => success(request_id, RuntimeResult::Scene(result)),
            Err(error) => failure_from_string(request_id, error),
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
            "unknown".to_string(),
            "RUNTIME_INVALID_REQUEST",
            error.to_string(),
        ),
    }
}

fn render(session_id: &str, sequence: u64, request: &StartRequest) -> Result<SceneResult, String> {
    let limits = request.limits.unwrap_or_default().sandboxed();
    let fallback = if request.modules.contains_key(&request.layout_path) {
        None
    } else {
        mocks::source(&request.scene_id)
    };
    let scene = execute_scene(
        &request.scene_id,
        &request.layout_path,
        request.viewport.clone(),
        &request.modules,
        &request.data_profile,
        limits,
        fallback,
    )?;
    Ok(SceneResult {
        session_id: session_id.to_string(),
        sequence,
        diagnostics: scene.diagnostics.clone(),
        scene,
    })
}

fn success(request_id: String, result: RuntimeResult) -> RuntimeResponse {
    let diagnostics = match &result {
        RuntimeResult::Scene(scene) => scene.diagnostics.clone(),
        _ => Vec::new(),
    };
    RuntimeResponse {
        protocol_version: PROTOCOL_VERSION,
        request_id,
        ok: true,
        result: Some(result),
        error: None,
        diagnostics,
    }
}

fn failure(request_id: String, code: &str, message: String) -> RuntimeResponse {
    RuntimeResponse {
        protocol_version: PROTOCOL_VERSION,
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

fn failure_from_string(request_id: String, error: String) -> RuntimeResponse {
    let code = error
        .split(|character: char| character == ':' || character.is_whitespace())
        .find(|part| part.starts_with("RUNTIME_"))
        .unwrap_or("RUNTIME_EXECUTION_FAILED")
        .to_string();
    failure(request_id, &code, error)
}
