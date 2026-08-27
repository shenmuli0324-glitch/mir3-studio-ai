//! Core 候选升级使用的隔离 MCP 运行时验证。

use crate::{config, service};
use mir3_domain::DomainStore;
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const CANARY_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreMcpCanaryReport {
    pub tool: String,
    pub capability_id: String,
    pub capability_mode: String,
    pub isolated_project: bool,
}

struct TempCanaryRoot(PathBuf);

impl Drop for TempCanaryRoot {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            log::warn!("Core MCP canary temp cleanup failed: {error}");
        }
    }
}

/// 在一次性项目和数据库中启动真实 MCP sidecar，并执行只读工具及官方只读能力。
pub async fn run(app: &AppHandle) -> Result<CoreMcpCanaryReport, String> {
    let binary = service::project::mcp_binary_path(app).ok_or_else(|| {
        "CORE_MCP_CANARY_BINARY_MISSING: MIR3 MCP sidecar is unavailable".to_string()
    })?;
    let domain_pack_root = config::get_dsh_data_path(app).join("domain-packs");
    if !domain_pack_root.is_dir() {
        return Err(format!(
            "CORE_MCP_CANARY_PACKS_MISSING: {}",
            domain_pack_root.display()
        ));
    }
    run_with_paths(&binary, &domain_pack_root).await
}

async fn run_with_paths(
    binary: &Path,
    domain_pack_root: &Path,
) -> Result<CoreMcpCanaryReport, String> {
    let temp = create_temp_root()?;
    let home = temp.0.join("home");
    let fixture = temp.0.join("fixture");
    let level_file = fixture.join("客户端/dev/Level/Level.txt");
    fs::create_dir_all(level_file.parent().ok_or_else(|| {
        "CORE_MCP_CANARY_FIXTURE_INVALID: level parent is unavailable".to_string()
    })?)
    .map_err(|error| format!("CORE_MCP_CANARY_FIXTURE_CREATE_FAILED: {error}"))?;
    fs::create_dir_all(fixture.join("引擎/Mir200/Envir"))
        .map_err(|error| format!("CORE_MCP_CANARY_FIXTURE_CREATE_FAILED: {error}"))?;
    fs::write(
        &level_file,
        "level=1\nrequiredExperience=100\nstatPoints=1\nrecommendedMonsterId=monster:fixture-1\n",
    )
    .map_err(|error| format!("CORE_MCP_CANARY_FIXTURE_WRITE_FAILED: {error}"))?;

    let store = DomainStore::new_with_domain_pack_root(
        home.join("projects"),
        domain_pack_root.to_path_buf(),
    )?;
    let project = store.import_project(&fixture)?;
    store.scan_project(&project.id, || false)?;
    let manifest = store
        .list_domain_systems()?
        .into_iter()
        .find(|manifest| manifest.system_id == "level")
        .ok_or_else(|| {
            "CORE_MCP_CANARY_LEVEL_MISSING: level domain pack is unavailable".to_string()
        })?;
    let lease = store.issue_task_scope(
        &project.id,
        "core-mcp-canary-task",
        &["level".to_string()],
        &["level".to_string()],
        &[],
        json!({"level": manifest.version}),
        mir3_domain::now_millis() + 60_000,
    )?;
    drop(store);

    let requests = [
        json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"tools/call",
            "params":{"name":"mir3_system_list","arguments":{"scopeToken":lease.token}}
        }),
        json!({
            "jsonrpc":"2.0",
            "id":2,
            "method":"tools/call",
            "params":{"name":"mir3_capability_invoke","arguments":{
                "scopeToken":lease.token,
                "systemId":"level",
                "capabilityId":"inspect-level-curve",
                "version":manifest.version,
                "params":{"operation":"inspect-level-curve"}
            }}
        }),
    ];
    let input = requests
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut command = Command::new(binary);
    command
        .env("MIR3_STUDIO_HOME", &home)
        .env("MIR3_ACTIVE_PROJECT_ID", &project.id)
        .env("MIR3_DOMAIN_PACK_ROOT", domain_pack_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    configure_no_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("CORE_MCP_CANARY_SPAWN_FAILED: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "CORE_MCP_CANARY_STDIN_MISSING: sidecar stdin is unavailable".to_string())?;
    stdin
        .write_all(input.as_bytes())
        .await
        .map_err(|error| format!("CORE_MCP_CANARY_WRITE_FAILED: {error}"))?;
    drop(stdin);
    let output = tokio::time::timeout(CANARY_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| "CORE_MCP_CANARY_TIMEOUT: sidecar did not finish in time".to_string())?
        .map_err(|error| format!("CORE_MCP_CANARY_WAIT_FAILED: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "CORE_MCP_CANARY_PROCESS_FAILED: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    validate_output(&String::from_utf8_lossy(&output.stdout))
}

fn validate_output(stdout: &str) -> Result<CoreMcpCanaryReport, String> {
    let responses = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .map_err(|error| format!("CORE_MCP_CANARY_RESPONSE_INVALID: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let system_list = response(&responses, 1)?;
    if system_list
        .pointer("/result/isError")
        .and_then(Value::as_bool)
        != Some(false)
        || system_list
            .pointer("/result/structuredContent/systems")
            .and_then(Value::as_array)
            .is_none_or(|systems| {
                systems.len() != 1
                    || systems[0].get("systemId").and_then(Value::as_str) != Some("level")
            })
    {
        return Err(
            "CORE_MCP_CANARY_TOOL_FAILED: mir3_system_list did not enforce the level-only scope"
                .to_string(),
        );
    }
    let capability = response(&responses, 2)?;
    if capability
        .pointer("/result/isError")
        .and_then(Value::as_bool)
        != Some(false)
        || capability
            .pointer("/result/structuredContent/mode")
            .and_then(Value::as_str)
            != Some("read")
        || capability
            .pointer("/result/structuredContent/operation")
            .and_then(Value::as_str)
            != Some("inspect-level-curve")
    {
        return Err(format!(
            "CORE_MCP_CANARY_CAPABILITY_FAILED: inspect-level-curve read dry-run failed: {capability}"
        ));
    }
    Ok(CoreMcpCanaryReport {
        tool: "mir3_system_list".to_string(),
        capability_id: "inspect-level-curve".to_string(),
        capability_mode: "read".to_string(),
        isolated_project: true,
    })
}

fn response(responses: &[Value], id: i64) -> Result<&Value, String> {
    responses
        .iter()
        .find(|response| response.get("id").and_then(Value::as_i64) == Some(id))
        .ok_or_else(|| format!("CORE_MCP_CANARY_RESPONSE_MISSING: response {id} is unavailable"))
}

fn create_temp_root() -> Result<TempCanaryRoot, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("CORE_MCP_CANARY_CLOCK_FAILED: {error}"))?
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "mir3-core-mcp-canary-{}-{timestamp}",
        std::process::id()
    ));
    fs::create_dir(&path)
        .map_err(|error| format!("CORE_MCP_CANARY_TEMP_CREATE_FAILED: {error}"))?;
    Ok(TempCanaryRoot(path))
}

#[cfg(windows)]
fn configure_no_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.as_std_mut().creation_flags(0x08000000);
}

