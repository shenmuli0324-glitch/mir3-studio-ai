//! MIR3 第一方系统插件安装器。
//!
//! 系统插件无第三方依赖和安装脚本，启动前直接复制到活动 Profile，并用带标记的
//! patch 块挂载 Workspace 桥与官方 MCP Client；不经过可跳过的社区预装流程。

use crate::{config, service};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub const PACKAGE_NAME: &str = "@mir3-studio/dsh-mir3-core";
const RETIRED_SAFE_FILES_PACKAGE: &str = "@mir3-studio/dsh-mir3-safe-files";
const LOCAL_PLUGIN_SPEC: &str = "file:.mir3-system-plugins/dsh-mir3-core";
const MARK_START: &str = "# >>> MIR3 Studio AI system plugin >>>";
const MARK_END: &str = "# <<< MIR3 Studio AI system plugin <<<";
pub(super) const DOMAIN_KERNEL_VERSION: &str = "1.0.0";
const DOMAIN_PACK_STATE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DomainPackRelease {
    pub version: String,
    pub hash: String,
    pub directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DomainPackState {
    pub schema_version: u32,
    pub system_id: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub candidate: Option<DomainPackRelease>,
    pub current: Option<DomainPackRelease>,
    pub previous: Option<DomainPackRelease>,
    pub lkg: Option<DomainPackRelease>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainPackStateView {
    #[serde(flatten)]
    pub state: DomainPackState,
    pub changelog: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DomainPackDescriptor {
    kind: String,
    system_id: String,
    version: String,
    kernel_api_range: String,
    supported_engine_range: String,
    manifest_schema_version: u32,
    resource_schema_version: u32,
    capability_schema_version: u32,
    memory_schema_version: u32,
}

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
    if let Err(error) = remove_retired_safe_files_plugin(&profile) {
        log::warn!("MIR3 retired Safe Files plugin cleanup incomplete: {error}");
    }

    let skill_source = resource_path(app, "mir3-skill")?.join("mir3-996-development");
    let skill_destination = config::get_dsh_data_path(app)
        .join("skills")
        .join("mir3-996-development");
    replace_directory(&skill_source, &skill_destination)?;

    // 领域包损坏不能阻断 Harness 启动；Kernel 会在包不可用时进入只读诊断。
    if let Err(error) = ensure_bundled_domain_packs(app) {
        log::error!("MIR3 domain packs unavailable: {error}");
    }

    let project_service = app.state::<service::project::ProjectService>();
    let active_project = project_service.store().active_project()?;
    let mcp_binary = service::project::mcp_binary_path(app);
    let patch = render_patch(app, active_project.as_ref(), mcp_binary.as_deref());
    merge_managed_patch(&profile.join("cordis.patch.yml"), &patch)?;
    log::info!("MIR3 system plugin ensured in {}", destination.display());
    Ok(())
}

/// 将随应用分发的领域包安装到版本化目录；首次安装直接建立 LKG，升级只暂存候选。
pub fn ensure_bundled_domain_packs(app: &AppHandle) -> Result<(), String> {
    let source_root = resource_path(app, "mir3-domain-packs")?;
    let destination_root = config::get_dsh_data_path(app).join("domain-packs");
    ensure_domain_pack_root(&source_root, &destination_root)
}

pub(crate) fn ensure_domain_pack_root(
    source_root: &Path,
    destination_root: &Path,
) -> Result<(), String> {
    let registry: Value = serde_json::from_str(
        &fs::read_to_string(source_root.join("registry.json"))
            .map_err(|error| format!("DOMAIN_REGISTRY_READ_FAILED: {error}"))?,
    )
    .map_err(|error| format!("DOMAIN_REGISTRY_INVALID: {error}"))?;
    let packs = registry
        .get("packs")
        .and_then(Value::as_array)
        .ok_or_else(|| "DOMAIN_REGISTRY_INVALID: packs must be an array".to_string())?;
    if packs.len() != 33 {
        return Err(format!(
            "DOMAIN_REGISTRY_COUNT_INVALID: expected 33, got {}",
            packs.len()
        ));
    }
    fs::create_dir_all(destination_root)
        .map_err(|error| format!("DOMAIN_PACK_ROOT_CREATE_FAILED: {error}"))?;
    let mut system_ids = BTreeSet::new();
    for pack in packs {
        let system_id = pack
            .get("systemId")
            .and_then(Value::as_str)
            .ok_or_else(|| "DOMAIN_REGISTRY_INVALID: systemId is required".to_string())?;
        if !system_ids.insert(system_id) {
            return Err(format!("DOMAIN_REGISTRY_DUPLICATE_SYSTEM: {system_id}"));
        }
        let source = source_root.join(system_id);
        let state = stage_domain_pack_candidate(destination_root, &source)?;
        if state.system_id != system_id {
            return Err(format!(
                "DOMAIN_REGISTRY_PACK_MISMATCH: expected {system_id}, got {}",
                state.system_id
            ));
        }
        if state.current.is_none() {
            activate_domain_pack_candidate(destination_root, system_id)?;
            mark_domain_pack_lkg(destination_root, system_id)?;
        }
    }
    Ok(())
}

/// 校验并暂存一个候选包。此阶段不会改变当前版本，进程中断后仍可安全重试。
pub fn stage_domain_pack_candidate(
    destination_root: &Path,
    source: &Path,
) -> Result<DomainPackState, String> {
    let descriptor = validate_domain_pack(source)?;
    let digest = hash_directory(source)?;
    let short_hash = digest
        .get(..12)
        .ok_or_else(|| "DOMAIN_PACK_HASH_INVALID: digest is too short".to_string())?
        .to_string();
    let release = DomainPackRelease {
        version: descriptor.version,
        hash: digest,
        directory: format!("{}-{short_hash}", descriptor.system_id),
    };
    let system_root = destination_root.join(&descriptor.system_id);
    let releases_root = system_root.join("releases");
    fs::create_dir_all(&releases_root)
        .map_err(|error| format!("DOMAIN_PACK_RELEASE_ROOT_CREATE_FAILED: {error}"))?;
    let release_path = releases_root.join(&release.directory);
    if !release_path.is_dir() {
        let staging = system_root.join(format!(
            ".staging-{}-{}",
            std::process::id(),
            release.directory
        ));
        remove_path_if_exists(&staging)?;
        copy_directory(source, &staging)?;
        let staged_hash = hash_directory(&staging)?;
        if staged_hash != release.hash {
            remove_path_if_exists(&staging)?;
            return Err(format!(
                "DOMAIN_PACK_HASH_MISMATCH: expected {}, got {staged_hash}",
                release.hash
            ));
        }
        fs::rename(&staging, &release_path).map_err(|error| {
            let _ = remove_path_if_exists(&staging);
            format!("DOMAIN_PACK_STAGE_COMMIT_FAILED: {error}")
        })?;
    } else if hash_directory(&release_path)? != release.hash {
        return Err(format!(
            "DOMAIN_PACK_INSTALLED_HASH_MISMATCH: {}",
            release_path.display()
        ));
    }

    let mut state = read_domain_pack_state(&system_root, &descriptor.system_id)?;
    if state
        .current
        .as_ref()
        .is_some_and(|current| current.version == release.version && current != &release)
    {
        return Err(format!(
            "DOMAIN_PACK_VERSION_REUSE_FORBIDDEN: {}@{} already identifies another immutable release",
            descriptor.system_id, release.version
        ));
    }
    if state.current.as_ref() == Some(&release) {
        state.candidate = None;
    } else {
        state.candidate = Some(release);
    }
    persist_domain_pack_state(&system_root, &state)?;
    Ok(state)
}

/// 原子切换到已校验候选；只有状态指针改变，运行中的旧版本目录始终保留。
pub fn activate_domain_pack_candidate(
    destination_root: &Path,
    system_id: &str,
) -> Result<DomainPackState, String> {
    validate_system_id(system_id)?;
    let system_root = destination_root.join(system_id);
    let mut state = read_domain_pack_state(&system_root, system_id)?;
    let candidate = state
        .candidate
        .clone()
        .ok_or_else(|| format!("DOMAIN_PACK_CANDIDATE_MISSING: {system_id}"))?;
    validate_installed_release(&system_root, system_id, &candidate)?;
    if state.current.as_ref() != Some(&candidate) {
        state.previous = state.current.clone();
        state.current = Some(candidate.clone());
    }
    state.enabled = true;
    state.candidate = None;
    persist_domain_pack_state(&system_root, &state)?;
    Ok(state)
}

/// 候选只有在运行时 canary 成功后才推进 LKG；canary 或 LKG 提交失败时，
/// 必须立即恢复 previous/LKG，避免 current 指向尚未验收的领域契约。
pub fn activate_domain_pack_with_canary<F>(
    destination_root: &Path,
    system_id: &str,
    canary: F,
) -> Result<DomainPackState, String>
where
    F: FnOnce(&DomainPackState) -> Result<(), String>,
{
    let activated = activate_domain_pack_candidate(destination_root, system_id)?;
    if let Err(error) = canary(&activated) {
        let rollback = rollback_domain_pack(destination_root, system_id);
        return Err(format!(
            "DOMAIN_PACK_RUNTIME_CANARY_FAILED: {error}; rollback={}",
            rollback
                .map(|_| "ok".to_string())
                .unwrap_or_else(|rollback_error| rollback_error)
        ));
    }
    match mark_domain_pack_lkg(destination_root, system_id) {
        Ok(stable) => Ok(stable),
        Err(error) => {
            let rollback = rollback_domain_pack(destination_root, system_id);
            Err(format!(
                "DOMAIN_PACK_LKG_COMMIT_FAILED: {error}; rollback={}",
                rollback
                    .map(|_| "ok".to_string())
                    .unwrap_or_else(|rollback_error| rollback_error)
            ))
        }
    }
}

/// 将当前版本标记为已通过 canary 的最后已知可用版本。
pub fn mark_domain_pack_lkg(
    destination_root: &Path,
    system_id: &str,
) -> Result<DomainPackState, String> {
    validate_system_id(system_id)?;
    let system_root = destination_root.join(system_id);
    let mut state = read_domain_pack_state(&system_root, system_id)?;
    let current = state
        .current
        .clone()
        .ok_or_else(|| format!("DOMAIN_PACK_CURRENT_MISSING: {system_id}"))?;
    validate_installed_release(&system_root, system_id, &current)?;
    state.lkg = Some(current);
    persist_domain_pack_state(&system_root, &state)?;
    Ok(state)
}

/// 候选失败时优先恢复 previous，否则恢复 LKG；失败版本仍保留供诊断。
pub fn rollback_domain_pack(
    destination_root: &Path,
    system_id: &str,
) -> Result<DomainPackState, String> {
    validate_system_id(system_id)?;
    let system_root = destination_root.join(system_id);
    let mut state = read_domain_pack_state(&system_root, system_id)?;
    let target = state
        .previous
        .clone()
        .or_else(|| state.lkg.clone())
        .ok_or_else(|| format!("DOMAIN_PACK_ROLLBACK_TARGET_MISSING: {system_id}"))?;
    validate_installed_release(&system_root, system_id, &target)?;
    let failed = state.current.clone();
    state.current = Some(target.clone());
    state.previous = failed.filter(|release| release != &target);
    state.candidate = None;
    state.enabled = true;
    persist_domain_pack_state(&system_root, &state)?;
    Ok(state)
}

/// 用户禁用只改变原子状态指针，不删除任何版本；运行中固定版本仍可由 Kernel 完成，
/// 新任务和领域入口不再取得 current。重新启用前再次校验当前版本完整性。
pub fn set_domain_pack_enabled(
    destination_root: &Path,
    system_id: &str,
    enabled: bool,
) -> Result<DomainPackState, String> {
    validate_system_id(system_id)?;
    let system_root = destination_root.join(system_id);
    let mut state = read_domain_pack_state(&system_root, system_id)?;
    if enabled {
        let current = state
            .current
            .as_ref()
            .ok_or_else(|| format!("DOMAIN_PACK_CURRENT_MISSING: {system_id}"))?;
        validate_installed_release(&system_root, system_id, current)?;
    }
    state.enabled = enabled;
    persist_domain_pack_state(&system_root, &state)?;
    Ok(state)
}

pub fn list_domain_pack_states(
    destination_root: &Path,
) -> Result<Vec<DomainPackStateView>, String> {
    if !destination_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut states = Vec::new();
    for entry in fs::read_dir(destination_root)
        .map_err(|error| format!("DOMAIN_PACK_ROOT_READ_FAILED: {error}"))?
    {
        let entry = entry.map_err(|error| format!("DOMAIN_PACK_ROOT_READ_FAILED: {error}"))?;
        if !entry.path().is_dir() {
            continue;
        }
        let system_id = entry.file_name().to_string_lossy().into_owned();
        validate_system_id(&system_id)?;
        states.push(domain_pack_state(destination_root, &system_id)?);
    }
    states.sort_by(|left, right| left.state.system_id.cmp(&right.state.system_id));
    Ok(states)
}

pub fn domain_pack_state(
    destination_root: &Path,
    system_id: &str,
) -> Result<DomainPackStateView, String> {
    validate_system_id(system_id)?;
    let system_root = destination_root.join(system_id);
    let state = read_domain_pack_state(&system_root, system_id)?;
    let release = state
        .current
        .as_ref()
        .or(state.candidate.as_ref())
        .or(state.lkg.as_ref());
    let changelog = match release {
        Some(release) => {
            validate_release_pointer(system_id, release)?;
            let path = system_root
                .join("releases")
                .join(&release.directory)
                .join("CHANGELOG.md");
            let metadata = fs::metadata(&path)
                .map_err(|error| format!("DOMAIN_PACK_CHANGELOG_METADATA_FAILED: {error}"))?;
            if metadata.len() > 1024 * 1024 {
                return Err("DOMAIN_PACK_CHANGELOG_TOO_LARGE: changelog exceeds 1 MiB".to_string());
            }
            fs::read_to_string(path)
                .map_err(|error| format!("DOMAIN_PACK_CHANGELOG_READ_FAILED: {error}"))?
        }
        None => String::new(),
    };
    Ok(DomainPackStateView { state, changelog })
}

fn validate_domain_pack(source: &Path) -> Result<DomainPackDescriptor, String> {
    for required in ["package.json", "domain.json", "README.md", "CHANGELOG.md"] {
        if !source.join(required).is_file() {
            return Err(format!(
                "DOMAIN_PACK_FILE_MISSING: {}",
                source.join(required).display()
            ));
        }
    }
    let descriptor: DomainPackDescriptor = serde_json::from_str(
        &fs::read_to_string(source.join("domain.json"))
            .map_err(|error| format!("DOMAIN_PACK_MANIFEST_READ_FAILED: {error}"))?,
    )
    .map_err(|error| format!("DOMAIN_PACK_MANIFEST_INVALID: {error}"))?;
    validate_system_id(&descriptor.system_id)?;
    if descriptor.kind != "domain" {
        return Err("DOMAIN_PACK_KIND_INVALID: expected domain".to_string());
    }
    let version = Version::parse(&descriptor.version)
        .map_err(|error| format!("DOMAIN_PACK_VERSION_INVALID: {error}"))?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err("DOMAIN_PACK_VERSION_INVALID: stable SemVer is required".to_string());
    }
    let requirement = VersionReq::parse(&descriptor.kernel_api_range)
        .map_err(|error| format!("DOMAIN_PACK_KERNEL_RANGE_INVALID: {error}"))?;
    let kernel = Version::parse(DOMAIN_KERNEL_VERSION)
        .map_err(|error| format!("DOMAIN_KERNEL_VERSION_INVALID: {error}"))?;
    if !requirement.matches(&kernel) {
        return Err(format!(
            "DOMAIN_PACK_KERNEL_INCOMPATIBLE: {} does not match {}",
            descriptor.kernel_api_range, DOMAIN_KERNEL_VERSION
        ));
    }
    VersionReq::parse(&descriptor.supported_engine_range)
        .map_err(|error| format!("DOMAIN_PACK_ENGINE_RANGE_INVALID: {error}"))?;
    if [
        descriptor.manifest_schema_version,
        descriptor.resource_schema_version,
        descriptor.capability_schema_version,
        descriptor.memory_schema_version,
    ]
    .iter()
    .any(|version| *version != 1)
    {
        return Err("DOMAIN_PACK_SCHEMA_UNSUPPORTED: only schema v1 is supported".to_string());
    }
    let package: Value = serde_json::from_str(
        &fs::read_to_string(source.join("package.json"))
            .map_err(|error| format!("DOMAIN_PACK_PACKAGE_READ_FAILED: {error}"))?,
    )
    .map_err(|error| format!("DOMAIN_PACK_PACKAGE_INVALID: {error}"))?;
    let expected_name = format!("@mir3-studio/domain-{}", descriptor.system_id);
    let package_files = package.get("files").and_then(Value::as_array);
    let includes_all_files = ["domain.json", "README.md", "CHANGELOG.md"]
        .iter()
        .all(|required| {
            package_files
                .is_some_and(|files| files.iter().any(|file| file.as_str() == Some(required)))
        });
    let package_domain = package.get("mir3Domain");
    if package.get("name").and_then(Value::as_str) != Some(expected_name.as_str())
        || package.get("version").and_then(Value::as_str) != Some(descriptor.version.as_str())
        || package.get("kind").and_then(Value::as_str) != Some("domain")
        || package_domain
            .and_then(|value| value.get("kind"))
            .and_then(Value::as_str)
            != Some("domain")
        || package_domain
            .and_then(|value| value.get("systemId"))
            .and_then(Value::as_str)
            != Some(descriptor.system_id.as_str())
        || package_domain
            .and_then(|value| value.get("kernelApiRange"))
            .and_then(Value::as_str)
            != Some(descriptor.kernel_api_range.as_str())
        || package_domain
            .and_then(|value| value.get("supportedEngineRange"))
            .and_then(Value::as_str)
            != Some(descriptor.supported_engine_range.as_str())
        || !includes_all_files
    {
        return Err(
            "DOMAIN_PACK_PACKAGE_MISMATCH: package and domain manifests differ".to_string(),
        );
    }
    let changelog = fs::read_to_string(source.join("CHANGELOG.md"))
        .map_err(|error| format!("DOMAIN_PACK_CHANGELOG_READ_FAILED: {error}"))?;
    if !changelog.contains(&format!("## {}", descriptor.version)) {
        return Err(format!(
            "DOMAIN_PACK_CHANGELOG_VERSION_MISSING: {}",
            descriptor.version
        ));
    }
    mir3_domain::execute_domain_pack_fixture_canary(
        source,
        &descriptor.system_id,
        &descriptor.version,
    )?;
    Ok(descriptor)
}

fn validate_system_id(system_id: &str) -> Result<(), String> {
    let valid = !system_id.is_empty()
        && system_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(format!("DOMAIN_PACK_SYSTEM_ID_INVALID: {system_id}"))
    }
}

