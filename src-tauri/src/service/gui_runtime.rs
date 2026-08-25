//! 996 GUI Runtime 的可信代理与项目静态配置快照。
//!
//! Lua worker 永远不获得项目路径或文件权限。主进程只把客户端 DEV 中经过
//! 校验的虚拟模块，以及固定白名单 XLS 的脱敏快照传给独立 sidecar。

use crate::service::project::ProjectService;
use mir3_domain::Mir3Project;
use mir3_ui::{
    parse_document, DiagnosticSeverity as UiDiagnosticSeverity, Mir3UiAsset, Mir3UiDiagnostic,
    Mir3UiDocument, Mir3UiSource,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const RUNTIME_PROTOCOL_VERSION: u32 = 2;
const MAX_RUNTIME_INPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_RUNTIME_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
const MAX_RUNTIME_MODULES: usize = 512;
const MAX_RUNTIME_SESSIONS: usize = 4;
const MAX_RUNTIME_MODULE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_RUNTIME_CATALOG_BYTES: usize = 16 * 1024 * 1024;
const MAX_RUNTIME_DIRECTORY_DEPTH: usize = 32;
const MAX_RUNTIME_PREFERENCES_BYTES: usize = 1024 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SNAPSHOT_XLS_BYTES: usize = 20 * 1024 * 1024;
const MAX_SNAPSHOT_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_SNAPSHOT_TABLES: usize = 8;
const MAX_SNAPSHOT_ROWS: usize = 20_000;
const MAX_SNAPSHOT_CELLS: usize = 200_000;
const MAX_SNAPSHOT_CELL_BYTES: usize = 64 * 1024;

const CONFIG_TABLES: &[&str] = &[
    "cfg_game_data",
    "cfg_colour_style",
    "cfg_colour_style_win32",
    "cfg_hotkey",
    "cfg_setup",
    "cfg_item",
    "cfg_equip",
    "cfg_show_equip",
    "cfg_suit",
    "cfg_suitex",
    "cfg_magic",
    "cfg_magicinfo",
    "cfg_skill_present",
    "cfg_buff",
    "cfg_auction_type",
    "cfg_sell_type",
    "cfg_store",
    "cfg_customjob",
    "cfg_loginAnim",
    "cfg_model_info",
    "cfg_mapinfo",
    "cfg_mapName",
    "cfg_npclist",
];

const GAME_DATA_KEYS: &[&str] = &[
    "AddItemTipsPos",
    "BackpackGuide",
    "ChangeEquipPageType",
    "CustomTipsBg",
    "EquipItemScale",
    "ExchangeEquipPageTime",
    "ExpTipsType",
    "Fashionfx",
    "GangColorList",
    "HideItemShortcutHotKey",
    "PcItemShourtcutNotMove",
    "ShowTipsNeiGuan",
    "SuitCalType",
    "TipsLowHeight",
    "TipsShowIcon",
    "YSShowType",
    "announce",
    "attShowTips",
    "attrShowInitValue",
    "bagConfig",
    "bindTypeStr",
    "buffShowConfig",
    "clickPressTime",
    "color_mask_opcity",
    "comboSkills",
    "cytChatTopOff",
    "equipLayerMod",
    "equipPageNum",
    "equipShowTips",
    "equipTipsSpName",
    "forceQuickBtn",
    "gamePadScale",
    "gamePadVisible",
    "hideMonsterAttrs",
    "itemSacle",
    "loadingBarExpScale",
    "niceboatTimeFormat",
    "noDigMonsters",
    "recruitment",
    "replaceBasicID",
    "skillSetMode",
    "skillTipsXY",
    "staticSacle",
    "suitTipColor",
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeDataSource {
    #[default]
    BuiltInMock,
    ProjectStatic,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeTableCapability {
    pub name: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCapabilities {
    pub available: bool,
    pub backend: String,
    pub data_source: RuntimeDataSource,
    pub project_static_available: bool,
    pub tables: Vec<RuntimeTableCapability>,
    pub limits: Value,
    pub diagnostics: Vec<RuntimeDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDiagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSceneEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub layout_path: String,
    pub platform: String,
    pub compatibility: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePresetEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub layout_path: String,
    pub platform: String,
    pub compatibility: String,
    pub default_map_id: Option<String>,
    pub overlay_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeWorldProfile {
    pub id: String,
    pub name: String,
    pub device: String,
    pub map_id: String,
    pub mock_profile_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCatalog {
    pub presets: Vec<RuntimePresetEntry>,
    pub modules: Vec<RuntimeSceneEntry>,
    pub world_profiles: Vec<RuntimeWorldProfile>,
    /// 兼容 V0.3 前端，内容仅包含四个组合场景。
    pub scenes: Vec<RuntimeSceneEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSceneStartRequest {
    #[serde(default)]
    pub scene_id: String,
    #[serde(default)]
    pub preset_id: Option<String>,
    #[serde(default)]
    pub module_id: Option<String>,
    #[serde(default)]
    pub map_id: Option<String>,
    #[serde(default)]
    pub mock_profile_id: Option<String>,
    pub device: String,
    pub viewport: RuntimeViewport,
    #[serde(default)]
    pub working_sources: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeViewport {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSceneResponse {
    pub session_id: String,
    pub sequence: u64,
    pub scene: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Value>,
    pub fallback: bool,
    pub diagnostics: Vec<RuntimeDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStopResponse {
    pub stopped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RuntimePreferences {
    projects: BTreeMap<String, RuntimeDataSource>,
}

#[derive(Clone)]
struct RuntimeSession {
    project_id: String,
    request: RuntimeSceneStartRequest,
    sequence: u64,
    worker_session_id: String,
    worker: Arc<Mutex<WorkerProcess>>,
    source_bindings: RuntimeSourceBindingIndex,
}

#[derive(Clone)]
struct RuntimeSourceBinding {
    template_node_id: String,
    line: usize,
    column: usize,
}

type RuntimeSourceBindingIndex = HashMap<(String, String), RuntimeSourceBinding>;

struct WorkerProcess {
    child: Child,
    stdin: ChildStdin,
    responses: Receiver<Result<Vec<u8>, String>>,
    working_directory: PathBuf,
}

#[derive(Clone)]
pub struct GuiRuntimeService {
    preferences_path: PathBuf,
    preferences: Arc<Mutex<RuntimePreferences>>,
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<RuntimeSession>>>>>,
    starting_projects: Arc<Mutex<HashMap<String, String>>>,
}

struct RuntimeStartReservation {
    service: GuiRuntimeService,
    project_id: String,
    token: String,
}

impl Drop for RuntimeStartReservation {
    fn drop(&mut self) {
        self.service
            .release_start_reservation(&self.project_id, &self.token);
    }
}

impl GuiRuntimeService {
    pub fn new(data_root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&data_root).map_err(|error| {
            format!(
                "GUI_RUNTIME_DATA_DIR_FAILED: {}: {error}",
                data_root.display()
            )
        })?;
        let preferences_path = data_root.join("runtime-preferences.json");
        let preferences = if preferences_path.is_file() {
            let bytes = read_bounded_file(
                &preferences_path,
                MAX_RUNTIME_PREFERENCES_BYTES,
                "GUI_RUNTIME_PREFERENCES_READ_FAILED",
                "GUI_RUNTIME_PREFERENCES_SIZE_LIMIT",
            )?;
            serde_json::from_slice(&bytes).unwrap_or_default()
        } else {
            RuntimePreferences::default()
        };
        Ok(Self {
            preferences_path,
            preferences: Arc::new(Mutex::new(preferences)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            starting_projects: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn data_source(&self, project_id: &str) -> RuntimeDataSource {
        self.preferences
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .projects
            .get(project_id)
            .copied()
            .unwrap_or_default()
    }

    pub fn set_data_source(
        &self,
        project_id: &str,
        data_source: RuntimeDataSource,
    ) -> Result<(), String> {
        let mut preferences = self
            .preferences
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut next = preferences.clone();
        next.projects.insert(project_id.to_string(), data_source);
        let bytes = serde_json::to_vec_pretty(&next)
            .map_err(|error| format!("GUI_RUNTIME_PREFERENCES_ENCODE_FAILED: {error}"))?;
        if bytes.len() > MAX_RUNTIME_PREFERENCES_BYTES {
            return Err(
                "GUI_RUNTIME_PREFERENCES_SIZE_LIMIT: runtime preferences are too large".to_string(),
            );
        }
        replace_file_safely(&self.preferences_path, &bytes).map_err(|error| {
            format!(
                "GUI_RUNTIME_PREFERENCES_REPLACE_FAILED: {}: {error}",
                self.preferences_path.display()
            )
        })?;
        *preferences = next;
        Ok(())
    }

    fn reserve_session_start(&self, project_id: &str) -> Result<RuntimeStartReservation, String> {
        let sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut starting_projects = self
            .starting_projects
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if sessions.len().saturating_add(starting_projects.len()) >= MAX_RUNTIME_SESSIONS {
            return Err("GUI_RUNTIME_SESSION_LIMIT: too many active runtime sessions".to_string());
        }
        if starting_projects.contains_key(project_id)
            || sessions.values().any(|candidate| {
                candidate
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .project_id
                    == project_id
            })
        {
            return Err(
                "GUI_RUNTIME_PROJECT_SESSION_EXISTS: stop the current project scene first"
                    .to_string(),
            );
        }
        let token = runtime_id("runtime-reservation");
        starting_projects.insert(project_id.to_string(), token.clone());
        Ok(RuntimeStartReservation {
            service: self.clone(),
            project_id: project_id.to_string(),
            token,
        })
    }

    fn release_start_reservation(&self, project_id: &str, token: &str) {
        let mut starting_projects = self
            .starting_projects
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if starting_projects
            .get(project_id)
            .is_some_and(|current| current == token)
        {
            starting_projects.remove(project_id);
        }
    }

    fn cancel_start_reservation(&self, project_id: &str) {
        self.starting_projects
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(project_id);
    }

    fn insert_reserved_session(
        &self,
        reservation: &RuntimeStartReservation,
        session: RuntimeSession,
    ) -> Result<String, String> {
        let session_id = runtime_id("runtime-session");
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut starting_projects = self
            .starting_projects
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if session.project_id != reservation.project_id
            || starting_projects.get(&session.project_id) != Some(&reservation.token)
        {
            return Err(
                "GUI_RUNTIME_START_RESERVATION_MISSING: runtime start slot expired".to_string(),
            );
        }
        if sessions.len().saturating_add(starting_projects.len()) > MAX_RUNTIME_SESSIONS {
            return Err("GUI_RUNTIME_SESSION_LIMIT: too many active runtime sessions".to_string());
        }
        if sessions.values().any(|candidate| {
            candidate
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .project_id
                == session.project_id
        }) {
            return Err(
                "GUI_RUNTIME_PROJECT_SESSION_EXISTS: stop the current project scene first"
                    .to_string(),
            );
        }
        starting_projects.remove(&session.project_id);
        sessions.insert(session_id.clone(), Arc::new(Mutex::new(session)));
        Ok(session_id)
    }

    fn session(&self, session_id: &str) -> Result<Arc<Mutex<RuntimeSession>>, String> {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(session_id)
            .cloned()
            .ok_or_else(|| "GUI_RUNTIME_SESSION_NOT_FOUND: runtime session is missing".to_string())
    }

    pub fn stop_session(&self, project_id: &str, session_id: &str) -> Result<(), String> {
        let removed = {
            let mut sessions = self
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let session = sessions.get(session_id).ok_or_else(|| {
                "GUI_RUNTIME_SESSION_NOT_FOUND: runtime session is missing".to_string()
            })?;
            let session_project = session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .project_id
                .clone();
            if session_project != project_id {
                return Err(
                    "GUI_RUNTIME_PROJECT_MISMATCH: runtime session belongs to another project"
                        .to_string(),
                );
            }
            sessions.remove(session_id).expect("会话已在同一把锁内验证")
        };
        let removed = removed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut worker = removed
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = worker.transact(
            "stop",
            json!({ "sessionId": removed.worker_session_id }),
            Duration::from_millis(500),
        );
        worker.terminate();
        Ok(())
    }

    pub fn stop_project_sessions(&self, project_id: &str) {
        let sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.cancel_start_reservation(project_id);
        let session_ids = sessions
            .iter()
            .filter_map(|(session_id, session)| {
                let belongs_to_project = session
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .project_id
                    == project_id;
                belongs_to_project.then(|| session_id.clone())
            })
            .collect::<Vec<_>>();
        drop(sessions);
        for session_id in session_ids {
            let _ = self.stop_session(project_id, &session_id);
        }
    }

    fn invalidate_session(&self, session_id: &str) {
        let removed = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(session_id);
        if let Some(removed) = removed {
            let session = removed
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            session
                .worker
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .terminate();
        }
    }
}

impl WorkerProcess {
    fn spawn(app: &AppHandle) -> Result<Self, String> {
        let binary = runtime_binary_path(app).ok_or_else(|| {
            "GUI_RUNTIME_BINARY_MISSING: runtime sidecar is unavailable".to_string()
        })?;
        let working_directory = std::env::temp_dir().join(runtime_id("mir3-gui-runtime"));
        fs::create_dir_all(&working_directory).map_err(|error| {
            format!(
                "GUI_RUNTIME_TEMP_DIR_FAILED: {}: {error}",
                working_directory.display()
            )
        })?;
        let mut command = Command::new(binary);
        command
            .current_dir(&working_directory)
            .env_clear()
            .env("LANG", "C.UTF-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            // Linux 支持对 sidecar 设置地址空间上限；macOS 对 RLIMIT_AS 返回 EINVAL。
            unsafe {
                command.pre_exec(|| {
                    let limit = libc::rlimit {
                        rlim_cur: 256 * 1024 * 1024,
                        rlim_max: 256 * 1024 * 1024,
                    };
                    if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("GUI_RUNTIME_SPAWN_FAILED: {error}"))?;
        let Some(stdin) = child.stdin.take() else {
            cleanup_failed_worker(&mut child, &working_directory);
            return Err("GUI_RUNTIME_STDIN_MISSING: sidecar stdin is unavailable".to_string());
        };
        let Some(stdout) = child.stdout.take() else {
            cleanup_failed_worker(&mut child, &working_directory);
            return Err("GUI_RUNTIME_STDOUT_MISSING: sidecar stdout is unavailable".to_string());
        };
        let Some(stderr) = child.stderr.take() else {
            cleanup_failed_worker(&mut child, &working_directory);
            return Err("GUI_RUNTIME_STDERR_MISSING: sidecar stderr is unavailable".to_string());
        };
        let (sender, responses) = mpsc::channel();
        std::thread::spawn(move || read_runtime_responses(stdout, sender));
        std::thread::spawn(move || {
            let mut stderr = stderr;
            let _ = io::copy(&mut stderr, &mut io::sink());
        });
        Ok(Self {
            child,
            stdin,
            responses,
            working_directory,
        })
    }

    fn transact(
        &mut self,
        request_type: &str,
        payload: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let request_id = runtime_id("runtime-request");
        let envelope = json!({
            "protocolVersion": RUNTIME_PROTOCOL_VERSION,
            "type": request_type,
            "requestId": request_id,
            "payload": payload,
        });
        let mut bytes = serde_json::to_vec(&envelope)
            .map_err(|error| format!("GUI_RUNTIME_REQUEST_ENCODE_FAILED: {error}"))?;
        bytes.push(b'\n');
        if bytes.len() > MAX_RUNTIME_INPUT_BYTES {
            return Err("GUI_RUNTIME_INPUT_LIMIT: runtime request is too large".to_string());
        }
        self.stdin
            .write_all(&bytes)
            .and_then(|_| self.stdin.flush())
            .map_err(|error| format!("GUI_RUNTIME_STDIN_WRITE_FAILED: {error}"))?;
        let output = match self.responses.recv_timeout(timeout) {
            Ok(output) => output?,
            Err(error) => {
                self.terminate();
                return Err(match error {
                    mpsc::RecvTimeoutError::Timeout => {
                        "GUI_RUNTIME_TIMEOUT: scene execution exceeded its budget".to_string()
                    }
                    mpsc::RecvTimeoutError::Disconnected => {
                        "GUI_RUNTIME_PROCESS_FAILED: sidecar closed its output".to_string()
                    }
                });
            }
        };
        let line = String::from_utf8(output)
            .map_err(|_| "GUI_RUNTIME_OUTPUT_ENCODING: sidecar returned non UTF-8".to_string())?;
        let response: Value = serde_json::from_str(line.trim())
            .map_err(|error| format!("GUI_RUNTIME_RESPONSE_INVALID: {error}"))?;
        if response.get("protocolVersion").and_then(Value::as_u64)
            != Some(RUNTIME_PROTOCOL_VERSION.into())
        {
            return Err(
                "GUI_RUNTIME_PROTOCOL_MISMATCH: sidecar protocol version does not match"
                    .to_string(),
            );
        }
        if response.get("requestId").and_then(Value::as_str) != Some(request_id.as_str()) {
            return Err("GUI_RUNTIME_RESPONSE_MISMATCH: request id does not match".to_string());
        }
        if response.get("ok").and_then(Value::as_bool) != Some(true) {
            let error = response.get("error");
            let code = error
                .and_then(|value| value.get("code"))
                .and_then(Value::as_str)
                .unwrap_or("GUI_RUNTIME_EXECUTION_FAILED");
            let message = error
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
                .or_else(|| error.and_then(Value::as_str))
                .unwrap_or("unknown runtime failure");
            return Err(format!("{code}: {message}"));
        }
        response
            .get("result")
            .cloned()
            .ok_or_else(|| "GUI_RUNTIME_RESPONSE_INVALID: result is missing".to_string())
    }

    fn terminate(&mut self) {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("taskkill")
                .args(["/PID", &self.child.id().to_string(), "/T", "/F"])
                .creation_flags(0x08000000)
                .status();
        }
        #[cfg(not(windows))]
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.working_directory);
    }
}

fn cleanup_failed_worker(child: &mut Child, working_directory: &Path) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_dir_all(working_directory);
}

fn read_runtime_responses(mut stdout: impl Read, sender: mpsc::Sender<Result<Vec<u8>, String>>) {
    let mut pending = Vec::new();
    let mut scanned = 0usize;
    let mut buffer = [0u8; 8192];
    loop {
        match stdout.read(&mut buffer) {
            Ok(0) => break,
            Ok(length) => {
                pending.extend_from_slice(&buffer[..length]);
                if pending.len() > MAX_RUNTIME_OUTPUT_BYTES {
                    let _ = sender.send(Err(
                        "GUI_RUNTIME_OUTPUT_LIMIT: sidecar response is too large".to_string(),
                    ));
                    break;
                }
                while let Some(relative_index) =
                    pending[scanned..].iter().position(|byte| *byte == b'\n')
                {
                    let index = scanned + relative_index;
                    let line = pending.drain(..=index).collect::<Vec<_>>();
                    if sender.send(Ok(line)).is_err() {
                        return;
                    }
                    scanned = 0;
                }
                scanned = pending.len();
            }
            Err(error) => {
                let _ = sender.send(Err(format!("GUI_RUNTIME_STDOUT_READ_FAILED: {error}")));
                break;
            }
        }
    }
}

impl Drop for WorkerProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

pub fn capabilities(
    app: &AppHandle,
    project_service: &ProjectService,
    runtime_service: &GuiRuntimeService,
    project_id: &str,
) -> Result<RuntimeCapabilities, String> {
    let project = ensure_active_project(project_service, project_id)?;
    let table_root = config_table_root(&project).ok();
    let tables = CONFIG_TABLES
        .iter()
        .map(|name| RuntimeTableCapability {
            name: (*name).to_string(),
            available: table_root
                .as_ref()
                .is_some_and(|root| root.join(format!("{name}.xls")).is_file()),
        })
        .collect::<Vec<_>>();
    let binary_available = runtime_binary_path(app).is_some();
    let project_static_available = tables.iter().any(|table| table.available);
    let mut diagnostics = Vec::new();
    if !binary_available {
        diagnostics.push(RuntimeDiagnostic {
            code: "GUI_RUNTIME_BINARY_MISSING".to_string(),
            severity: "warning".to_string(),
            message: "996 GUI Runtime sidecar 尚未构建，将使用静态预览".to_string(),
        });
    }
    if table_root.is_none() {
        diagnostics.push(RuntimeDiagnostic {
            code: "GUI_RUNTIME_STATIC_CONFIG_MISSING".to_string(),
            severity: "info".to_string(),
            message: "项目未提供可用的 Mir200/Envir/Data 静态配置目录".to_string(),
        });
    }
    Ok(RuntimeCapabilities {
        available: binary_available,
        backend: if binary_available {
            "sidecar"
        } else {
            "unavailable"
        }
        .to_string(),
        data_source: runtime_service.data_source(project_id),
        project_static_available,
        tables,
        limits: json!({
            "inputBytes": MAX_RUNTIME_INPUT_BYTES,
            "outputBytes": MAX_RUNTIME_OUTPUT_BYTES,
            "nodes": 10_000,
            "modules": MAX_RUNTIME_MODULES,
            "snapshotBytes": MAX_SNAPSHOT_BYTES,
        }),
        diagnostics,
    })
}

pub fn catalog(
    project_service: &ProjectService,
    project_id: &str,
) -> Result<RuntimeCatalog, String> {
    let project = ensure_active_project(project_service, project_id)?;
    let dev_root = canonical_dev_root(&project)?;
    let layout_root = dev_root.join("GUILayout");
    let mut files = Vec::new();
    collect_lua_files(&layout_root, &mut files)?;
    let mut total_bytes = 0usize;
    let mut modules = Vec::new();
    for path in files {
        if let Some(entry) = scene_entry(&layout_root, &path, &mut total_bytes)? {
            modules.push(entry);
        }
    }
    modules.sort_by(|left, right| left.layout_path.cmp(&right.layout_path));
    let presets = runtime_presets();
    let scenes = presets
        .iter()
        .map(|preset| RuntimeSceneEntry {
            id: preset.id.clone(),
            name: preset.name.clone(),
            category: preset.category.clone(),
            layout_path: preset.layout_path.clone(),
            platform: preset.platform.clone(),
            compatibility: preset.compatibility.clone(),
        })
        .collect();
    Ok(RuntimeCatalog {
        presets,
        modules,
        world_profiles: runtime_world_profiles(),
        scenes,
    })
}

fn runtime_presets() -> Vec<RuntimePresetEntry> {
    vec![
        RuntimePresetEntry {
            id: "character-create".to_string(),
            name: "人物创建".to_string(),
            category: "login".to_string(),
            layout_path: "GUILayout/login/LoginRolePanel.lua".to_string(),
            platform: "shared".to_string(),
            compatibility: "approximate".to_string(),
            default_map_id: None,
            overlay_ids: Vec::new(),
        },
        RuntimePresetEntry {
            id: "character-select".to_string(),
            name: "人物选择".to_string(),
            category: "login".to_string(),
            layout_path: "GUILayout/login/LoginRolePanel.lua".to_string(),
            platform: "shared".to_string(),
            compatibility: "approximate".to_string(),
            default_map_id: None,
            overlay_ids: Vec::new(),
        },
        RuntimePresetEntry {
            id: "game-mobile".to_string(),
            name: "移动端游戏".to_string(),
            category: "game".to_string(),
            layout_path: "GUILayout/GUIInit.lua".to_string(),
            platform: "mobile".to_string(),
            compatibility: "approximate".to_string(),
            default_map_id: Some("01".to_string()),
            overlay_ids: vec!["bag".to_string(), "team".to_string(), "store".to_string()],
        },
        RuntimePresetEntry {
            id: "game-pc".to_string(),
            name: "PC 端游戏".to_string(),
            category: "game".to_string(),
            layout_path: "GUILayout/GUIInit.lua".to_string(),
            platform: "pc".to_string(),
            compatibility: "approximate".to_string(),
            default_map_id: Some("1".to_string()),
            overlay_ids: vec!["bag".to_string(), "team".to_string(), "store".to_string()],
        },
    ]
}

fn runtime_world_profiles() -> Vec<RuntimeWorldProfile> {
    [
        (
            "mobile-01",
            "移动端 · 边境城市",
            "mobile",
            "01",
            "mobile-hud",
        ),
        ("pc-1", "PC · 道馆", "pc", "1", "pc-hud"),
        ("world-d021", "世界 · d021", "shared", "d021", "world"),
        ("world-d032", "世界 · d032", "shared", "d032", "world"),
    ]
    .into_iter()
    .map(
        |(id, name, device, map_id, mock_profile_id)| RuntimeWorldProfile {
            id: id.to_string(),
            name: name.to_string(),
            device: device.to_string(),
            map_id: map_id.to_string(),
            mock_profile_id: mock_profile_id.to_string(),
        },
    )
    .collect()
}

fn requested_preset_id(request: &RuntimeSceneStartRequest) -> &str {
    request
        .preset_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(&request.scene_id)
}

fn runtime_entry_for_preset(preset: &RuntimePresetEntry) -> RuntimeSceneEntry {
    RuntimeSceneEntry {
        id: preset.id.clone(),
        name: preset.name.clone(),
        category: preset.category.clone(),
        layout_path: preset.layout_path.clone(),
        platform: preset.platform.clone(),
        compatibility: preset.compatibility.clone(),
    }
}

pub fn start_scene(
    app: &AppHandle,
    project_service: &ProjectService,
    runtime_service: &GuiRuntimeService,
    project_id: &str,
    request: RuntimeSceneStartRequest,
) -> Result<RuntimeSceneResponse, String> {
    let project = ensure_active_project(project_service, project_id)?;
    let reservation = runtime_service.reserve_session_start(project_id)?;
    let catalog = catalog(project_service, project_id)?;
    let preset_id = requested_preset_id(&request);
    let preset = catalog
        .presets
        .iter()
        .find(|entry| entry.id == preset_id)
        .cloned();
    let entry = if let Some(preset) = &preset {
        runtime_entry_for_preset(preset)
    } else {
        catalog
            .modules
            .iter()
            .find(|entry| entry.id == preset_id)
            .cloned()
            .ok_or_else(|| {
                "GUI_RUNTIME_SCENE_NOT_FOUND: preset or module is not in runtime catalog"
                    .to_string()
            })?
    };
    let modules = collect_runtime_modules(&project, &request.working_sources)?;
    let source_bindings = runtime_source_binding_index(&modules);
    let data_profile = build_data_profile(project_service, runtime_service, &project, &entry)?;
    let payload = json!({
        "sceneId": entry.id,
        "presetId": preset.as_ref().map(|value| value.id.as_str()),
        "layoutPath": entry.layout_path,
        "device": request.device,
        "viewport": request.viewport,
        "moduleId": request.module_id,
        "mapId": request.map_id.as_ref().or(preset.as_ref().and_then(|value| value.default_map_id.as_ref())),
        "mockProfileId": request.mock_profile_id,
        "overlayIds": [],
        "availableOverlayIds": preset.as_ref().map(|value| value.overlay_ids.as_slice()).unwrap_or_default(),
        "modules": modules,
        "dataProfile": data_profile,
    });
    let mut worker = match WorkerProcess::spawn(app) {
        Ok(worker) => worker,
        Err(error) => {
            ensure_same_active_project(project_service, &project)?;
            return static_fallback_response(&request, &entry, &modules, &error);
        }
    };
    let result = match worker.transact("start", payload, Duration::from_secs(5)) {
        Ok(result) => result,
        Err(error) => {
            ensure_same_active_project(project_service, &project)?;
            return static_fallback_response(&request, &entry, &modules, &error);
        }
    };
    let sequence = result
        .get("sequence")
        .and_then(Value::as_u64)
        .ok_or_else(|| "GUI_RUNTIME_RESPONSE_INVALID: start sequence is missing".to_string())?;
    let mut scene = result
        .get("scene")
        .cloned()
        .ok_or_else(|| "GUI_RUNTIME_RESPONSE_INVALID: start scene is missing".to_string())?;
    enrich_runtime_scene(&mut scene, &request, &result);
    enrich_runtime_source_bindings(&mut scene, &source_bindings);
    let diagnostics = diagnostics_from_value(result.get("diagnostics"));
    let worker_session_id = result
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "GUI_RUNTIME_SESSION_INVALID: sidecar did not return a session id".to_string()
        })?
        .to_string();
    let session = RuntimeSession {
        project_id: project_id.to_string(),
        request,
        sequence,
        worker_session_id,
        worker: Arc::new(Mutex::new(worker)),
        source_bindings,
    };
    ensure_same_active_project(project_service, &project)?;
    let session_id = runtime_service.insert_reserved_session(&reservation, session)?;
    if let Err(error) = ensure_same_active_project(project_service, &project) {
        let _ = runtime_service.stop_session(project_id, &session_id);
        return Err(error);
    }
    Ok(RuntimeSceneResponse {
        session_id,
        sequence,
        scene,
        patch: result.get("patch").cloned(),
        fallback: false,
        diagnostics,
    })
}

fn static_fallback_response(
    request: &RuntimeSceneStartRequest,
    entry: &RuntimeSceneEntry,
    modules: &BTreeMap<String, String>,
    runtime_error: &str,
) -> Result<RuntimeSceneResponse, String> {
    let scene = compose_static_scene(request, entry, modules)?;
    let mut scene = serde_json::to_value(scene)
        .map_err(|error| format!("GUI_RUNTIME_FALLBACK_ENCODE_FAILED: {error}"))?;
    enrich_runtime_scene(&mut scene, request, &Value::Null);
    Ok(RuntimeSceneResponse {
        session_id: String::new(),
        sequence: 0,
        scene,
        patch: None,
        fallback: true,
        diagnostics: vec![RuntimeDiagnostic {
            code: "GUI_RUNTIME_STATIC_FALLBACK".to_string(),
            severity: "warning".to_string(),
            message: format!("Runtime 场景不可用，已使用静态组合预览：{runtime_error}"),
        }],
    })
}

fn compose_static_scene(
    request: &RuntimeSceneStartRequest,
    entry: &RuntimeSceneEntry,
    modules: &BTreeMap<String, String>,
) -> Result<Mir3UiDocument, String> {
    let layout_source = modules.get(&entry.layout_path).ok_or_else(|| {
        "GUI_RUNTIME_FALLBACK_ENTRY_MISSING: GUILayout source is missing".to_string()
    })?;
    let export_pattern =
        Regex::new(r#"GUI\s*:\s*LoadExport\s*\([^,\n]+,\s*[\"']([^\"']+)[\"']"#)
            .map_err(|error| format!("GUI_RUNTIME_FALLBACK_PATTERN_FAILED: {error}"))?;
    let mut layout_paths = vec![entry.layout_path.clone()];
    layout_paths.extend(static_layout_paths_for_preset(&entry.id));
    let mut paths = layout_paths
        .iter()
        .filter_map(|path| modules.get(path))
        .flat_map(|source| export_pattern.captures_iter(source))
        .filter_map(|capture| capture.get(1))
        .map(|capture| export_module_path(capture.as_str()))
        .filter(|path| modules.contains_key(path))
        .collect::<Vec<_>>();
    paths.extend(
        static_export_paths_for_preset(&entry.id)
            .into_iter()
            .filter(|path| modules.contains_key(path)),
    );
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        paths.push(entry.layout_path.clone());
    }

    let mut documents = Vec::new();
    for path in paths {
        let Some(source) = modules.get(&path) else {
            continue;
        };
        let sha = hex_sha256(source.as_bytes());
        if let Ok(document) = parse_document(source, &path, &sha, "utf-8", "\n") {
            documents.push(document);
        }
    }
    if documents.is_empty() {
        return Err(
            "GUI_RUNTIME_FALLBACK_EMPTY: no static GUI document could be composed".to_string(),
        );
    }
    let mut scene = documents.remove(0);
    scene.source = Mir3UiSource {
        dev_relative_path: entry.layout_path.clone(),
        sha256: hex_sha256(layout_source.as_bytes()),
        encoding: "utf-8".to_string(),
        newline: "\n".to_string(),
        byte_length: layout_source.len(),
    };
    scene.viewport.width = request.viewport.width;
    scene.viewport.height = request.viewport.height;
    for (index, document) in documents.into_iter().enumerate() {
        merge_static_document(&mut scene, document, index + 1);
    }
    scene.diagnostics.push(Mir3UiDiagnostic {
        severity: UiDiagnosticSeverity::Warning,
        code: "GUI_RUNTIME_STATIC_FALLBACK".to_string(),
        message: "Runtime 未能完成场景执行，当前显示安全静态组合预览".to_string(),
        span: None,
        node_id: None,
    });
    Ok(scene)
}

fn export_module_path(value: &str) -> String {
    let value = value.trim_start_matches('/');
    let value = value.strip_suffix(".lua").unwrap_or(value);
    format!("GUIExport/{value}.lua")
}

fn static_layout_paths_for_preset(preset_id: &str) -> Vec<String> {
    let paths: &[&str] = match preset_id {
        "game-mobile" => &[
            "GUILayout/main/MainProperty.lua",
            "GUILayout/main/MainAvartar.lua",
            "GUILayout/main/MainMiniMap.lua",
            "GUILayout/main/MainAssist.lua",
            "GUILayout/main/MainTarget.lua",
            "GUILayout/main/MainMonster.lua",
            "GUILayout/be_strong/BeStrongUp.lua",
            "GUILayout/main/MainJoyStick.lua",
            "GUILayout/main/MainWidgets.lua",
            "GUILayout/main/MainCollect.lua",
            "GUILayout/MainSkill.lua",
        ],
        "game-pc" => &[
            "GUILayout/main/MainMiniMap.lua",
            "GUILayout/main/MainBuff_win32.lua",
            "GUILayout/main/MainPKMode_win32.lua",
            "GUILayout/main/MainSkillShortcut_win32.lua",
            "GUILayout/main/MainItemShortcut_win32.lua",
            "GUILayout/main/MainSkillLaunch_win32.lua",
            "GUILayout/main/MainProperty_win32.lua",
            "GUILayout/main/MainChat_win32.lua",
            "GUILayout/main/MainAssist_win32.lua",
            "GUILayout/main/MainTarget.lua",
            "GUILayout/main/MainMonster.lua",
            "GUILayout/be_strong/BeStrongUp.lua",
            "GUILayout/main/MainWidgets_win32.lua",
            "GUILayout/main/MainCollect.lua",
        ],
        _ => &[],
    };
    paths.iter().map(|path| (*path).to_string()).collect()
}

fn static_export_paths_for_preset(preset_id: &str) -> Vec<String> {
    let paths: &[&str] = match preset_id {
        "character-create" => &["GUIExport/login_role/login_role_create.lua"],
        "character-select" => &["GUIExport/login_role/login_role.lua"],
        _ => &[],
    };
    paths.iter().map(|path| (*path).to_string()).collect()
}

fn merge_static_document(target: &mut Mir3UiDocument, mut source: Mir3UiDocument, index: usize) {
    let id_map = source
        .nodes
        .iter()
        .map(|node| (node.id.clone(), format!("fallback-{index}-{}", node.id)))
        .collect::<HashMap<_, _>>();
    for node in &mut source.nodes {
        node.id = id_map
            .get(&node.id)
            .cloned()
            .unwrap_or_else(|| node.id.clone());
        node.parent_id = node
            .parent_id
            .as_ref()
            .and_then(|parent| id_map.get(parent))
            .cloned();
        node.children = node
            .children
            .iter()
            .filter_map(|child| id_map.get(child).cloned())
            .collect();
    }
    target.roots.extend(
        source
            .roots
            .iter()
            .filter_map(|root| id_map.get(root).cloned()),
    );
    target.nodes.extend(source.nodes);
    for asset in source.assets {
        merge_static_asset(&mut target.assets, asset, &id_map);
    }
    for mut diagnostic in source.diagnostics {
        diagnostic.node_id = diagnostic
            .node_id
            .as_ref()
            .and_then(|node_id| id_map.get(node_id))
            .cloned();
        target.diagnostics.push(diagnostic);
    }
}

fn merge_static_asset(
    target: &mut Vec<Mir3UiAsset>,
    mut asset: Mir3UiAsset,
    id_map: &HashMap<String, String>,
) {
    asset.node_ids = asset
        .node_ids
        .iter()
        .filter_map(|node_id| id_map.get(node_id).cloned())
        .collect();
    if let Some(existing) = target
        .iter_mut()
        .find(|existing| existing.logical_path == asset.logical_path)
    {
        existing.node_ids.extend(asset.node_ids);
    } else {
        target.push(asset);
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_bounded_file(
    path: &Path,
    max_bytes: usize,
    read_code: &str,
    size_code: &str,
) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("{read_code}: {}: {error}", path.display()))?;
    let mut bytes = Vec::with_capacity(max_bytes.min(64 * 1024));
    file.take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{read_code}: {}: {error}", path.display()))?;
    if bytes.len() > max_bytes {
        return Err(format!(
            "{size_code}: {} exceeds {} bytes",
            path.display(),
            max_bytes
        ));
    }
    Ok(bytes)
}

fn add_to_budget(
    total: &mut usize,
    amount: usize,
    max_bytes: usize,
    message: &str,
) -> Result<(), String> {
    let next = total
        .checked_add(amount)
        .ok_or_else(|| message.to_string())?;
    if next > max_bytes {
        return Err(message.to_string());
    }
    *total = next;
    Ok(())
}

fn replace_file_safely(target: &Path, content: &[u8]) -> io::Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::other("target has no parent directory"))?;
    let file_name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("runtime-preferences.json");
    let nonce = runtime_id("preferences");
    let temporary = parent.join(format!(".{file_name}.{nonce}.tmp"));
    let backup = parent.join(format!(".{file_name}.{nonce}.backup"));

    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    if let Err(error) = output.write_all(content).and_then(|_| output.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    drop(output);

    if !target.exists() {
        return match fs::rename(&temporary, target) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(error)
            }
        };
    }
    if let Err(error) = fs::rename(target, &backup) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    match fs::rename(&temporary, target) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(&backup, target);
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

pub fn scene_event(
    _app: &AppHandle,
    project_service: &ProjectService,
    runtime_service: &GuiRuntimeService,
    project_id: &str,
    session_id: &str,
    node_id: &str,
    event_type: &str,
    payload: Value,
    expected_sequence: u64,
) -> Result<RuntimeSceneResponse, String> {
    ensure_active_project(project_service, project_id)?;
    let session_handle = runtime_service.session(session_id)?;
    let mut session = session_handle
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if session.project_id != project_id {
        return Err(
            "GUI_RUNTIME_PROJECT_MISMATCH: runtime session belongs to another project".to_string(),
        );
    }
    if session.sequence != expected_sequence {
        return Err("GUI_RUNTIME_SEQUENCE_STALE: runtime scene sequence has changed".to_string());
    }
    let result = session
        .worker
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .transact(
            "event",
            json!({
                "sessionId": session.worker_session_id,
                "name": event_type,
                "payload": { "nodeId": node_id, "data": payload },
            }),
            Duration::from_millis(500),
        );
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            drop(session);
            runtime_service.invalidate_session(session_id);
            return Err(error);
        }
    };
    let patch = result.get("patch").cloned();
    let validated =
        validate_runtime_scene_result(&result, "event", expected_sequence.saturating_add(1));
    let (next_sequence, mut scene, diagnostics) = match validated {
        Ok(validated) => validated,
        Err(error) => {
            drop(session);
            runtime_service.invalidate_session(session_id);
            return Err(error);
        }
    };
    enrich_runtime_scene(&mut scene, &session.request, &result);
    enrich_runtime_source_bindings(&mut scene, &session.source_bindings);
    session.sequence = next_sequence;
    Ok(RuntimeSceneResponse {
        session_id: session_id.to_string(),
        sequence: session.sequence,
        scene,
        patch,
        fallback: false,
        diagnostics,
    })
}

pub fn reload_scene(
    _app: &AppHandle,
    project_service: &ProjectService,
    runtime_service: &GuiRuntimeService,
    project_id: &str,
    session_id: &str,
    working_sources: BTreeMap<String, String>,
) -> Result<RuntimeSceneResponse, String> {
    let project = ensure_active_project(project_service, project_id)?;
    let session_handle = runtime_service.session(session_id)?;
    let mut session = session_handle
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if session.project_id != project_id {
        return Err(
            "GUI_RUNTIME_PROJECT_MISMATCH: runtime session belongs to another project".to_string(),
        );
    }
    let mut request = session.request.clone();
    request.working_sources = working_sources;
    let catalog = catalog(project_service, project_id)?;
    let preset_id = requested_preset_id(&request);
    let preset = catalog
        .presets
        .iter()
        .find(|entry| entry.id == preset_id)
        .cloned();
    let entry = if let Some(preset) = &preset {
        runtime_entry_for_preset(preset)
    } else {
        catalog
            .modules
            .iter()
            .find(|entry| entry.id == preset_id)
            .cloned()
            .ok_or_else(|| {
                "GUI_RUNTIME_SCENE_NOT_FOUND: preset or module is not in runtime catalog"
                    .to_string()
            })?
    };
    let modules = collect_runtime_modules(&project, &request.working_sources)?;
    let source_bindings = runtime_source_binding_index(&modules);
    let data_profile = build_data_profile(project_service, runtime_service, &project, &entry)?;
    let result = session
        .worker
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .transact(
            "reload",
            json!({
                "sessionId": session.worker_session_id,
                "layoutPath": entry.layout_path,
                "sceneId": entry.id,
                "presetId": preset.as_ref().map(|value| value.id.as_str()),
                "moduleId": request.module_id,
                "mapId": request.map_id.as_ref().or(preset.as_ref().and_then(|value| value.default_map_id.as_ref())),
                "mockProfileId": request.mock_profile_id,
                "overlayIds": [],
                "availableOverlayIds": preset.as_ref().map(|value| value.overlay_ids.as_slice()).unwrap_or_default(),
                "modules": modules,
                "dataProfile": data_profile,
            }),
            Duration::from_secs(5),
        );
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            drop(session);
            runtime_service.invalidate_session(session_id);
            return Err(error);
        }
    };
    let expected_sequence = session.sequence.saturating_add(1);
    let patch = result.get("patch").cloned();
    let validated = validate_runtime_scene_result(&result, "reload", expected_sequence);
    let (next_sequence, mut scene, diagnostics) = match validated {
        Ok(validated) => validated,
        Err(error) => {
            drop(session);
            runtime_service.invalidate_session(session_id);
            return Err(error);
        }
    };
    enrich_runtime_scene(&mut scene, &request, &result);
    enrich_runtime_source_bindings(&mut scene, &source_bindings);
    session.sequence = next_sequence;
    session.request = request;
    session.source_bindings = source_bindings;
    Ok(RuntimeSceneResponse {
        session_id: session_id.to_string(),
        sequence: session.sequence,
        scene,
        patch,
        fallback: false,
        diagnostics,
    })
}

fn enrich_runtime_scene(
    scene: &mut Value,
    request: &RuntimeSceneStartRequest,
    runtime_result: &Value,
) {
    let Some(scene_object) = scene.as_object_mut() else {
        return;
    };
    let preset_id = requested_preset_id(request);
    let stage = match preset_id {
        "character-create" => json!({
            "kind": "login",
            "backgroundAsset": "private/login/create_bg.jpg",
            "compatibility": "approximate",
        }),
        "character-select" => json!({
            "kind": "login",
            "backgroundAsset": "private/login/bg_cjzy_02.jpg",
            "compatibility": "approximate",
        }),
        "game-pc" => json!({
            "kind": "world",
            "mapId": request.map_id.as_deref().unwrap_or("1"),
            "compatibility": "approximate",
        }),
        _ => json!({
            "kind": "world",
            "mapId": request.map_id.as_deref().unwrap_or("01"),
            "compatibility": "approximate",
        }),
    };
    let roots = scene_object
        .get("roots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let windows = runtime_result
        .get("windowStack")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    let kind = item.get("kind")?.as_str()?;
                    let id = item.get("id").and_then(Value::as_str).unwrap_or(kind);
                    let root_node_ids = roots
                        .iter()
                        .filter(|root| root.as_str().is_some_and(|root_id| root_id.contains(kind)))
                        .cloned()
                        .collect::<Vec<_>>();
                    Some(json!({
                        "id": id,
                        "kind": kind,
                        "rootNodeIds": root_node_ids,
                        "modal": false,
                        "zOrder": 300 + index,
                        "source": "runtime",
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    scene_object.insert(
        "runtime".to_string(),
        json!({
            "profileId": preset_id,
            "device": request.device,
            "stage": stage,
            "layers": [
                { "id": "stage", "rootNodeIds": [], "zOrder": 0 },
                { "id": "world", "rootNodeIds": [], "zOrder": 100 },
                { "id": "hud", "rootNodeIds": roots, "zOrder": 200 },
                { "id": "windows", "rootNodeIds": windows.iter().flat_map(|window| window["rootNodeIds"].as_array().cloned().unwrap_or_default()).collect::<Vec<_>>(), "zOrder": 300 }
            ],
            "windows": windows,
        }),
    );
}

fn build_data_profile(
    project_service: &ProjectService,
    runtime_service: &GuiRuntimeService,
    project: &Mir3Project,
    scene: &RuntimeSceneEntry,
) -> Result<Value, String> {
    let mode = runtime_service.data_source(&project.id);
    let profile_id = match scene.id.as_str() {
        "character-create" => "login-create",
        "character-select" => "login-role",
        "game-mobile" => "mobile-hud",
        "game-pc" => "pc-hud",
        _ => mock_profile_for_scene(&scene.layout_path),
    };
    let mock_values = scene_mock_values(profile_id);
    let mut profile = json!({
        "origin": "builtInMock",
        "profileId": profile_id,
        "virtualClock": 0,
        "tables": {},
        "values": mock_values,
        "metaValues": mock_values,
        "sourceHashes": {},
        "redactions": ["credentials", "network", "playerState"],
    });
    if mode == RuntimeDataSource::BuiltInMock {
        return Ok(profile);
    }
    let table_names = if matches!(scene.id.as_str(), "game-mobile" | "game-pc") {
        vec![
            "cfg_game_data",
            "cfg_item",
            "cfg_equip",
            "cfg_magic",
            "cfg_buff",
            "cfg_mapinfo",
            "cfg_npclist",
            "cfg_colour_style",
        ]
    } else {
        tables_for_scene(&scene.layout_path)
    };
    let snapshot = build_config_snapshot(project_service, project, &table_names)?;
    profile["origin"] = Value::String("projectStatic".to_string());
    profile["tables"] = snapshot["tables"].clone();
    let mut merged_values = scene_mock_values(profile_id)
        .as_object()
        .cloned()
        .unwrap_or_default();
    if let Some(static_values) = snapshot["values"].as_object() {
        merged_values.extend(static_values.clone());
    }
    profile["values"] = Value::Object(merged_values.clone());
    profile["metaValues"] = Value::Object(merged_values);
    profile["sourceHashes"] = snapshot["sourceHashes"].clone();
    Ok(profile)
}

fn build_config_snapshot(
    project_service: &ProjectService,
    project: &Mir3Project,
    table_names: &[&str],
) -> Result<Value, String> {
    if table_names.len() > MAX_SNAPSHOT_TABLES {
        return Err("GUI_RUNTIME_SNAPSHOT_TABLE_LIMIT: too many static tables".to_string());
    }
    let table_root = config_table_root(project)?;
    let mut tables = Map::new();
    let mut values = Map::new();
    let mut source_hashes = Map::new();
    let mut total_rows = 0usize;
    let mut total_cells = 0usize;
    let mut total_source_bytes = 0usize;
    for name in table_names {
        if !CONFIG_TABLES.contains(name) {
            return Err(format!("GUI_RUNTIME_CONFIG_DENIED: {name}"));
        }
        let file = table_root.join(format!("{name}.xls"));
        if !file.is_file() {
            continue;
        }
        let canonical = fs::canonicalize(&file).map_err(|error| {
            format!(
                "GUI_RUNTIME_CONFIG_PATH_FAILED: {}: {error}",
                file.display()
            )
        })?;
        if !canonical.starts_with(&table_root) {
            return Err("GUI_RUNTIME_CONFIG_OUTSIDE: static table escaped Data root".to_string());
        }
        let source = read_bounded_file(
            &canonical,
            MAX_SNAPSHOT_XLS_BYTES,
            "GUI_RUNTIME_CONFIG_READ_FAILED",
            "GUI_RUNTIME_CONFIG_SIZE_LIMIT",
        )?;
        add_to_budget(
            &mut total_source_bytes,
            source.len(),
            MAX_SNAPSHOT_SOURCE_BYTES,
            "GUI_RUNTIME_SNAPSHOT_SOURCE_LIMIT: static XLS sources are too large",
        )?;
        let source_sha256 = hex_sha256(&source);
        let project_root = fs::canonicalize(&project.root)
            .map_err(|error| format!("GUI_RUNTIME_PROJECT_PATH_FAILED: {error}"))?;
        let relative = canonical
            .strip_prefix(&project_root)
            .map_err(|_| "GUI_RUNTIME_CONFIG_OUTSIDE: table escaped project root".to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let workbook = project_service
            .store()
            .safe_xls_open(&project.id, &relative)?;
        if workbook.sha256 != source_sha256 {
            return Err(
                "GUI_RUNTIME_CONFIG_CHANGED: static XLS changed while snapshot was opening"
                    .to_string(),
            );
        }
        let Some(sheet_meta) = workbook.sheets.first() else {
            continue;
        };
        let sheet = project_service.store().safe_xls_sheet_read(
            &project.id,
            &relative,
            &sheet_meta.name,
            &workbook.sha256,
        )?;
        total_rows += sheet.row_count;
        total_cells += sheet.row_count.saturating_mul(sheet.column_count);
        if total_rows > MAX_SNAPSHOT_ROWS || total_cells > MAX_SNAPSHOT_CELLS {
            return Err(
                "GUI_RUNTIME_SNAPSHOT_DIMENSION_LIMIT: static snapshot is too large".to_string(),
            );
        }
        let parsed = parse_config_table(name, &sheet.rows);
        if *name == "cfg_game_data" {
            if let Some(object) = parsed.get("byKey").and_then(Value::as_object) {
                values.extend(object.clone());
            }
        }
        tables.insert((*name).to_string(), parsed);
        source_hashes.insert((*name).to_string(), Value::String(workbook.sha256));
    }
    let snapshot = json!({
        "tables": tables,
        "values": values,
        "sourceHashes": source_hashes,
    });
    let encoded = serde_json::to_vec(&snapshot)
        .map_err(|error| format!("GUI_RUNTIME_SNAPSHOT_ENCODE_FAILED: {error}"))?;
    if encoded.len() > MAX_SNAPSHOT_BYTES {
        return Err("GUI_RUNTIME_SNAPSHOT_BYTE_LIMIT: static snapshot is too large".to_string());
    }
    Ok(snapshot)
}

fn parse_config_table(name: &str, rows: &[Vec<String>]) -> Value {
    let header_index = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row.first()
                .is_some_and(|cell| cell.trim_start().starts_with("///"))
        })
        .map(|(index, _)| index)
        .last();
    let Some(header_index) = header_index else {
        return json!({ "rows": [] });
    };
    let headers = rows[header_index]
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let value = value.trim().trim_start_matches("///").trim();
            if value.is_empty() {
                format!("column{index}")
            } else {
                value.to_string()
            }
        })
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut by_key = Map::new();
    for row in rows.iter().skip(header_index + 1) {
        let first = row.first().map_or("", |value| value.trim());
        if first.is_empty() || first.starts_with("//") {
            continue;
        }
        if name == "cfg_game_data" && !GAME_DATA_KEYS.contains(&first) {
            continue;
        }
        let mut item = Map::new();
        for (index, header) in headers.iter().enumerate() {
            if !config_column_allowed(name, header, index) {
                continue;
            }
            let value = row.get(index).map_or("", String::as_str);
            item.insert(header.clone(), scalar_value(value));
        }
        if name == "cfg_game_data" {
            let value = row.get(1).map_or(Value::Null, |value| scalar_value(value));
            by_key.insert(first.to_string(), value);
        }
        output.push(Value::Object(item));
    }
    json!({ "rows": output, "byKey": by_key })
}

fn config_column_allowed(table: &str, header: &str, index: usize) -> bool {
    if table == "cfg_game_data" {
        return index < 2;
    }
    let header = header.trim().to_ascii_lowercase();
    let common = [
        "id",
        "idx",
        "name",
        "type",
        "kind",
        "icon",
        "iconid",
        "image",
        "resource",
        "stdmode",
        "shape",
        "looks",
        "color",
        "colour",
        "price",
        "currency",
        "need",
        "needlevel",
        "level",
        "job",
        "sex",
        "itemid",
        "itemidx",
        "magicid",
        "skillid",
        "buffid",
        "effect",
        "effectid",
        "effecttype",
        "model",
        "modelid",
        "map",
        "mapid",
        "mapname",
        "category",
        "categoryid",
        "firstlevel",
        "firstlevelname",
        "secondlevel",
        "secondlevelname",
        "direction",
        "width",
        "height",
        "scale",
        "itemsacle",
        "auctionby",
        "neffect",
        "desc",
        "tips",
        "value",
    ];
    common.contains(&header.as_str())
}

fn scalar_value(input: &str) -> Value {
    let value = input.trim();
    if value.len() > MAX_SNAPSHOT_CELL_BYTES {
        return Value::Null;
    }
    if value.is_empty() {
        return Value::Null;
    }
    if value.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if value.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if let Ok(number) = value.parse::<i64>() {
        return Value::Number(number.into());
    }
    if let Ok(number) = value.parse::<f64>() {
        if let Some(number) = serde_json::Number::from_f64(number) {
            return Value::Number(number);
        }
    }
    Value::String(value.to_string())
}

/// 为 Runtime 实例补充静态模板节点和精确源码行，确保完整场景中的编辑仍能落到 GUIExport。
fn runtime_source_binding_index(modules: &BTreeMap<String, String>) -> RuntimeSourceBindingIndex {
    let mut index = HashMap::new();
    for (path, source) in modules {
        if !path.starts_with("GUIExport/") {
            continue;
        }
        let sha = hex_sha256(source.as_bytes());
        let Ok(document) = parse_document(source, path, &sha, "utf-8", "\n") else {
            continue;
        };
        for node in document.nodes {
            index
                .entry((path.clone(), node.name.value.clone()))
                .or_insert(RuntimeSourceBinding {
                    template_node_id: node.id,
                    line: node.source_binding.create_call.start.row + 1,
                    column: node.source_binding.create_call.start.column,
                });
        }
    }
    index
}

/// Sidecar 不持有源码 AST；主进程用只读解析结果补齐 Source Binding，不暴露绝对路径。
fn enrich_runtime_source_bindings(scene: &mut Value, index: &RuntimeSourceBindingIndex) {
    let Some(nodes) = scene.get_mut("nodes").and_then(Value::as_object_mut) else {
        return;
    };
    for node in nodes.values_mut() {
        let Some(node_object) = node.as_object_mut() else {
            continue;
        };
        let Some(name) = node_object
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        let Some(source_ref) = node_object
            .get_mut("sourceRef")
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        let Some(path) = source_ref
            .get("devRelativePath")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        let Some(binding) = index.get(&(path, name)) else {
            continue;
        };
        source_ref.insert("line".to_string(), json!(binding.line));
        source_ref.insert("column".to_string(), json!(binding.column));
        source_ref.insert(
            "templateNodeId".to_string(),
            json!(binding.template_node_id),
        );
    }
}

fn collect_runtime_modules(
    project: &Mir3Project,
    working_sources: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    let dev_root = canonical_dev_root(project)?;
    let mut paths = Vec::new();
    for directory in ["GUILayout", "GUIExport", "GUIData"] {
        collect_lua_files(&dev_root.join(directory), &mut paths)?;
    }
    if paths.len() > MAX_RUNTIME_MODULES {
        return Err("GUI_RUNTIME_MODULE_LIMIT: too many Lua modules".to_string());
    }
    let mut modules = BTreeMap::new();
    let mut byte_length = 0usize;
    for path in paths {
        let relative = path
            .strip_prefix(&dev_root)
            .map_err(|_| "GUI_RUNTIME_MODULE_OUTSIDE: Lua module escaped DEV".to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let source = if let Some(source) = working_sources.get(&relative) {
            if source.len() > MAX_RUNTIME_MODULE_BYTES as usize {
                return Err(format!(
                    "GUI_RUNTIME_MODULE_SIZE_LIMIT: {relative} exceeds 8 MiB"
                ));
            }
            source.clone()
        } else {
            let bytes = read_bounded_file(
                &path,
                MAX_RUNTIME_MODULE_BYTES as usize,
                "GUI_RUNTIME_MODULE_READ_FAILED",
                "GUI_RUNTIME_MODULE_SIZE_LIMIT",
            )?;
            String::from_utf8_lossy(&bytes).into_owned()
        };
        add_to_budget(
            &mut byte_length,
            source.len(),
            MAX_RUNTIME_INPUT_BYTES,
            "GUI_RUNTIME_INPUT_LIMIT: virtual modules are too large",
        )?;
        modules.insert(relative, source);
    }
    for path in working_sources.keys() {
        if !modules.contains_key(path) {
            return Err(format!("GUI_RUNTIME_WORKING_SOURCE_DENIED: {path}"));
        }
    }
    Ok(modules)
}

fn runtime_binary_path(app: &AppHandle) -> Option<PathBuf> {
    let binary = if cfg!(windows) {
        "mir3-gui-runtime.exe"
    } else {
        "mir3-gui-runtime"
    };
    let mut candidates = Vec::new();
    if let Ok(resource) = app.path().resource_dir() {
        candidates.push(resource.join(binary));
        candidates.push(resource.join("binaries").join(binary));
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            candidates.push(directory.join(binary));
        }
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("debug")
            .join(binary),
    );
    candidates.into_iter().find(|path| path.is_file())
}

fn ensure_active_project(
    project_service: &ProjectService,
    project_id: &str,
) -> Result<Mir3Project, String> {
    let project = project_service
        .store()
        .active_project()?
        .ok_or_else(|| "GUI_RUNTIME_PROJECT_REQUIRED: no active project".to_string())?;
    if project.id != project_id {
        return Err("GUI_RUNTIME_PROJECT_MISMATCH: project is not active".to_string());
    }
    Ok(project)
}

fn ensure_same_active_project(
    project_service: &ProjectService,
    expected: &Mir3Project,
) -> Result<(), String> {
    let current = ensure_active_project(project_service, &expected.id)?;
    if current.root != expected.root
        || current.client_root != expected.client_root
        || current.engine_root != expected.engine_root
    {
        return Err(
            "GUI_RUNTIME_PROJECT_CHANGED: active project changed while runtime was starting"
                .to_string(),
        );
    }
    Ok(())
}

fn canonical_dev_root(project: &Mir3Project) -> Result<PathBuf, String> {
    let project_root = fs::canonicalize(&project.root)
        .map_err(|error| format!("GUI_RUNTIME_PROJECT_PATH_FAILED: {error}"))?;
    let client_root = fs::canonicalize(&project.client_root)
        .map_err(|error| format!("GUI_RUNTIME_CLIENT_PATH_FAILED: {error}"))?;
    let dev_root = fs::canonicalize(client_root.join("dev"))
        .map_err(|error| format!("GUI_RUNTIME_DEV_PATH_FAILED: {error}"))?;
    if !dev_root.starts_with(&project_root) {
        return Err("GUI_RUNTIME_DEV_OUTSIDE: DEV escaped active project".to_string());
    }
    Ok(dev_root)
}

fn config_table_root(project: &Mir3Project) -> Result<PathBuf, String> {
    let project_root = fs::canonicalize(&project.root)
        .map_err(|error| format!("GUI_RUNTIME_PROJECT_PATH_FAILED: {error}"))?;
    let engine_root = fs::canonicalize(&project.engine_root)
        .map_err(|error| format!("GUI_RUNTIME_ENGINE_PATH_FAILED: {error}"))?;
    let table_root = fs::canonicalize(engine_root.join("Mir200/Envir/Data"))
        .map_err(|error| format!("GUI_RUNTIME_CONFIG_PATH_FAILED: {error}"))?;
    if !table_root.starts_with(&project_root) || !table_root.starts_with(&engine_root) {
        return Err("GUI_RUNTIME_CONFIG_OUTSIDE: Data escaped engine root".to_string());
    }
    Ok(table_root)
}

fn collect_lua_files(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }
    let root_metadata = fs::symlink_metadata(root).map_err(|error| {
        format!(
            "GUI_RUNTIME_MODULE_ROOT_FAILED: {}: {error}",
            root.display()
        )
    })?;
    if root_metadata.file_type().is_symlink() {
        return Err(
            "GUI_RUNTIME_MODULE_ROOT_LINK: module root cannot be a symbolic link".to_string(),
        );
    }
    let canonical_root = fs::canonicalize(root).map_err(|error| {
        format!(
            "GUI_RUNTIME_MODULE_ROOT_FAILED: {}: {error}",
            root.display()
        )
    })?;
    collect_lua_files_inner(&canonical_root, &canonical_root, 0, output)
}

fn collect_lua_files_inner(
    trusted_root: &Path,
    directory: &Path,
    depth: usize,
    output: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if depth > MAX_RUNTIME_DIRECTORY_DEPTH {
        return Err("GUI_RUNTIME_MODULE_DEPTH_LIMIT: module tree is too deep".to_string());
    }
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "GUI_RUNTIME_MODULE_LIST_FAILED: {}: {error}",
            directory.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("GUI_RUNTIME_MODULE_ENTRY_FAILED: {error}"))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "GUI_RUNTIME_MODULE_METADATA_FAILED: {}: {error}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            let canonical = fs::canonicalize(&path).map_err(|error| {
                format!(
                    "GUI_RUNTIME_MODULE_PATH_FAILED: {}: {error}",
                    path.display()
                )
            })?;
            if !canonical.starts_with(trusted_root) {
                return Err(
                    "GUI_RUNTIME_MODULE_OUTSIDE: directory escaped trusted root".to_string()
                );
            }
            collect_lua_files_inner(trusted_root, &canonical, depth + 1, output)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("lua"))
        {
            if metadata.len() > MAX_RUNTIME_MODULE_BYTES {
                return Err(format!(
                    "GUI_RUNTIME_MODULE_SIZE_LIMIT: {} exceeds 8 MiB",
                    path.display()
                ));
            }
            let canonical = fs::canonicalize(&path).map_err(|error| {
                format!(
                    "GUI_RUNTIME_MODULE_PATH_FAILED: {}: {error}",
                    path.display()
                )
            })?;
            if !canonical.starts_with(trusted_root) {
                return Err("GUI_RUNTIME_MODULE_OUTSIDE: file escaped trusted root".to_string());
            }
            output.push(canonical);
            if output.len() > MAX_RUNTIME_MODULES {
                return Err("GUI_RUNTIME_MODULE_LIMIT: too many Lua modules".to_string());
            }
        }
    }
    Ok(())
}

fn scene_entry(
    layout_root: &Path,
    path: &Path,
    total_bytes: &mut usize,
) -> Result<Option<RuntimeSceneEntry>, String> {
    let source = read_bounded_file(
        path,
        MAX_RUNTIME_MODULE_BYTES as usize,
        "GUI_RUNTIME_CATALOG_READ_FAILED",
        "GUI_RUNTIME_MODULE_SIZE_LIMIT",
    )?;
    add_to_budget(
        total_bytes,
        source.len(),
        MAX_RUNTIME_CATALOG_BYTES,
        "GUI_RUNTIME_CATALOG_SIZE_LIMIT: GUILayout catalog is too large",
    )?;
    let source = String::from_utf8_lossy(&source);
    let runnable = source.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("function ") && line.contains(".main(")
    });
    if !runnable {
        return Ok(None);
    }
    let relative = path
        .strip_prefix(layout_root)
        .map_err(|_| "GUI_RUNTIME_CATALOG_OUTSIDE: scene escaped GUILayout".to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    let layout_path = format!("GUILayout/{relative}");
    let id = relative.trim_end_matches(".lua").to_string();
    let name = path
        .file_stem()
        .ok_or_else(|| "GUI_RUNTIME_CATALOG_NAME_INVALID: scene file has no name".to_string())?
        .to_string_lossy()
        .into_owned();
    let category = relative.split('/').next().unwrap_or("root").to_string();
    let platform = if name.ends_with("_win32") {
        "pc"
    } else if path.with_file_name(format!("{name}_win32.lua")).is_file() {
        "shared"
    } else {
        "mobile"
    };
    Ok(Some(RuntimeSceneEntry {
        id,
        name,
        category,
        layout_path,
        platform: platform.to_string(),
        compatibility: "approximate".to_string(),
    }))
}

