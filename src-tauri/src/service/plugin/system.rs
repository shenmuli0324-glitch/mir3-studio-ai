//! MIR3 第一方系统插件安装器。
//!
//! 系统插件无第三方依赖和安装脚本，启动前直接复制到活动 Profile，并用带标记的
//! patch 块挂载 Workspace 桥与官方 MCP Client；不经过可跳过的社区预装流程。

use crate::{config, service};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub const PACKAGE_NAME: &str = "@mir3-studio/dsh-mir3-core";
const LOCAL_PLUGIN_SPEC: &str = "file:.mir3-system-plugins/dsh-mir3-core";
const MARK_START: &str = "# >>> MIR3 Studio AI system plugin >>>";
const MARK_END: &str = "# <<< MIR3 Studio AI system plugin <<<";

pub fn ensure(app: &AppHandle) -> Result<(), String> {
    let profile = service::profile::ensure_active_profile(app)?;
    let source = resource_path(app, "mir3-core-plugin")?;
    // 保留一份 Profile 内的本地依赖源，避免后续 pnpm 操作把第一方插件当成
    // npm 注册表包解析，或将 node_modules 中的“额外目录”清理掉。
    let local_source = profile.join(".mir3-system-plugins").join("dsh-mir3-core");
    replace_directory(&source, &local_source)?;
    let destination = profile
        .join("node_modules")
        .join("@mir3-studio")
        .join("dsh-mir3-core");
    replace_directory(&local_source, &destination)?;
    ensure_manifest_dependency(&profile.join("package.json"))?;

    let skill_source = resource_path(app, "mir3-skill")?.join("mir3-996-development");
    let skill_destination = config::get_dsh_data_path(app)
        .join("skills")
        .join("mir3-996-development");
    replace_directory(&skill_source, &skill_destination)?;

    let project_service = app.state::<service::project::ProjectService>();
    let active_project = project_service.store().active_project()?;
    let mcp_binary = service::project::mcp_binary_path(app);
    let patch = render_patch(app, active_project.as_ref(), mcp_binary.as_deref());
    merge_managed_patch(&profile.join("cordis.patch.yml"), &patch)?;
    log::info!("MIR3 system plugin ensured in {}", destination.display());
    Ok(())
}

fn resource_path(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    if let Ok(resource) = app.path().resource_dir() {
        for candidate in [resource.join("resources").join(name), resource.join(name)] {
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }
    }
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(name);
    source
        .is_dir()
        .then_some(source)
        .ok_or_else(|| format!("MIR3_SYSTEM_RESOURCE_MISSING: {name}"))
}

fn replace_directory(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        fs::remove_dir_all(destination)
            .map_err(|e| format!("MIR3_SYSTEM_PLUGIN_REMOVE_FAILED: {e}"))?;
    }
    copy_directory(source, destination)
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|e| format!("MIR3_SYSTEM_PLUGIN_CREATE_FAILED: {e}"))?;
    for entry in fs::read_dir(source).map_err(|e| format!("MIR3_SYSTEM_PLUGIN_READ_FAILED: {e}"))? {
        let entry = entry.map_err(|e| format!("MIR3_SYSTEM_PLUGIN_READ_FAILED: {e}"))?;
        let source_path = entry.path();
        let target = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_directory(&source_path, &target)?;
        } else {
            fs::copy(&source_path, &target)
                .map_err(|e| format!("MIR3_SYSTEM_PLUGIN_COPY_FAILED: {e}"))?;
        }
    }
    Ok(())
}

fn ensure_manifest_dependency(path: &Path) -> Result<(), String> {
    let raw =
        fs::read_to_string(path).map_err(|e| format!("MIR3_SYSTEM_MANIFEST_READ_FAILED: {e}"))?;
    let mut manifest: Value =
        serde_json::from_str(&raw).map_err(|e| format!("MIR3_SYSTEM_MANIFEST_INVALID: {e}"))?;
    let dependencies = manifest
        .as_object_mut()
        .and_then(|root| root.get_mut("dependencies"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            "MIR3_SYSTEM_MANIFEST_INVALID: dependencies must be an object".to_string()
        })?;
    dependencies.insert(
        PACKAGE_NAME.to_string(),
        Value::String(LOCAL_PLUGIN_SPEC.to_string()),
    );
    let content = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("MIR3_SYSTEM_MANIFEST_RENDER_FAILED: {e}"))?;
    fs::write(path, format!("{content}\n"))
        .map_err(|e| format!("MIR3_SYSTEM_MANIFEST_WRITE_FAILED: {e}"))
}

/// 第一方系统插件由 Studio 管理，不能通过普通插件管理或故障恢复入口移除。
pub fn is_system_plugin(id: &str) -> bool {
    id == PACKAGE_NAME
}