fn validate_installed_release(
    system_root: &Path,
    system_id: &str,
    release: &DomainPackRelease,
) -> Result<(), String> {
    validate_release_pointer(system_id, release)?;
    let release_path = system_root.join("releases").join(&release.directory);
    let descriptor = validate_domain_pack(&release_path)?;
    if descriptor.system_id != system_id || descriptor.version != release.version {
        return Err(format!("DOMAIN_PACK_RELEASE_MISMATCH: {system_id}"));
    }
    let actual = hash_directory(&release_path)?;
    if actual != release.hash {
        return Err(format!("DOMAIN_PACK_RELEASE_HASH_MISMATCH: {system_id}"));
    }
    Ok(())
}

fn validate_release_pointer(system_id: &str, release: &DomainPackRelease) -> Result<(), String> {
    let version = Version::parse(&release.version)
        .map_err(|error| format!("DOMAIN_PACK_RELEASE_VERSION_INVALID: {error}"))?;
    let valid_hash = release.hash.len() == 64
        && release
            .hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    let expected_directory = release
        .hash
        .get(..12)
        .map(|hash| format!("{system_id}-{hash}"));
    if !version.pre.is_empty()
        || !version.build.is_empty()
        || !valid_hash
        || expected_directory.as_deref() != Some(release.directory.as_str())
    {
        return Err(format!("DOMAIN_PACK_RELEASE_POINTER_INVALID: {system_id}"));
    }
    Ok(())
}