fn tables_for_scene(layout_path: &str) -> Vec<&'static str> {
    let lower = layout_path.to_ascii_lowercase();
    if lower.contains("auction") {
        vec![
            "cfg_game_data",
            "cfg_auction_type",
            "cfg_item",
            "cfg_equip",
            "cfg_colour_style",
        ]
    } else if lower.contains("bag") || lower.contains("item") {
        vec![
            "cfg_game_data",
            "cfg_item",
            "cfg_equip",
            "cfg_show_equip",
            "cfg_colour_style",
        ]
    } else if lower.contains("login") || lower.contains("role") {
        vec![
            "cfg_game_data",
            "cfg_customjob",
            "cfg_loginAnim",
            "cfg_model_info",
        ]
    } else if lower.contains("store") {
        vec![
            "cfg_game_data",
            "cfg_store",
            "cfg_item",
            "cfg_equip",
            "cfg_colour_style",
        ]
    } else if lower.contains("main") || lower.contains("hud") {
        vec![
            "cfg_game_data",
            "cfg_colour_style",
            "cfg_buff",
            "cfg_model_info",
        ]
    } else {
        vec!["cfg_game_data", "cfg_colour_style"]
    }
}

fn mock_profile_for_scene(layout_path: &str) -> &'static str {
    let lower = layout_path.to_ascii_lowercase();
    if lower.contains("auction") {
        "auction"
    } else if lower.contains("bag") {
        "bag"
    } else if lower.contains("login") {
        "login"
    } else if lower.contains("store") {
        "store"
    } else if lower.contains("win32") {
        "hud-pc"
    } else {
        "hud-mobile"
    }
}