#[cfg(not(windows))]
fn configure_no_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_failed_or_forged_mcp_canary_results() {
        let failed = json!({"jsonrpc":"2.0","id":1,"result":{"isError":true}});
        let forged = json!({"jsonrpc":"2.0","id":2,"result":{"isError":false,"structuredContent":{"mode":"write","operation":"inspect-level-curve"}}});
        let output = format!("{failed}\n{forged}\n");
        assert!(validate_output(&output)
            .unwrap_err()
            .starts_with("CORE_MCP_CANARY_TOOL_FAILED:"));
    }

    #[tokio::test]
    async fn launches_real_mcp_for_read_tool_and_capability_on_isolated_fixture() {
        let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("debug")
            .join(if cfg!(windows) {
                "mir3-mcp.exe"
            } else {
                "mir3-mcp"
            });
        if !binary.is_file() {
            eprintln!("CORE_MCP_CANARY_TEST_SKIPPED: build mir3-mcp first");
            return;
        }
        let packs = create_temp_root().unwrap();
        let installed = packs.0.join("domain-packs");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("mir3-domain-packs");
        crate::service::plugin::system::ensure_domain_pack_root(&source, &installed).unwrap();
        let report = run_with_paths(&binary, &installed).await.unwrap();
        assert_eq!(report.tool, "mir3_system_list");
        assert_eq!(report.capability_id, "inspect-level-curve");
        assert_eq!(report.capability_mode, "read");
        assert!(report.isolated_project);
    }
}