fn read_domain_pack_state(system_root: &Path, system_id: &str) -> Result<DomainPackState, String> {
    let path = system_root.join("state.json");
    let backup = system_root.join(".state.previous");
    // 上次切换若在 Windows 的两次 rename 之间中断，优先恢复已 fsync 的旧状态。
    if !path.exists() && backup.is_file() {
        fs::rename(&backup, &path)
            .map_err(|error| format!("DOMAIN_PACK_STATE_RECOVERY_FAILED: {error}"))?;
    }
    if !path.is_file() {
        return Ok(DomainPackState {
            schema_version: DOMAIN_PACK_STATE_SCHEMA,
            system_id: system_id.to_string(),
            enabled: true,
            candidate: None,
            current: None,
            previous: None,
            lkg: None,
        });
    }
    let state: DomainPackState = serde_json::from_str(
        &fs::read_to_string(&path)
            .map_err(|error| format!("DOMAIN_PACK_STATE_READ_FAILED: {error}"))?,
    )
    .map_err(|error| format!("DOMAIN_PACK_STATE_INVALID: {error}"))?;
    if state.schema_version != DOMAIN_PACK_STATE_SCHEMA || state.system_id != system_id {
        return Err(format!("DOMAIN_PACK_STATE_INCOMPATIBLE: {system_id}"));
    }
    Ok(state)
}