fn scene_mock_values(profile_id: &str) -> Value {
    let common = json!({
        "previewDataMode": "sceneMock",
        "playerName": "模拟角色",
        "level": 42,
        "job": 1,
        "gold": 128000,
        "virtualTime": "12:00:00",
    });
    let mut values = common.as_object().cloned().unwrap_or_default();
    let profile = match profile_id {
        "auction" => json!({
            "auctionCategory": "全部",
            "auctionItems": [
                { "id": 1001, "name": "模拟武器", "price": 12000 },
                { "id": 1002, "name": "模拟防具", "price": 8600 }
            ]
        }),
        "bag" => json!({
            "bagCapacity": 48,
            "bagItems": [
                { "id": 2001, "name": "模拟药水", "count": 10 },
                { "id": 2002, "name": "模拟卷轴", "count": 2 }
            ]
        }),
        "login" | "login-create" | "login-role" => json!({
            "loginState": "offlinePreview",
            "roles": [{ "id": 1, "name": "模拟角色", "level": 42, "job": 1 }]
        }),
        "store" => json!({
            "storeCategory": "推荐",
            "storeItems": [{ "id": 3001, "name": "模拟商品", "price": 100 }]
        }),
        "hud-pc" | "pc-hud" => {
            json!({ "deviceProfile": "pc", "hp": 850, "maxHp": 1000, "mp": 420, "maxMp": 600 })
        }
        _ => {
            json!({ "deviceProfile": "mobile", "hp": 850, "maxHp": 1000, "mp": 420, "maxMp": 600 })
        }
    };
    if let Some(profile) = profile.as_object() {
        values.extend(profile.clone());
    }
    Value::Object(values)
}