fn render_patch(
    app: &AppHandle,
    project: Option<&mir3_domain::Mir3Project>,
    mcp_binary: Option<&Path>,
) -> String {
    let mut rows = vec![
        MARK_START.to_string(),
        "- insert:".to_string(),
        "    - id: mir3-core-plugin".to_string(),
        format!("      name: '{}'", PACKAGE_NAME),
    ];
    if let (Some(project), Some(binary)) = (project, mcp_binary) {
        rows.extend([
            "    - id: mir3-mcp".to_string(),
            "      name: '@deepseek-ai/dsh-mcp-client'".to_string(),
            "      config:".to_string(),
            "        serverName: mir3".to_string(),
            "        transport: stdio".to_string(),
            format!(
                "        command: '{}'",
                yaml_quote(&binary.to_string_lossy())
            ),
            "        args: []".to_string(),
            format!("        cwd: '{}'", yaml_quote(&project.root)),
            "        env:".to_string(),
            format!(
                "          MIR3_STUDIO_HOME: '{}'",
                yaml_quote(&config::get_dsh_data_path(app).to_string_lossy())
            ),
            format!(
                "          MIR3_ACTIVE_PROJECT_ID: '{}'",
                yaml_quote(&project.id)
            ),
            "        failOnStartupError: false".to_string(),
        ]);
    }
    rows.push(MARK_END.to_string());
    rows.join("\n")
}

fn merge_managed_patch(path: &Path, managed: &str) -> Result<(), String> {
    let existing = fs::read_to_string(path).unwrap_or_else(|_| "[]\n".to_string());
    let without_old = remove_managed_block(&existing);
    let has_sequence = without_old
        .lines()
        .map(str::trim)
        .any(|line| line.starts_with("- "));
    let base = if has_sequence {
        without_old.trim_end().to_string()
    } else {
        without_old
            .lines()
            .filter(|line| line.trim() != "[]")
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end()
            .to_string()
    };
    let content = if base.is_empty() {
        format!("{managed}\n")
    } else {
        format!("{base}\n{managed}\n")
    };
    fs::write(path, content).map_err(|e| format!("MIR3_SYSTEM_PATCH_WRITE_FAILED: {e}"))
}

fn remove_managed_block(content: &str) -> String {
    let mut output = Vec::new();
    let mut managed = false;
    for line in content.lines() {
        if line.trim() == MARK_START {
            managed = true;
            continue;
        }
        if line.trim() == MARK_END {
            managed = false;
            continue;
        }
        if !managed {
            output.push(line);
        }
    }
    output.join("\n")
}

fn yaml_quote(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_patch_replaces_empty_yaml_array() {
        let path = std::env::temp_dir().join(format!("mir3-system-patch-{}", std::process::id()));
        fs::write(&path, "# user comment\n[]\n").unwrap();
        merge_managed_patch(&path, &format!("{MARK_START}\n- insert: []\n{MARK_END}")).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# user comment"));
        assert!(!content.lines().any(|line| line.trim() == "[]"));
        assert_eq!(content.matches(MARK_START).count(), 1);
        fs::remove_file(path).ok();
    }

    #[test]
    fn managed_patch_is_idempotent() {
        let path = std::env::temp_dir().join(format!(
            "mir3-system-patch-idempotent-{}",
            std::process::id()
        ));
        let block = format!("{MARK_START}\n- insert: []\n{MARK_END}");
        fs::write(&path, "- config: {}\n").unwrap();
        merge_managed_patch(&path, &block).unwrap();
        merge_managed_patch(&path, &block).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content.matches(MARK_START).count(), 1);
        fs::remove_file(path).ok();
    }

    #[test]
    fn manifest_uses_profile_local_system_plugin_source() {
        let path =
            std::env::temp_dir().join(format!("mir3-system-manifest-{}.json", std::process::id()));
        fs::write(&path, r#"{"dependencies":{}}"#).unwrap();
        ensure_manifest_dependency(&path).unwrap();
        let manifest: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            manifest.pointer("/dependencies/@mir3-studio~1dsh-mir3-core"),
            Some(&Value::String(LOCAL_PLUGIN_SPEC.to_string()))
        );
        fs::remove_file(path).ok();
    }

    #[test]
    fn bundled_plugin_matches_harness_module_loader_contract() {
        let server_entry = include_str!("../../../resources/mir3-core-plugin/lib/index.js");
        let client_entry = include_str!("../../../resources/mir3-core-plugin/lib/client.js");
        let manifest: Value = serde_json::from_str(include_str!(
            "../../../resources/mir3-core-plugin/package.json"
        ))
        .unwrap();

        assert!(server_entry.contains("export default plugin"));
        assert!(server_entry.contains("function apply()"));
        assert!(client_entry.contains("module.exports = { apply, inject, name }"));
        assert!(client_entry.contains("return module.exports"));
        assert_eq!(
            manifest.pointer("/exports/./default"),
            Some(&Value::String("./lib/index.js".to_string()))
        );
        assert_eq!(
            manifest.pointer("/exports/.~1client/default"),
            Some(&Value::String("./lib/client.js".to_string()))
        );
    }
}