fn default_enabled() -> bool {
    true
}

fn persist_domain_pack_state(system_root: &Path, state: &DomainPackState) -> Result<(), String> {
    fs::create_dir_all(system_root)
        .map_err(|error| format!("DOMAIN_PACK_STATE_ROOT_CREATE_FAILED: {error}"))?;
    let path = system_root.join("state.json");
    let pending = system_root.join(format!(".state-{}.pending", std::process::id()));
    let backup = system_root.join(".state.previous");
    remove_path_if_exists(&pending)?;
    let content = format!(
        "{}\n",
        serde_json::to_string_pretty(state)
            .map_err(|error| format!("DOMAIN_PACK_STATE_RENDER_FAILED: {error}"))?
    );
    let mut pending_file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&pending)
        .map_err(|error| format!("DOMAIN_PACK_STATE_WRITE_FAILED: {error}"))?;
    pending_file
        .write_all(content.as_bytes())
        .map_err(|error| format!("DOMAIN_PACK_STATE_WRITE_FAILED: {error}"))?;
    pending_file
        .sync_all()
        .map_err(|error| format!("DOMAIN_PACK_STATE_SYNC_FAILED: {error}"))?;
    drop(pending_file);
    remove_path_if_exists(&backup)?;
    if path.exists() {
        fs::rename(&path, &backup)
            .map_err(|error| format!("DOMAIN_PACK_STATE_BACKUP_FAILED: {error}"))?;
    }
    if let Err(error) = fs::rename(&pending, &path) {
        if backup.exists() {
            let _ = fs::rename(&backup, &path);
        }
        return Err(format!("DOMAIN_PACK_STATE_COMMIT_FAILED: {error}"));
    }
    remove_path_if_exists(&backup)?;
    Ok(())
}