fn diagnostics_from_value(value: Option<&Value>) -> Vec<RuntimeDiagnostic> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| serde_json::from_value(item.clone()).ok())
        .collect()
}

fn validate_runtime_scene_result(
    result: &Value,
    operation: &str,
    expected_sequence: u64,
) -> Result<(u64, Value, Vec<RuntimeDiagnostic>), String> {
    let sequence = result
        .get("sequence")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("GUI_RUNTIME_RESPONSE_INVALID: {operation} sequence is missing"))?;
    if sequence != expected_sequence {
        return Err(format!(
            "GUI_RUNTIME_SEQUENCE_INVALID: {operation} sequence did not advance once"
        ));
    }
    let scene = result
        .get("scene")
        .cloned()
        .ok_or_else(|| format!("GUI_RUNTIME_RESPONSE_INVALID: {operation} scene is missing"))?;
    if !scene.is_object() {
        return Err(format!(
            "GUI_RUNTIME_RESPONSE_INVALID: {operation} scene must be an object"
        ));
    }
    let diagnostics = match result.get("diagnostics") {
        None | Some(Value::Null) => Vec::new(),
        Some(value) => serde_json::from_value(value.clone()).map_err(|error| {
            format!("GUI_RUNTIME_RESPONSE_INVALID: {operation} diagnostics are invalid: {error}")
        })?,
    };
    Ok((sequence, scene, diagnostics))
}