fn hash_directory(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    for relative in files {
        digest.update(relative.to_string_lossy().replace('\\', "/").as_bytes());
        digest.update([0]);
        let mut file = File::open(root.join(&relative))
            .map_err(|error| format!("DOMAIN_PACK_HASH_READ_FAILED: {error}"))?;
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| format!("DOMAIN_PACK_HASH_READ_FAILED: {error}"))?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        digest.update([0]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(directory).map_err(|error| format!("DOMAIN_PACK_HASH_LIST_FAILED: {error}"))?
    {
        let entry = entry.map_err(|error| format!("DOMAIN_PACK_HASH_LIST_FAILED: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("DOMAIN_PACK_HASH_METADATA_FAILED: {error}"))?;
        let path = entry.path();
        if file_type.is_symlink() {
            return Err(format!("DOMAIN_PACK_SYMLINK_FORBIDDEN: {}", path.display()));
        }
        if file_type.is_dir() {
            collect_files(root, &path, files)?;
        } else if file_type.is_file() {
            files.push(
                path.strip_prefix(root)
                    .map_err(|error| format!("DOMAIN_PACK_HASH_PATH_FAILED: {error}"))?
                    .to_path_buf(),
            );
        }
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|error| format!("DOMAIN_PACK_REMOVE_FAILED: {error}"))?;
    } else if path.exists() {
        fs::remove_file(path).map_err(|error| format!("DOMAIN_PACK_REMOVE_FAILED: {error}"))?;
    }
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
    dependencies.remove(RETIRED_SAFE_FILES_PACKAGE);
    if let Some(plugins) = manifest
        .pointer_mut("/dsh/bundle/plugins")
        .and_then(Value::as_array_mut)
    {
        plugins.retain(|plugin| plugin.as_str() != Some(RETIRED_SAFE_FILES_PACKAGE));
    }
    let content = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("MIR3_SYSTEM_MANIFEST_RENDER_FAILED: {e}"))?;
    fs::write(path, format!("{content}\n"))
        .map_err(|e| format!("MIR3_SYSTEM_MANIFEST_WRITE_FAILED: {e}"))
}

/// 旧版第二 Harness 根插件已经并入 Core，启动时清理其 Profile 本地副本。
fn remove_retired_safe_files_plugin(profile: &Path) -> Result<(), String> {
    for path in [
        profile
            .join("node_modules")
            .join("@mir3-studio")
            .join("dsh-mir3-safe-files"),
        profile
            .join(".mir3-system-plugins")
            .join("dsh-mir3-safe-files"),
    ] {
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|error| {
                format!(
                    "MIR3_RETIRED_PLUGIN_REMOVE_FAILED: {}: {error}",
                    path.display()
                )
            })?;
        } else if path.exists() {
            fs::remove_file(&path).map_err(|error| {
                format!(
                    "MIR3_RETIRED_PLUGIN_REMOVE_FAILED: {}: {error}",
                    path.display()
                )
            })?;
        }
    }
    Ok(())
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
        fs::write(
            &path,
            r#"{"dependencies":{"@mir3-studio/dsh-mir3-safe-files":"file:retired"},"dsh":{"bundle":{"plugins":["regular","@mir3-studio/dsh-mir3-safe-files"]}}}"#,
        )
        .unwrap();
        ensure_manifest_dependency(&path).unwrap();
        let manifest: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            manifest.pointer("/dependencies/@mir3-studio~1dsh-mir3-core"),
            Some(&Value::String(LOCAL_PLUGIN_SPEC.to_string()))
        );
        assert_eq!(
            manifest.pointer("/dependencies/@mir3-studio~1dsh-mir3-safe-files"),
            None
        );
        assert_eq!(
            manifest.pointer("/dsh/bundle/plugins"),
            Some(&serde_json::json!(["regular"]))
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
        assert!(server_entry.contains("function apply(ctx)"));
        assert!(server_entry.contains("inject: ['sessions', 'sandboxPolicy']"));
        assert!(server_entry.contains("exec?.agent?.session"));
        assert!(server_entry.contains("MIR3_SYSTEM_SESSION_DRAFT_REQUIRED"));
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
        assert!(client_entry.contains("const PROTOCOL_VERSION = 2"));
        assert!(client_entry.contains("event.origin !== parentOrigin"));
        assert!(client_entry.contains("event.source !== window.parent"));
        assert!(client_entry.contains("ctx.sessions.create"));
        assert!(client_entry.contains("ctx.workspaces.archiveSession"));
        assert!(!client_entry.contains("postMessage({ source: 'mir3-core-plugin', version: 1"));
        assert!(!client_entry.contains("document.addEventListener('click'"));
    }

    #[test]
    fn domain_pack_candidate_activation_lkg_and_rollback_are_transactional() {
        let root = test_directory("domain-pack-lifecycle");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("mir3-domain-packs")
            .join("level");
        let (original_version, next_version) = test_pack_versions(&source);
        let first = stage_domain_pack_candidate(&root, &source).unwrap();
        assert!(first.current.is_none());
        assert_eq!(first.candidate.as_ref().unwrap().version, original_version);
        let active = activate_domain_pack_candidate(&root, "level").unwrap();
        assert_eq!(active.current.as_ref().unwrap().version, original_version);
        assert!(active.candidate.is_none());
        let stable = mark_domain_pack_lkg(&root, "level").unwrap();
        assert_eq!(stable.lkg, stable.current);
        let disabled = set_domain_pack_enabled(&root, "level", false).unwrap();
        assert!(!disabled.enabled);
        assert!(disabled.current.is_some());
        let enabled = set_domain_pack_enabled(&root, "level", true).unwrap();
        assert!(enabled.enabled);

        let next_source = root.join("next-source");
        copy_directory(&source, &next_source).unwrap();
        set_test_pack_version(&next_source, &next_version);
        let staged = stage_domain_pack_candidate(&root, &next_source).unwrap();
        assert_eq!(staged.current.as_ref().unwrap().version, original_version);
        assert_eq!(staged.candidate.as_ref().unwrap().version, next_version);
        let upgraded = activate_domain_pack_candidate(&root, "level").unwrap();
        assert_eq!(upgraded.current.as_ref().unwrap().version, next_version);
        assert_eq!(
            upgraded.previous.as_ref().unwrap().version,
            original_version
        );
        assert_eq!(upgraded.lkg.as_ref().unwrap().version, original_version);

        let restored = rollback_domain_pack(&root, "level").unwrap();
        assert_eq!(restored.current.as_ref().unwrap().version, original_version);
        assert_eq!(restored.previous.as_ref().unwrap().version, next_version);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn domain_pack_canary_failure_rolls_back_and_success_advances_lkg() {
        let base = test_directory("domain-pack-canary");
        let installed = base.join("installed");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("mir3-domain-packs")
            .join("level");
        let (original_version, next_version) = test_pack_versions(&source);
        let initial = stage_domain_pack_candidate(&installed, &source).unwrap();
        assert!(initial.current.is_none());
        activate_domain_pack_candidate(&installed, "level").unwrap();
        mark_domain_pack_lkg(&installed, "level").unwrap();

        let candidate = base.join("candidate");
        copy_directory(&source, &candidate).unwrap();
        set_test_pack_version(&candidate, &next_version);
        stage_domain_pack_candidate(&installed, &candidate).unwrap();
        let error = activate_domain_pack_with_canary(&installed, "level", |_| {
            Err("fixture canary rejected".to_string())
        })
        .unwrap_err();
        assert!(error.starts_with("DOMAIN_PACK_RUNTIME_CANARY_FAILED:"));
        assert!(error.ends_with("rollback=ok"));
        let restored = read_domain_pack_state(&installed.join("level"), "level").unwrap();
        assert_eq!(restored.current.as_ref().unwrap().version, original_version);
        assert_eq!(restored.lkg.as_ref().unwrap().version, original_version);

        stage_domain_pack_candidate(&installed, &candidate).unwrap();
        let stable = activate_domain_pack_with_canary(&installed, "level", |state| {
            assert_eq!(state.current.as_ref().unwrap().version, next_version);
            Ok(())
        })
        .unwrap();
        assert_eq!(stable.current.as_ref().unwrap().version, next_version);
        assert_eq!(stable.lkg, stable.current);
        assert_eq!(stable.previous.as_ref().unwrap().version, original_version);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn domain_pack_activation_rejects_tampered_release() {
        let root = test_directory("domain-pack-tamper");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("mir3-domain-packs")
            .join("shop");
        let staged = stage_domain_pack_candidate(&root, &source).unwrap();
        let candidate = staged.candidate.unwrap();
        fs::write(
            root.join("shop")
                .join("releases")
                .join(candidate.directory)
                .join("README.md"),
            "tampered",
        )
        .unwrap();
        let error = activate_domain_pack_candidate(&root, "shop").unwrap_err();
        assert!(error.starts_with("DOMAIN_PACK_RELEASE_HASH_MISMATCH:"));
        let state = read_domain_pack_state(&root.join("shop"), "shop").unwrap();
        assert!(state.current.is_none());
        assert!(state.candidate.is_some());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn semantically_tampered_candidate_fixture_never_changes_current() {
        let base = test_directory("domain-pack-fixture-tamper");
        let installed = base.join("installed");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("mir3-domain-packs")
            .join("level");
        let original_version = Version::parse(
            serde_json::from_str::<Value>(&fs::read_to_string(source.join("domain.json")).unwrap())
                .unwrap()["version"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let next_version = Version::new(
            original_version.major,
            original_version.minor,
            original_version.patch + 1,
        )
        .to_string();
        stage_domain_pack_candidate(&installed, &source).unwrap();
        activate_domain_pack_candidate(&installed, "level").unwrap();
        mark_domain_pack_lkg(&installed, "level").unwrap();

        let candidate = base.join("candidate");
        copy_directory(&source, &candidate).unwrap();
        set_test_pack_version(&candidate, &next_version);
        let valid_path = candidate.join("fixtures/valid.json");
        let mut valid: Value =
            serde_json::from_str(&fs::read_to_string(&valid_path).unwrap()).unwrap();
        valid["records"][0]["level"] = Value::from(999);
        fs::write(
            &valid_path,
            format!("{}\n", serde_json::to_string_pretty(&valid).unwrap()),
        )
        .unwrap();

        let error = stage_domain_pack_candidate(&installed, &candidate).unwrap_err();
        assert!(error.starts_with("DOMAIN_PACK_FIXTURE_VALID_REJECTED:"));
        let state = read_domain_pack_state(&installed.join("level"), "level").unwrap();
        assert_eq!(
            state.current.as_ref().unwrap().version,
            original_version.to_string()
        );
        assert_eq!(state.lkg, state.current);
        assert!(state.candidate.is_none());
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn domain_pack_release_pointer_rejects_path_traversal() {
        let release = DomainPackRelease {
            version: "1.0.0".to_string(),
            hash: "0".repeat(64),
            directory: "../../outside".to_string(),
        };
        assert_eq!(
            validate_release_pointer("shop", &release),
            Err("DOMAIN_PACK_RELEASE_POINTER_INVALID: shop".to_string())
        );
    }

    #[test]
    fn domain_pack_state_recovers_interrupted_pointer_swap() {
        let root = test_directory("domain-pack-state-recovery").join("level");
        let state = DomainPackState {
            schema_version: DOMAIN_PACK_STATE_SCHEMA,
            system_id: "level".to_string(),
            enabled: true,
            candidate: None,
            current: None,
            previous: None,
            lkg: None,
        };
        persist_domain_pack_state(&root, &state).unwrap();
        fs::rename(root.join("state.json"), root.join(".state.previous")).unwrap();
        assert_eq!(read_domain_pack_state(&root, "level").unwrap(), state);
        assert!(root.join("state.json").is_file());
        fs::remove_dir_all(root.parent().unwrap()).ok();
    }

    #[test]
    fn all_bundled_domain_packs_install_with_initial_lkg() {
        let root = test_directory("domain-pack-bundle");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("mir3-domain-packs");
        ensure_domain_pack_root(&source, &root).unwrap();
        let installed = fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .count();
        assert_eq!(installed, 33);
        let listed = list_domain_pack_states(&root).unwrap();
        assert_eq!(listed.len(), 33);
        assert!(listed.iter().all(|view| view.changelog.contains("1.0.0")));
        for system_id in ["level", "map", "shop", "cross_server"] {
            let state = read_domain_pack_state(&root.join(system_id), system_id).unwrap();
            assert_eq!(state.current, state.lkg);
            assert!(state.current.is_some());
            assert!(state.candidate.is_none());
        }
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn all_33_domain_packs_support_disable_upgrade_and_rollback() {
        let base = test_directory("domain-pack-full-matrix");
        let installed = base.join("installed");
        let candidates = base.join("candidates");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("mir3-domain-packs");
        ensure_domain_pack_root(&source, &installed).unwrap();
        fs::create_dir_all(&candidates).unwrap();
        let registry: Value =
            serde_json::from_str(&fs::read_to_string(source.join("registry.json")).unwrap())
                .unwrap();
        let packs = registry["packs"].as_array().unwrap();
        assert_eq!(packs.len(), 33);

        for pack in packs {
            let system_id = pack["systemId"].as_str().unwrap();
            let original_version = Version::parse(pack["version"].as_str().unwrap()).unwrap();
            let next_version = Version::new(
                original_version.major,
                original_version.minor,
                original_version.patch + 1,
            )
            .to_string();
            let disabled = set_domain_pack_enabled(&installed, system_id, false).unwrap();
            assert!(!disabled.enabled, "{system_id} did not disable");
            let enabled = set_domain_pack_enabled(&installed, system_id, true).unwrap();
            assert!(enabled.enabled, "{system_id} did not re-enable");

            let candidate = candidates.join(system_id);
            copy_directory(&source.join(system_id), &candidate).unwrap();
            set_test_pack_version(&candidate, &next_version);
            let staged = stage_domain_pack_candidate(&installed, &candidate).unwrap();
            assert_eq!(staged.candidate.as_ref().unwrap().version, next_version);
            let upgraded = activate_domain_pack_candidate(&installed, system_id).unwrap();
            assert_eq!(upgraded.current.as_ref().unwrap().version, next_version);
            assert_eq!(
                upgraded.previous.as_ref().unwrap().version,
                original_version.to_string()
            );
            let rolled_back = rollback_domain_pack(&installed, system_id).unwrap();
            assert_eq!(
                rolled_back.current.as_ref().unwrap().version,
                original_version.to_string()
            );
            assert_eq!(rolled_back.previous.as_ref().unwrap().version, next_version);
        }
        assert_eq!(list_domain_pack_states(&installed).unwrap().len(), 33);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn corrupt_or_disabled_pack_is_isolated_and_reported_as_a_missing_dependency() {
        let base = test_directory("domain-pack-isolation");
        let installed = base.join("installed");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("mir3-domain-packs");
        ensure_domain_pack_root(&source, &installed).unwrap();

        let registry: Value =
            serde_json::from_str(&fs::read_to_string(source.join("registry.json")).unwrap())
                .unwrap();
        let system_ids = registry["packs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pack| pack["systemId"].as_str().unwrap().to_string())
            .collect::<BTreeSet<_>>();
        assert_eq!(system_ids.len(), 33);
        for system_id in &system_ids {
            let state = read_domain_pack_state(&installed.join(system_id), system_id).unwrap();
            let release = state.current.unwrap();
            let readme = installed
                .join(system_id)
                .join("releases")
                .join(&release.directory)
                .join("README.md");
            let original = fs::read(&readme).unwrap();
            fs::write(&readme, "corrupted").unwrap();

            let store = mir3_domain::DomainStore::new_with_domain_pack_root(
                base.join(format!("data-corrupt-{system_id}")),
                &installed,
            )
            .unwrap();
            let active = store
                .list_domain_systems()
                .unwrap()
                .into_iter()
                .map(|pack| pack.system_id)
                .collect::<BTreeSet<_>>();
            let expected = system_ids
                .iter()
                .filter(|candidate| *candidate != system_id)
                .cloned()
                .collect::<BTreeSet<_>>();
            assert_eq!(active, expected, "{system_id} corruption was not isolated");
            if system_id == "monster" {
                let dependency = store.resolve_domain_dependencies("level").unwrap();
                assert!(dependency.missing.contains(&"monster".to_string()));
                assert!(!dependency.cycles.is_empty());
                assert!(store.resolve_domain_dependencies("shop").is_ok());
            }
            fs::write(&readme, original).unwrap();
            validate_installed_release(&installed.join(system_id), system_id, &release).unwrap();
        }

        let clean_installed = base.join("installed-disabled");
        ensure_domain_pack_root(&source, &clean_installed).unwrap();
        set_domain_pack_enabled(&clean_installed, "monster", false).unwrap();
        let disabled_store = mir3_domain::DomainStore::new_with_domain_pack_root(
            base.join("data-disabled"),
            &clean_installed,
        )
        .unwrap();
        assert_eq!(disabled_store.list_domain_systems().unwrap().len(), 32);
        assert!(disabled_store
            .resolve_domain_dependencies("level")
            .unwrap()
            .missing
            .contains(&"monster".to_string()));
        fs::remove_dir_all(base).ok();
    }

    fn test_directory(label: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("mir3-{label}-{}-{suffix}", std::process::id()))
    }

    fn test_pack_versions(root: &Path) -> (String, String) {
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(root.join("domain.json")).unwrap()).unwrap();
        let original = Version::parse(manifest["version"].as_str().unwrap()).unwrap();
        let next = Version::new(original.major, original.minor, original.patch + 1).to_string();
        (original.to_string(), next)
    }

    fn set_test_pack_version(root: &Path, version: &str) {
        for file in ["package.json", "domain.json"] {
            let path = root.join(file);
            let mut value: Value =
                serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            value["version"] = Value::String(version.to_string());
            fs::write(
                &path,
                format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
            )
            .unwrap();
        }
        fs::write(
            root.join("CHANGELOG.md"),
            format!("# Changelog\n\n## {version}\n\n- Test release.\n"),
        )
        .unwrap();
    }
}