fn runtime_id(prefix: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut hasher = Sha256::new();
    hasher.update(prefix.as_bytes());
    hasher.update(now.to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    format!("{prefix}-{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create(label: &str) -> Self {
            let path = std::env::temp_dir().join(runtime_id(label));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn game_data_snapshot_exposes_only_registered_keys() {
        let rows = vec![
            vec!["///k".to_string(), "value".to_string()],
            vec!["bagConfig".to_string(), "1#2".to_string()],
            vec!["LOGIN_PASSWORD".to_string(), "secret".to_string()],
        ];
        let parsed = parse_config_table("cfg_game_data", &rows);
        assert_eq!(parsed["byKey"]["bagConfig"], "1#2");
        assert!(parsed["byKey"].get("LOGIN_PASSWORD").is_none());
        assert!(!serde_json::to_string(&parsed).unwrap().contains("secret"));
    }

    #[test]
    fn scene_table_selection_never_includes_sensitive_configuration() {
        for path in [
            "GUILayout/auction/AuctionMain.lua",
            "GUILayout/login/LoginAccount.lua",
            "GUILayout/player_bag/Bag.lua",
        ] {
            let tables = tables_for_scene(path);
            assert!(tables.len() <= MAX_SNAPSHOT_TABLES);
            assert!(!tables.iter().any(|name| {
                name.contains("Setup.json") || name.contains("dbsrc") || name.contains("Config")
            }));
        }
    }

    #[test]
    fn scalar_values_keep_primitive_types() {
        assert_eq!(scalar_value("42"), json!(42));
        assert_eq!(scalar_value("true"), json!(true));
        assert_eq!(scalar_value("a#b"), json!("a#b"));
        assert_eq!(scalar_value(""), Value::Null);
    }

    #[test]
    fn static_table_column_allowlist_removes_credentials_and_network_fields() {
        let rows = vec![
            vec![
                "///id".to_string(),
                "name".to_string(),
                "password".to_string(),
                "ip".to_string(),
                "port".to_string(),
            ],
            vec![
                "1".to_string(),
                "测试物品".to_string(),
                "secret-marker".to_string(),
                "10.0.0.1".to_string(),
                "7000".to_string(),
            ],
        ];
        let encoded = serde_json::to_string(&parse_config_table("cfg_item", &rows)).unwrap();
        assert!(encoded.contains("测试物品"));
        assert!(!encoded.contains("secret-marker"));
        assert!(!encoded.contains("10.0.0.1"));
        assert!(!encoded.contains("7000"));
    }

    #[test]
    fn runtime_stdout_is_rejected_before_an_unbounded_line_can_accumulate() {
        let (sender, receiver) = mpsc::channel();
        read_runtime_responses(vec![b'x'; MAX_RUNTIME_OUTPUT_BYTES + 1].as_slice(), sender);
        let error = receiver.recv().unwrap().unwrap_err();
        assert!(error.starts_with("GUI_RUNTIME_OUTPUT_LIMIT:"));
    }

    #[test]
    fn bounded_file_read_rejects_growth_past_the_declared_limit() {
        let directory = TestDirectory::create("runtime-bounded-read");
        let path = directory.0.join("oversized.lua");
        fs::write(&path, vec![b'x'; 9]).unwrap();
        let error = read_bounded_file(&path, 8, "READ_FAILED", "SIZE_LIMIT").unwrap_err();
        assert!(error.starts_with("SIZE_LIMIT:"));

        let mut total = 7usize;
        let error = add_to_budget(&mut total, 2, 8, "TOTAL_LIMIT").unwrap_err();
        assert_eq!(error, "TOTAL_LIMIT");
        assert_eq!(total, 7);
    }

    #[test]
    fn runtime_start_reservations_enforce_project_and_global_limits() {
        let directory = TestDirectory::create("runtime-reservations");
        let service = GuiRuntimeService::new(directory.0.clone()).unwrap();
        let first = service.reserve_session_start("project-1").unwrap();
        assert!(service.reserve_session_start("project-1").is_err());
        let second = service.reserve_session_start("project-2").unwrap();
        let third = service.reserve_session_start("project-3").unwrap();
        let fourth = service.reserve_session_start("project-4").unwrap();
        assert!(service.reserve_session_start("project-5").is_err());

        drop(first);
        let fifth = service.reserve_session_start("project-5").unwrap();
        drop((second, third, fourth, fifth));
        assert!(service
            .starting_projects
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty());
    }

    #[test]
    fn stale_start_reservation_cannot_release_a_new_project_slot() {
        let directory = TestDirectory::create("runtime-reservation-token");
        let service = GuiRuntimeService::new(directory.0.clone()).unwrap();
        let stale = service.reserve_session_start("project-1").unwrap();
        service.cancel_start_reservation("project-1");
        let current = service.reserve_session_start("project-1").unwrap();
        drop(stale);
        assert!(service
            .starting_projects
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains_key("project-1"));
        drop(current);
        assert!(service
            .starting_projects
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty());
    }

    #[test]
    fn preferences_replace_an_existing_file_without_losing_memory_state() {
        let directory = TestDirectory::create("runtime-preferences");
        let service = GuiRuntimeService::new(directory.0.clone()).unwrap();
        service
            .set_data_source("project-1", RuntimeDataSource::ProjectStatic)
            .unwrap();
        service
            .set_data_source("project-1", RuntimeDataSource::BuiltInMock)
            .unwrap();
        let reopened = GuiRuntimeService::new(directory.0.clone()).unwrap();
        assert_eq!(
            reopened.data_source("project-1"),
            RuntimeDataSource::BuiltInMock
        );
        assert!(directory.0.join("runtime-preferences.json").is_file());
    }

    #[test]
    fn post_transaction_scene_response_requires_exact_protocol_fields() {
        let valid = json!({
            "sequence": 2,
            "scene": { "nodes": [] },
            "diagnostics": []
        });
        assert!(validate_runtime_scene_result(&valid, "event", 2).is_ok());

        let missing_scene = json!({ "sequence": 2, "diagnostics": [] });
        assert!(validate_runtime_scene_result(&missing_scene, "event", 2).is_err());
        let wrong_sequence = json!({ "sequence": 3, "scene": {}, "diagnostics": [] });
        assert!(validate_runtime_scene_result(&wrong_sequence, "event", 2).is_err());
        let malformed_diagnostics = json!({ "sequence": 2, "scene": {}, "diagnostics": "invalid" });
        assert!(validate_runtime_scene_result(&malformed_diagnostics, "event", 2).is_err());
    }

    #[test]
    fn six_scene_mock_profiles_are_offline_and_contain_no_credentials() {
        for profile in ["login", "hud-mobile", "hud-pc", "bag", "auction", "store"] {
            let encoded = serde_json::to_string(&scene_mock_values(profile)).unwrap();
            assert!(encoded.contains("sceneMock"));
            for denied in ["password", "dbinfo", "127.0.0.1", "engineRoot"] {
                assert!(!encoded
                    .to_ascii_lowercase()
                    .contains(&denied.to_ascii_lowercase()));
            }
        }
    }

    #[test]
    fn runtime_home_contains_only_four_composed_presets() {
        let presets = runtime_presets();
        assert_eq!(presets.len(), 4);
        assert_eq!(presets[0].id, "character-create");
        assert_eq!(presets[1].id, "character-select");
        assert_eq!(presets[2].default_map_id.as_deref(), Some("01"));
        assert_eq!(presets[3].default_map_id.as_deref(), Some("1"));
        assert!(presets[2].overlay_ids.contains(&"bag".to_string()));
        assert!(presets[3].overlay_ids.contains(&"store".to_string()));
    }

    #[test]
    fn runtime_nodes_receive_precise_export_template_bindings() {
        let modules = BTreeMap::from([(
            "GUIExport/demo/main.lua".to_string(),
            "local ui = {}\nfunction ui.init(parent)\n  local Button_close = GUI:Button_Create(parent, \"Button_close\", 10, 20, \"close.png\")\nend\nreturn ui\n".to_string(),
        )]);
        let index = runtime_source_binding_index(&modules);
        let mut scene = json!({
            "nodes": {
                "runtime-node-1": {
                    "name": "Button_close",
                    "sourceRef": { "devRelativePath": "GUIExport/demo/main.lua" }
                }
            }
        });
        enrich_runtime_source_bindings(&mut scene, &index);
        let source_ref = &scene["nodes"]["runtime-node-1"]["sourceRef"];
        assert_eq!(source_ref["line"], 3);
        assert!(source_ref["column"].as_u64().is_some_and(|value| value > 0));
        assert!(source_ref["templateNodeId"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
    }
}
