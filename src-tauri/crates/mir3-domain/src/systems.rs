//! 33 个领域包的稳定契约、真实文件投影与通用能力目录。
//!
//! 领域包只描述安全的资源与操作，不拥有 Harness 生命周期，也不能直接写项目。

use crate::DomainStore;
use rusqlite::{params, OptionalExtension};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const REGISTRY_JSON: &str = include_str!("../../../resources/mir3-domain-packs/registry.json");
static REGISTRY: OnceLock<DomainRegistry> = OnceLock::new();
const DOMAIN_PACK_STATE_SCHEMA: u32 = 1;
const DOMAIN_KERNEL_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct RuntimeDomainPackRelease {
    version: String,
    hash: String,
    directory: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeDomainPackState {
    schema_version: u32,
    system_id: String,
    #[serde(default = "default_runtime_pack_enabled")]
    enabled: bool,
    current: Option<RuntimeDomainPackRelease>,
    previous: Option<RuntimeDomainPackRelease>,
    lkg: Option<RuntimeDomainPackRelease>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainRegistry {
    pub schema_version: u32,
    pub packs: Vec<DomainManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainManifest {
    pub kind: String,
    pub system_id: String,
    pub version: String,
    pub kernel_api_range: String,
    pub supported_engine_range: String,
    #[serde(default)]
    pub engine_compatibility: DomainEngineCompatibility,
    pub manifest_schema_version: u32,
    pub resource_schema_version: u32,
    pub capability_schema_version: u32,
    pub memory_schema_version: u32,
    pub category: String,
    pub complexity: u8,
    pub renderer: String,
    #[serde(default)]
    pub documentation: DomainDocumentation,
    #[serde(default)]
    pub required_kernel_primitives: Vec<String>,
    pub file_projection: FileProjection,
    #[serde(default)]
    pub resources: DomainResourcesContract,
    #[serde(default)]
    pub presentation: DomainPresentation,
    #[serde(default)]
    pub operations: Vec<DomainOperationContract>,
    #[serde(default)]
    pub validators: Vec<DomainValidatorContract>,
    #[serde(default)]
    pub fixtures: DomainFixturesContract,
    pub dependencies: Vec<String>,
    pub capabilities: Vec<OfficialCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainEngineCompatibility {
    /// 只允许 Kernel 已实现且可审计的版本归一化策略。
    #[serde(default)]
    pub strategy: String,
    #[serde(default)]
    pub version_aliases: Vec<String>,
    #[serde(default)]
    pub required_evidence: Vec<String>,
    #[serde(default)]
    pub unknown_version_policy: String,
    #[serde(default)]
    pub incompatible_version_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainFixturesContract {
    #[serde(default)]
    pub valid: String,
    #[serde(default)]
    pub invalid: String,
    #[serde(default)]
    pub expected_diagnostics: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProjection {
    /// 兼容首版领域包的简写选择器。
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub owned_selectors: Vec<String>,
    #[serde(default)]
    pub dependency_selectors: Vec<DomainDependencySelector>,
    #[serde(default)]
    pub excludes: Vec<String>,
    #[serde(default)]
    pub content_fingerprints: Vec<DomainContentFingerprint>,
    #[serde(default)]
    pub path_aliases: Vec<DomainPathAlias>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub editable_extensions: Vec<String>,
    #[serde(default)]
    pub structured_extensions: Vec<String>,
    #[serde(default)]
    pub readonly_extensions: Vec<String>,
    #[serde(default)]
    pub unknown_format_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainDependencySelector {
    pub system_id: String,
    #[serde(default)]
    pub selectors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainContentFingerprint {
    pub contains: String,
    #[serde(default)]
    pub case_sensitive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainPathAlias {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainDocumentation {
    pub readme: String,
    pub changelog: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainResourcesContract {
    #[serde(default)]
    pub resource_types: Vec<String>,
    #[serde(default)]
    pub stable_resource_id: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub mappings: Vec<String>,
    #[serde(default)]
    pub dependency_edges: Vec<DomainDependencyEdge>,
    #[serde(default)]
    pub unique_key: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainDependencyEdge {
    pub field: String,
    pub system_id: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainPresentation {
    #[serde(default)]
    pub views: Vec<String>,
    #[serde(default)]
    pub primary_view: String,
    #[serde(default)]
    pub safe_primitive: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainOperationContract {
    pub id: String,
    #[serde(default)]
    pub parameter_schema: serde_json::Value,
    #[serde(default)]
    pub read_systems: Vec<String>,
    #[serde(default)]
    pub write_systems: Vec<String>,
    #[serde(default)]
    pub preconditions: Vec<String>,
    #[serde(default)]
    pub steps: Vec<DomainCapabilityStep>,
    #[serde(default)]
    pub reversible: bool,
    #[serde(default)]
    pub preview_policy: DomainPreviewPolicy,
    #[serde(default)]
    pub unknown_format_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainValidatorContract {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub encoding: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub resource_type: String,
    #[serde(default)]
    pub fields: serde_json::Value,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub references: Vec<DomainDependencyEdge>,
    #[serde(default)]
    pub match_by: String,
    #[serde(default)]
    pub compare_fields: Vec<String>,
    #[serde(default)]
    pub missing_projection: String,
    #[serde(default)]
    pub rule: String,
    #[serde(default)]
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialCapability {
    pub id: String,
    pub version: String,
    pub preview_required: bool,
    pub validation_required: bool,
    pub confirmation_required: bool,
    #[serde(default)]
    pub parameter_schema: serde_json::Value,
    #[serde(default)]
    pub read_systems: Vec<String>,
    #[serde(default)]
    pub write_systems: Vec<String>,
    #[serde(default)]
    pub preconditions: Vec<String>,
    #[serde(default)]
    pub steps: Vec<DomainCapabilityStep>,
    #[serde(default)]
    pub reversible: bool,
    #[serde(default)]
    pub preview_policy: DomainPreviewPolicy,
    #[serde(default)]
    pub unknown_format_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainCapabilityStep {
    #[serde(default)]
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub operation: String,
    #[serde(default)]
    pub primitive: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub schema: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainPreviewPolicy {
    pub preview_required: bool,
    pub validation_required: bool,
    pub confirmation_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainFileQuery {
    pub text: String,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainFileRecord {
    pub path: String,
    pub role: String,
    pub category: String,
    pub extension: Option<String>,
    pub size: u64,
    pub modified_at: i64,
    pub resource_id: String,
    pub ownership: String,
    pub access: String,
    pub systems: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainResourceRecord {
    pub id: String,
    pub system_id: String,
    pub resource_type: String,
    pub label: String,
    pub files: Vec<DomainFileRecord>,
    pub dependency_systems: Vec<String>,
    pub writable: bool,
    pub projection: serde_json::Value,
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub fields: serde_json::Map<String, serde_json::Value>,
    pub source: crate::DomainResourceSource,
    #[serde(default)]
    pub dependencies: Vec<crate::DomainResourceDependency>,
    #[serde(default)]
    pub mappings_applied: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainDependencyGraph {
    pub system_id: String,
    pub direct: Vec<String>,
    pub transitive: Vec<String>,
    pub missing: Vec<String>,
    pub cycles: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainSystemDescription {
    pub manifest: DomainManifest,
    pub owned_files: usize,
    pub shared_files: usize,
    pub writable_files: usize,
    pub readonly_files: usize,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainValidationReport {
    pub system_id: String,
    pub valid: bool,
    pub owned_files: usize,
    pub writable_files: usize,
    pub readonly_files: usize,
    pub missing_dependencies: Vec<String>,
    pub validators: Vec<DomainValidatorResult>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainValidatorResult {
    pub id: String,
    pub kind: String,
    pub valid: bool,
    pub checked: usize,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
struct DomainDraftOverlay {
    draft_id: String,
    revision: i64,
    changes: BTreeMap<String, Option<Vec<u8>>>,
}

#[derive(Debug, Clone)]
struct DomainValidationFile {
    record: DomainFileRecord,
    content: Option<String>,
    syntax_error: Option<String>,
}

pub fn bundled_domain_registry() -> Result<&'static DomainRegistry, String> {
    if let Some(registry) = REGISTRY.get() {
        return Ok(registry);
    }
    let parsed: DomainRegistry = serde_json::from_str(REGISTRY_JSON)
        .map_err(|error| format!("DOMAIN_REGISTRY_INVALID: {error}"))?;
    validate_registry(&parsed)?;
    let _ = REGISTRY.set(parsed);
    REGISTRY
        .get()
        .ok_or_else(|| "DOMAIN_REGISTRY_UNAVAILABLE: registry initialization failed".to_string())
}

/// 使用与 Kernel 相同的完整契约校验领域包，供安装器在切换指针前执行 canary。
pub fn validate_domain_pack_manifest(
    path: &Path,
    expected_system_id: &str,
    expected_version: &str,
) -> Result<DomainManifest, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("DOMAIN_PACK_MANIFEST_READ_FAILED: {error}"))?;
    let manifest: DomainManifest = serde_json::from_str(&content)
        .map_err(|error| format!("DOMAIN_PACK_MANIFEST_INVALID: {error}"))?;
    if manifest.system_id != expected_system_id || manifest.version != expected_version {
        return Err(format!(
            "DOMAIN_PACK_RELEASE_MISMATCH: expected {expected_system_id}@{expected_version}, got {}@{}",
            manifest.system_id, manifest.version
        ));
    }
    let mut registry = bundled_domain_registry()?.clone();
    let target = registry
        .packs
        .iter_mut()
        .find(|pack| pack.system_id == expected_system_id)
        .ok_or_else(|| format!("DOMAIN_SYSTEM_NOT_FOUND: {expected_system_id}"))?;
    *target = manifest.clone();
    validate_registry(&registry)?;
    Ok(manifest)
}

impl DomainStore {
    /// 运行时只信任版本状态指针与完整目录哈希；current 损坏时只退到 LKG。
    pub(crate) fn runtime_manifest(&self, system_id: &str) -> Result<DomainManifest, String> {
        self.runtime_manifest_at_version(system_id, None)
    }

    /// Draft 与任务通过固定版本读取契约，避免升级后静默换用新操作和校验规则。
    pub(crate) fn runtime_manifest_at_version(
        &self,
        system_id: &str,
        version: Option<&str>,
    ) -> Result<DomainManifest, String> {
        let packs_root = self.domain_pack_root();
        if !packs_root.is_dir() {
            let manifest = bundled_domain_registry()?
                .packs
                .iter()
                .find(|manifest| manifest.system_id == system_id)
                .cloned()
                .ok_or_else(|| format!("DOMAIN_SYSTEM_NOT_FOUND: {system_id}"))?;
            if version.is_some_and(|expected| expected != manifest.version) {
                return Err(format!(
                    "DOMAIN_PACK_VERSION_UNAVAILABLE: {system_id}@{}",
                    version.unwrap_or_default()
                ));
            }
            return Ok(manifest);
        }
        let system_root = packs_root.join(system_id);
        let state_path = system_root.join("state.json");
        let state: RuntimeDomainPackState = serde_json::from_str(
            &fs::read_to_string(&state_path)
                .map_err(|error| format!("DOMAIN_PACK_STATE_READ_FAILED: {system_id}: {error}"))?,
        )
        .map_err(|error| format!("DOMAIN_PACK_STATE_INVALID: {system_id}: {error}"))?;
        if state.schema_version != DOMAIN_PACK_STATE_SCHEMA || state.system_id != system_id {
            return Err(format!("DOMAIN_PACK_STATE_INCOMPATIBLE: {system_id}"));
        }
        // 显式固定版本代表已经运行中的任务/Draft；禁用只阻止新任务取得 current，
        // 不在提交途中热切换旧任务的契约。
        if !state.enabled && version.is_none() {
            return Err(format!("DOMAIN_PACK_DISABLED: {system_id}"));
        }

        let releases = if let Some(expected) = version {
            [&state.current, &state.lkg, &state.previous]
                .into_iter()
                .flatten()
                .filter(|release| release.version == expected)
                .collect::<Vec<_>>()
        } else {
            [&state.current, &state.lkg]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
        };
        if releases.is_empty() {
            if let Some(expected) = version {
                return load_historical_runtime_release(&system_root, system_id, expected);
            }
            return Err(format!(
                "DOMAIN_PACK_VERSION_UNAVAILABLE: {system_id}@{}",
                version.unwrap_or("current")
            ));
        }
        let mut failures = Vec::new();
        for release in releases {
            match load_runtime_release(&system_root, system_id, release) {
                Ok(manifest) => return Ok(manifest),
                Err(error) => failures.push(error),
            }
        }
        Err(format!(
            "DOMAIN_PACK_RUNTIME_UNAVAILABLE: {system_id}: {}",
            failures.join(" | ")
        ))
    }

    fn runtime_domain_registry(&self) -> Result<DomainRegistry, String> {
        let packs_root = self.domain_pack_root();
        if !packs_root.is_dir() {
            return Ok(bundled_domain_registry()?.clone());
        }
        let baseline = bundled_domain_registry()?;
        let mut packs = Vec::with_capacity(baseline.packs.len());
        for expected in &baseline.packs {
            // 单包 current 与 LKG 都损坏时关闭该包，不拖垮其他领域。
            if let Ok(manifest) = self.runtime_manifest(&expected.system_id) {
                packs.push(manifest);
            }
        }
        if packs.is_empty() {
            return Err("DOMAIN_REGISTRY_UNAVAILABLE: no verified active domain packs".to_string());
        }
        Ok(DomainRegistry {
            schema_version: baseline.schema_version,
            packs,
        })
    }
}

fn default_runtime_pack_enabled() -> bool {
    true
}

/// 已离开 current/previous/LKG 的版本仍可能被运行中任务固定使用。
/// 历史目录没有可变指针，因此必须重新计算完整哈希、核对内容寻址目录，
/// 并再次执行完整 manifest 契约校验；同版本存在多个有效目录时拒绝猜测。
fn load_historical_runtime_release(
    system_root: &Path,
    system_id: &str,
    expected_version: &str,
) -> Result<DomainManifest, String> {
    Version::parse(expected_version)
        .map_err(|error| format!("DOMAIN_PACK_RELEASE_VERSION_INVALID: {error}"))?;
    let releases_root = system_root.join("releases");
    let entries = fs::read_dir(&releases_root).map_err(|error| {
        format!(
            "DOMAIN_PACK_RELEASES_READ_FAILED: {}: {error}",
            releases_root.display()
        )
    })?;
    let mut matches = Vec::new();
    let mut failures = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("DOMAIN_PACK_RELEASES_READ_FAILED: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("DOMAIN_PACK_HASH_METADATA_FAILED: {error}"))?;
        if file_type.is_symlink() {
            failures.push(format!(
                "DOMAIN_PACK_SYMLINK_FORBIDDEN: {}",
                entry.path().display()
            ));
            continue;
        }
        if !file_type.is_dir() {
            continue;
        }
        let manifest_path = entry.path().join("domain.json");
        let manifest: DomainManifest = match fs::read_to_string(&manifest_path)
            .map_err(|error| format!("DOMAIN_PACK_MANIFEST_READ_FAILED: {error}"))
            .and_then(|content| {
                serde_json::from_str(&content)
                    .map_err(|error| format!("DOMAIN_PACK_MANIFEST_INVALID: {error}"))
            }) {
            Ok(manifest) => manifest,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        if manifest.system_id != system_id || manifest.version != expected_version {
            continue;
        }
        let actual_hash = match hash_runtime_release(&entry.path()) {
            Ok(hash) => hash,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        let directory = entry.file_name().to_string_lossy().into_owned();
        let release = RuntimeDomainPackRelease {
            version: expected_version.to_string(),
            hash: actual_hash,
            directory,
        };
        match load_runtime_release(system_root, system_id, &release) {
            Ok(manifest) => matches.push(manifest),
            Err(error) => failures.push(error),
        }
    }
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(format!(
            "DOMAIN_PACK_VERSION_UNAVAILABLE: {system_id}@{expected_version}: {}",
            failures.join(" | ")
        )),
        _ => Err(format!(
            "DOMAIN_PACK_VERSION_AMBIGUOUS: {system_id}@{expected_version} has multiple verified releases"
        )),
    }
}

fn load_runtime_release(
    system_root: &Path,
    system_id: &str,
    release: &RuntimeDomainPackRelease,
) -> Result<DomainManifest, String> {
    validate_runtime_release_pointer(system_id, release)?;
    let release_root = system_root.join("releases").join(&release.directory);
    let actual_hash = hash_runtime_release(&release_root)?;
    if actual_hash != release.hash {
        return Err(format!("DOMAIN_PACK_RELEASE_HASH_MISMATCH: {system_id}"));
    }
    validate_domain_pack_manifest(
        &release_root.join("domain.json"),
        system_id,
        &release.version,
    )
}

fn validate_runtime_release_pointer(
    system_id: &str,
    release: &RuntimeDomainPackRelease,
) -> Result<(), String> {
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

fn hash_runtime_release(root: &Path) -> Result<String, String> {
    if !root.is_dir() {
        return Err(format!("DOMAIN_PACK_RELEASE_MISSING: {}", root.display()));
    }
    let mut files = Vec::new();
    collect_runtime_release_files(root, root, &mut files)?;
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

fn collect_runtime_release_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
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
            collect_runtime_release_files(root, &path, files)?;
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

impl DomainStore {
    pub fn list_domain_systems(&self) -> Result<Vec<DomainManifest>, String> {
        Ok(self.runtime_domain_registry()?.packs)
    }

    pub fn describe_domain_system(
        &self,
        project_id: &str,
        system_id: &str,
    ) -> Result<DomainSystemDescription, String> {
        let manifest = self.runtime_manifest(system_id)?;
        let files = self.query_domain_files(
            project_id,
            system_id,
            &DomainFileQuery {
                text: String::new(),
                limit: Some(10_000),
                offset: None,
            },
        )?;
        let owned_files = files
            .iter()
            .filter(|file| file.ownership == "owned")
            .count();
        let shared_files = files
            .iter()
            .filter(|file| file.ownership == "shared")
            .count();
        let writable_files = files
            .iter()
            .filter(|file| file.access != "readonly")
            .count();
        let readonly_files = files.len().saturating_sub(writable_files);
        let engine_compatibility = self.assert_project_engine_compatible(project_id, &manifest);
        let mut diagnostics = Vec::new();
        if let Err(error) = &engine_compatibility {
            diagnostics.push(error.clone());
        }
        if let Some(reason) = self.read_only_reason() {
            diagnostics.push(format!("DOMAIN_KERNEL_READONLY:{reason}"));
        }
        if files.is_empty() {
            diagnostics.push("DOMAIN_FILES_NOT_DETECTED".to_string());
        }
        if writable_files == 0 && !files.is_empty() {
            diagnostics.push("DOMAIN_VERIFIED_WRITABLE_SCHEMA_NOT_DETECTED".to_string());
        }
        Ok(DomainSystemDescription {
            manifest,
            owned_files,
            shared_files,
            writable_files,
            readonly_files,
            diagnostics,
        })
    }

    pub fn query_domain_files(
        &self,
        project_id: &str,
        system_id: &str,
        query: &DomainFileQuery,
    ) -> Result<Vec<DomainFileRecord>, String> {
        let registry = self.runtime_domain_registry()?;
        let manifest = registry
            .packs
            .iter()
            .find(|manifest| manifest.system_id == system_id)
            .ok_or_else(|| format!("DOMAIN_SYSTEM_NOT_FOUND: {system_id}"))?;
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT path,role,category,extension,size,modified_at,content FROM files
                 WHERE (?1='' OR path LIKE ?2 ESCAPE '\\') ORDER BY path",
            )
            .map_err(|error| format!("DOMAIN_FILE_QUERY_FAILED: {error}"))?;
        let text = query.text.trim();
        let pattern = format!("%{}%", text.replace('%', "\\%").replace('_', "\\_"));
        let rows = statement
            .query_map(params![text, pattern], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })
            .map_err(|error| format!("DOMAIN_FILE_QUERY_FAILED: {error}"))?;
        let offset = query.offset.unwrap_or_default();
        let limit = query.limit.unwrap_or(250).clamp(1, 10_000);
        let mut matched = Vec::new();
        for row in rows {
            let (path, role, category, extension, size, modified_at, content) =
                row.map_err(|error| format!("DOMAIN_FILE_QUERY_FAILED: {error}"))?;
            let owns_file =
                matches_projection(manifest, &path, extension.as_deref(), content.as_deref());
            let dependency_file = !owns_file
                && registry.packs.iter().any(|candidate| {
                    manifest.dependencies.contains(&candidate.system_id)
                        && matches_projection(
                            candidate,
                            &path,
                            extension.as_deref(),
                            content.as_deref(),
                        )
                });
            if !owns_file && !dependency_file {
                continue;
            }
            let systems = registry
                .packs
                .iter()
                .filter(|candidate| {
                    matches_projection(candidate, &path, extension.as_deref(), content.as_deref())
                })
                .map(|candidate| candidate.system_id.clone())
                .collect::<Vec<_>>();
            let ownership = if dependency_file {
                "dependency"
            } else if systems.len() > 1 {
                "shared"
            } else {
                "owned"
            };
            let access = if dependency_file || self.read_only_reason().is_some() {
                "readonly"
            } else {
                self.verified_access_for(project_id, manifest, &path, extension.as_deref())
            };
            let resource_manifest = systems
                .first()
                .and_then(|owner| registry.packs.iter().find(|pack| pack.system_id == *owner))
                .unwrap_or(manifest);
            matched.push(DomainFileRecord {
                resource_id: stable_resource_id(resource_manifest, &path, content.as_deref()),
                path,
                role,
                category,
                extension,
                size: size.max(0) as u64,
                modified_at,
                ownership: ownership.to_string(),
                access: access.to_string(),
                systems,
            });
        }
        Ok(matched.into_iter().skip(offset).take(limit).collect())
    }

    pub fn query_unclaimed_domain_files(
        &self,
        project_id: &str,
        query: &DomainFileQuery,
    ) -> Result<Vec<DomainFileRecord>, String> {
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT path,role,category,extension,size,modified_at,content FROM files
                 WHERE (?1='' OR path LIKE ?2 ESCAPE '\\') ORDER BY path",
            )
            .map_err(|error| format!("DOMAIN_FILE_QUERY_FAILED: {error}"))?;
        let text = query.text.trim();
        let pattern = format!("%{}%", text.replace('%', "\\%").replace('_', "\\_"));
        let rows = statement
            .query_map(params![text, pattern], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })
            .map_err(|error| format!("DOMAIN_FILE_QUERY_FAILED: {error}"))?;
        let registry = self.runtime_domain_registry()?;
        let offset = query.offset.unwrap_or_default();
        let limit = query.limit.unwrap_or(250).clamp(1, 10_000);
        let mut files = Vec::new();
        for row in rows {
            let (path, role, category, extension, size, modified_at, content) =
                row.map_err(|error| format!("DOMAIN_FILE_QUERY_FAILED: {error}"))?;
            if registry.packs.iter().any(|manifest| {
                matches_projection(manifest, &path, extension.as_deref(), content.as_deref())
            }) {
                continue;
            }
            files.push(DomainFileRecord {
                resource_id: stable_unknown_resource_id(&path),
                path,
                role,
                category,
                extension,
                size: size.max(0) as u64,
                modified_at,
                ownership: "unknown".to_string(),
                access: "readonly".to_string(),
                systems: Vec::new(),
            });
        }
        Ok(files.into_iter().skip(offset).take(limit).collect())
    }

    pub fn validate_domain_system(
        &self,
        project_id: &str,
        system_id: &str,
    ) -> Result<DomainValidationReport, String> {
        let manifest = self.runtime_manifest(system_id)?;
        self.validate_domain_manifest(project_id, &manifest, None)
    }

    /// 使用 Draft 覆盖层校验固定版本领域内容；不会读取 active 版本替代绑定版本，
    /// 也不会把正式索引报告冒充为 Draft 校验结果。
    pub fn validate_domain_draft(
        &self,
        project_id: &str,
        draft_id: &str,
    ) -> Result<DomainValidationReport, String> {
        let draft = self.get_draft(project_id, draft_id)?;
        let binding = self
            .project_connection(project_id)?
            .query_row(
                "SELECT system_id,plugin_version,legacy FROM draft_domains WHERE draft_id=?1",
                [draft_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)? != 0,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("DRAFT_DOMAIN_READ_FAILED: {error}"))?
            .ok_or_else(|| "DRAFT_DOMAIN_REQUIRED: Draft has no domain binding".to_string())?;
        let (Some(system_id), Some(plugin_version), false) = binding else {
            return Err(
                "DRAFT_DOMAIN_REQUIRED: legacy or unscoped Draft cannot be validated".into(),
            );
        };
        if system_id == "__studio_gui__" {
            return Err("DRAFT_DOMAIN_REQUIRED: GUI Draft has no domain manifest".to_string());
        }
        let manifest = self.runtime_manifest_at_version(&system_id, Some(&plugin_version))?;
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT path,content,deleted FROM draft_changes WHERE draft_id=?1 ORDER BY path",
            )
            .map_err(|error| format!("DRAFT_VALIDATION_READ_FAILED: {error}"))?;
        let rows = statement
            .query_map([draft_id], |row| {
                let path = row.get::<_, String>(0)?;
                let content = row.get::<_, Option<Vec<u8>>>(1)?;
                let deleted = row.get::<_, i64>(2)? != 0;
                Ok((path, (!deleted).then_some(content).flatten()))
            })
            .map_err(|error| format!("DRAFT_VALIDATION_READ_FAILED: {error}"))?;
        let changes = rows
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map_err(|error| format!("DRAFT_VALIDATION_READ_FAILED: {error}"))?;
        drop(statement);
        drop(connection);
        self.validate_domain_manifest(
            project_id,
            &manifest,
            Some(&DomainDraftOverlay {
                draft_id: draft_id.to_string(),
                revision: draft.revision,
                changes,
            }),
        )
    }

    fn validate_domain_manifest(
        &self,
        project_id: &str,
        manifest: &DomainManifest,
        overlay: Option<&DomainDraftOverlay>,
    ) -> Result<DomainValidationReport, String> {
        let registry = self.runtime_domain_registry()?;
        let known: BTreeSet<&str> = registry
            .packs
            .iter()
            .map(|pack| pack.system_id.as_str())
            .collect();
        let missing_dependencies = manifest
            .dependencies
            .iter()
            .filter(|dependency| !known.contains(dependency.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let files = self.validation_files_for_manifest(project_id, manifest, overlay)?;
        let mut validators = Vec::with_capacity(manifest.validators.len());
        for validator in &manifest.validators {
            validators.push(self.execute_domain_validator(
                project_id,
                manifest,
                &files,
                validator,
                &missing_dependencies,
            ));
        }
        let engine_compatibility = self.assert_project_engine_compatible(project_id, manifest);
        let mut diagnostics = Vec::new();
        if let Err(error) = &engine_compatibility {
            diagnostics.push(error.clone());
        }
        if let Some(overlay) = overlay {
            diagnostics.push(format!(
                "DOMAIN_DRAFT_OVERLAY_VALIDATED:{}:{}",
                overlay.draft_id, overlay.revision
            ));
        }
        if !missing_dependencies.is_empty() {
            diagnostics.push("DOMAIN_DEPENDENCY_MISSING".to_string());
        }
        diagnostics.extend(
            validators
                .iter()
                .flat_map(|validator| validator.diagnostics.iter().cloned()),
        );
        let writable_files = files
            .iter()
            .filter(|file| file.record.access != "readonly")
            .count();
        Ok(DomainValidationReport {
            system_id: manifest.system_id.clone(),
            valid: engine_compatibility.is_ok()
                && missing_dependencies.is_empty()
                && validators.iter().all(|value| value.valid),
            owned_files: files.len(),
            writable_files,
            readonly_files: files.len().saturating_sub(writable_files),
            missing_dependencies,
            validators,
            diagnostics,
        })
    }

    fn execute_domain_validator(
        &self,
        project_id: &str,
        manifest: &DomainManifest,
        files: &[DomainValidationFile],
        validator: &DomainValidatorContract,
        missing_dependencies: &[String],
    ) -> DomainValidatorResult {
        let mut valid = true;
        let mut checked = 0;
        let mut diagnostics = Vec::new();
        match validator.kind.as_str() {
            "syntax" => {
                for file in files.iter().filter(|file| {
                    file.record.access != "readonly"
                        && (validator.extensions.is_empty()
                            || file.record.extension.as_ref().is_some_and(|extension| {
                                validator
                                    .extensions
                                    .iter()
                                    .any(|value| value.eq_ignore_ascii_case(extension))
                            }))
                }) {
                    checked += 1;
                    if let Some(error) = &file.syntax_error {
                        valid = false;
                        diagnostics.push(format!("{}:{error}", file.record.path));
                    }
                }
            }
            "schema" => {
                checked = files.len();
                if validator.schema != manifest.resources.schema
                    || !manifest
                        .resources
                        .resource_types
                        .iter()
                        .any(|resource_type| resource_type.ends_with(&validator.resource_type))
                {
                    valid = false;
                    diagnostics.push("DOMAIN_SCHEMA_CONTRACT_MISMATCH".to_string());
                }
                for file in files {
                    if file.record.resource_id
                        != stable_resource_id(manifest, &file.record.path, file.content.as_deref())
                        || !matches!(
                            file.record.access.as_str(),
                            "editable" | "structured" | "readonly"
                        )
                    {
                        valid = false;
                        diagnostics
                            .push(format!("DOMAIN_SCHEMA_FILE_INVALID:{}", file.record.path));
                    }
                }
            }
            "uniqueness" | "unique-range" => {
                let mut paths = BTreeSet::new();
                let mut resource_ids = BTreeSet::new();
                checked = files.len();
                for file in files {
                    if !paths.insert(file.record.path.to_lowercase())
                        || !resource_ids.insert(file.record.resource_id.as_str())
                    {
                        valid = false;
                        diagnostics
                            .push(format!("DOMAIN_UNIQUENESS_CONFLICT:{}", file.record.path));
                    }
                }
                if let Some(fields) = validator.fields.as_array() {
                    for field in fields.iter().filter_map(serde_json::Value::as_str) {
                        let mut values = BTreeSet::new();
                        for file in files {
                            if let Some(content) = &file.content {
                                for value in extract_field_values(&content, field) {
                                    checked += 1;
                                    if !values.insert(value.clone()) {
                                        valid = false;
                                        diagnostics.push(format!(
                                            "DOMAIN_UNIQUENESS_FIELD_CONFLICT:{field}:{value}"
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "range" => {
                checked = files.len();
                for file in files {
                    let maximum = match file.record.extension.as_deref().unwrap_or_default() {
                        extension if extension.eq_ignore_ascii_case("map") => 64 * 1024 * 1024,
                        extension if extension.eq_ignore_ascii_case("xls") => 20 * 1024 * 1024,
                        _ => 16 * 1024 * 1024,
                    };
                    if file.record.size > maximum {
                        valid = false;
                        diagnostics
                            .push(format!("DOMAIN_FILE_RANGE_EXCEEDED:{}", file.record.path));
                    }
                }
                if let Some(ranges) = validator.fields.as_array() {
                    for range in ranges {
                        let Some(field) = range.get("field").and_then(serde_json::Value::as_str)
                        else {
                            valid = false;
                            diagnostics.push("DOMAIN_RANGE_CONTRACT_INVALID".to_string());
                            continue;
                        };
                        let minimum = range
                            .get("minimum")
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or(f64::NEG_INFINITY);
                        let maximum = range
                            .get("maximum")
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or(f64::INFINITY);
                        for file in files {
                            if let Some(content) = &file.content {
                                for value in extract_field_values(&content, field) {
                                    checked += 1;
                                    match value.parse::<f64>() {
                                        Ok(number) if number >= minimum && number <= maximum => {}
                                        _ => {
                                            valid = false;
                                            diagnostics.push(format!(
                                                "DOMAIN_RANGE_FIELD_INVALID:{field}:{value}"
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "reference-integrity" => {
                checked = validator.references.len();
                if !missing_dependencies.is_empty() {
                    valid = false;
                    diagnostics.extend(
                        missing_dependencies
                            .iter()
                            .map(|dependency| format!("DOMAIN_REFERENCE_MISSING:{dependency}")),
                    );
                }
                for reference in &validator.references {
                    if !manifest
                        .dependencies
                        .iter()
                        .any(|dependency| dependency == &reference.system_id)
                    {
                        valid = false;
                        diagnostics.push(format!(
                            "DOMAIN_REFERENCE_CONTRACT_INVALID:{}",
                            reference.system_id
                        ));
                    }
                    let referenced_values = files
                        .iter()
                        .filter_map(|file| file.content.clone())
                        .flat_map(|content| extract_field_values(&content, &reference.field))
                        .collect::<BTreeSet<_>>();
                    if referenced_values.is_empty() {
                        continue;
                    }
                    let dependency_values = self
                        .validation_dependency_values(project_id, &reference.system_id)
                        .unwrap_or_default();
                    for value in referenced_values {
                        checked += 1;
                        if reference.required && !dependency_values.contains(&value) {
                            valid = false;
                            diagnostics.push(format!(
                                "DOMAIN_REFERENCE_VALUE_MISSING:{}:{value}",
                                reference.system_id
                            ));
                        }
                    }
                }
                if self
                    .resolve_domain_dependencies(&manifest.system_id)
                    .is_ok_and(|graph| !graph.cycles.is_empty())
                {
                    diagnostics.push("DOMAIN_REFERENCE_CYCLE_DETECTED".to_string());
                }
            }
            "client-engine-consistency" => {
                checked = files.len();
                let client = files.iter().any(|file| file.record.role == "client");
                let engine = files.iter().any(|file| file.record.role == "engine");
                if client != engine {
                    diagnostics.push("DOMAIN_CLIENT_ENGINE_SIDE_INCOMPLETE".to_string());
                }
                if validator.match_by.is_empty() || validator.compare_fields.is_empty() {
                    valid = false;
                    diagnostics.push("DOMAIN_CLIENT_ENGINE_RULE_INVALID".to_string());
                }
                if client && engine && !validator.match_by.is_empty() {
                    let values_for_role = |role: &str| {
                        files
                            .iter()
                            .filter(|file| file.record.role == role)
                            .filter_map(|file| file.content.clone())
                            .flat_map(|content| extract_field_values(&content, &validator.match_by))
                            .collect::<BTreeSet<_>>()
                    };
                    let client_values = values_for_role("client");
                    let engine_values = values_for_role("engine");
                    checked += client_values.len() + engine_values.len();
                    if validator.missing_projection == "error"
                        && !client_values.is_empty()
                        && !engine_values.is_empty()
                        && client_values != engine_values
                    {
                        valid = false;
                        diagnostics.push("DOMAIN_CLIENT_ENGINE_KEY_MISMATCH".to_string());
                    }
                    for field in &validator.compare_fields {
                        let values_for_field = |role: &str| {
                            files
                                .iter()
                                .filter(|file| file.record.role == role)
                                .filter_map(|file| file.content.clone())
                                .flat_map(|content| extract_field_values(&content, field))
                                .collect::<BTreeSet<_>>()
                        };
                        let client_fields = values_for_field("client");
                        let engine_fields = values_for_field("engine");
                        if !client_fields.is_empty()
                            && !engine_fields.is_empty()
                            && client_fields != engine_fields
                        {
                            valid = false;
                            diagnostics
                                .push(format!("DOMAIN_CLIENT_ENGINE_FIELD_MISMATCH:{field}"));
                        }
                    }
                }
            }
            "runtime-diagnostics" => {
                checked = files.iter().filter(|file| file.content.is_some()).count();
                if files.is_empty() {
                    diagnostics.push("DOMAIN_RUNTIME_NO_MATCHED_FILES".to_string());
                }
                if files.iter().all(|file| file.record.access == "readonly") && !files.is_empty() {
                    diagnostics.push("DOMAIN_RUNTIME_READONLY_ONLY".to_string());
                }
                if validator.rule.is_empty() || validator.target.is_empty() {
                    valid = false;
                    diagnostics.push("DOMAIN_RUNTIME_RULE_INVALID".to_string());
                }
                if files.iter().any(|file| {
                    file.record.access != "readonly"
                        && (file.content.is_none() || file.syntax_error.is_some())
                }) {
                    valid = false;
                    diagnostics.push("DOMAIN_RUNTIME_CONTENT_UNAVAILABLE".to_string());
                }
                if let Err(error) = validate_runtime_overlay_rule(&validator.rule, files) {
                    valid = false;
                    diagnostics.push(error);
                }
            }
            _ => {
                valid = false;
                diagnostics.push(format!("DOMAIN_VALIDATOR_UNSUPPORTED:{}", validator.kind));
            }
        }
        DomainValidatorResult {
            id: validator.id.clone(),
            kind: validator.kind.clone(),
            valid,
            checked,
            diagnostics,
        }
    }

    /// 生成与正式文件投影同构的 Draft 覆盖视图。可写格式必须从真实字节重新解析，
    /// 禁止沿用扫描时的旧摘录掩盖草稿中的损坏内容。
    fn validation_files_for_manifest(
        &self,
        project_id: &str,
        manifest: &DomainManifest,
        overlay: Option<&DomainDraftOverlay>,
    ) -> Result<Vec<DomainValidationFile>, String> {
        let project = self.get_project(project_id)?;
        let root = PathBuf::from(project.root);
        let connection = self.project_connection(project_id)?;
        let mut statement = connection
            .prepare(
                "SELECT path,role,category,extension,size,modified_at,content FROM files ORDER BY path",
            )
            .map_err(|error| format!("DOMAIN_VALIDATION_READ_FAILED: {error}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })
            .map_err(|error| format!("DOMAIN_VALIDATION_READ_FAILED: {error}"))?;
        let indexed = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("DOMAIN_VALIDATION_READ_FAILED: {error}"))?;
        drop(statement);
        drop(connection);

        let mut files = Vec::new();
        let mut seen = BTreeSet::new();
        for (path, role, category, extension, size, modified_at, indexed_content) in indexed {
            seen.insert(path.clone());
            if overlay
                .and_then(|value| value.changes.get(&path))
                .is_some_and(Option::is_none)
            {
                continue;
            }
            let changed = overlay.and_then(|value| value.changes.get(&path));
            let bytes = match changed {
                Some(Some(bytes)) => Some(bytes.clone()),
                Some(None) => None,
                None if access_for(manifest, extension.as_deref()) != "readonly" => {
                    Some(read_validation_project_file(&root, &path)?)
                }
                None => None,
            };
            let fingerprint_content = bytes
                .as_deref()
                .and_then(crate::safe_files::decode_supported_text)
                .or_else(|| indexed_content.clone());
            if !matches_projection(
                manifest,
                &path,
                extension.as_deref(),
                fingerprint_content.as_deref(),
            ) {
                continue;
            }
            let access = access_for(manifest, extension.as_deref());
            let (content, syntax_error, projected_size) = validation_file_payload(
                &path,
                extension.as_deref(),
                access,
                bytes.as_deref(),
                indexed_content,
            );
            files.push(DomainValidationFile {
                record: DomainFileRecord {
                    resource_id: stable_resource_id(manifest, &path, content.as_deref()),
                    path,
                    role,
                    category,
                    extension,
                    size: projected_size.unwrap_or_else(|| size.max(0) as u64),
                    modified_at,
                    ownership: "owned".to_string(),
                    access: access.to_string(),
                    systems: vec![manifest.system_id.clone()],
                },
                content,
                syntax_error,
            });
        }

        if let Some(overlay) = overlay {
            for (path, content) in &overlay.changes {
                let Some(bytes) = content else {
                    continue;
                };
                if seen.contains(path) {
                    continue;
                }
                let extension = Path::new(path)
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|value| value.to_lowercase());
                let fingerprint_content = crate::safe_files::decode_supported_text(bytes);
                if !matches_projection(
                    manifest,
                    path,
                    extension.as_deref(),
                    fingerprint_content.as_deref(),
                ) {
                    continue;
                }
                let access = access_for(manifest, extension.as_deref());
                let (content, syntax_error, projected_size) =
                    validation_file_payload(path, extension.as_deref(), access, Some(bytes), None);
                let normalized = path.replace('\\', "/").to_lowercase();
                let role = if normalized.contains("客户端") || normalized.contains("client") {
                    "client"
                } else if normalized.contains("引擎") || normalized.contains("engine") {
                    "engine"
                } else {
                    "project"
                };
                files.push(DomainValidationFile {
                    record: DomainFileRecord {
                        resource_id: stable_resource_id(manifest, path, content.as_deref()),
                        path: path.clone(),
                        role: role.to_string(),
                        category: "other".to_string(),
                        extension,
                        size: projected_size.unwrap_or(bytes.len() as u64),
                        modified_at: 0,
                        ownership: "owned".to_string(),
                        access: access.to_string(),
                        systems: vec![manifest.system_id.clone()],
                    },
                    content,
                    syntax_error,
                });
            }
        }
        files.sort_by(|left, right| left.record.path.cmp(&right.record.path));
        Ok(files)
    }

    /// 依赖引用按其领域稳定键精确解析，不再使用任意文件全文子串命中。
    fn validation_dependency_values(
        &self,
        project_id: &str,
        system_id: &str,
    ) -> Result<BTreeSet<String>, String> {
        let manifest = self.runtime_manifest(system_id)?;
        let files = self.validation_files_for_manifest(project_id, &manifest, None)?;
        Ok(manifest
            .resources
            .unique_key
            .iter()
            .flat_map(|field| {
                files
                    .iter()
                    .filter_map(|file| file.content.as_deref())
                    .flat_map(|content| extract_field_values(content, field))
            })
            .collect())
    }

    fn verified_access_for(
        &self,
        project_id: &str,
        manifest: &DomainManifest,
        path: &str,
        extension: Option<&str>,
    ) -> &'static str {
        let declared = access_for(manifest, extension);
        if declared == "readonly" {
            return declared;
        }
        if self
            .assert_project_engine_compatible(project_id, manifest)
            .is_err()
        {
            return "readonly";
        }
        match extension.unwrap_or_default() {
            value if value.eq_ignore_ascii_case("map") => self
                .verified_map_header(project_id, path)
                .filter(|header| header.capabilities.terrain_editable)
                .map(|_| "structured")
                .unwrap_or("readonly"),
            value if value.eq_ignore_ascii_case("xls") => self
                .safe_xls_open(project_id, path)
                .map(|_| "structured")
                .unwrap_or("readonly"),
            _ => declared,
        }
    }

    fn indexed_file_content(&self, project_id: &str, path: &str) -> Option<String> {
        self.project_connection(project_id)
            .ok()?
            .query_row("SELECT content FROM files WHERE path=?1", [path], |row| {
                row.get::<_, Option<String>>(0)
            })
            .ok()
            .flatten()
    }

    fn verified_map_header(&self, project_id: &str, path: &str) -> Option<mir3_map::MapHeader> {
        let project = self.get_project(project_id).ok()?;
        let target = std::path::Path::new(&project.root).join(path);
        let metadata = target.metadata().ok()?;
        let file_len = usize::try_from(metadata.len()).ok()?;
        let mut prefix = [0_u8; 28];
        let mut file = File::open(target).ok()?;
        file.read_exact(&mut prefix).ok()?;
        Some(mir3_map::detect_header_with_len(&prefix, file_len, None))
    }

    pub fn get_domain_resource(
        &self,
        project_id: &str,
        system_id: &str,
        resource_id: &str,
    ) -> Result<DomainResourceRecord, String> {
        if let Some(resource) = self
            .query_domain_resources(
                project_id,
                system_id,
                &crate::DomainResourceQuery {
                    text: String::new(),
                    resource_type: None,
                    limit: Some(10_000),
                    offset: None,
                },
            )?
            .into_iter()
            .find(|resource| resource.id == resource_id)
        {
            return Ok(resource);
        }
        let manifest = self.runtime_manifest(system_id)?;
        let file = self
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
            .find(|file| file.resource_id == resource_id)
            .ok_or_else(|| format!("DOMAIN_RESOURCE_NOT_FOUND: {resource_id}"))?;
        let resource_type = manifest
            .resources
            .resource_types
            .first()
            .cloned()
            .unwrap_or_else(|| "file".to_string());
        let (projection, diagnostics) = self.project_domain_resource(project_id, &file);
        Ok(DomainResourceRecord {
            id: file.resource_id.clone(),
            system_id: system_id.to_string(),
            resource_type,
            label: file
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&file.path)
                .to_string(),
            writable: file.access != "readonly",
            files: vec![file.clone()],
            dependency_systems: manifest.dependencies.clone(),
            projection,
            diagnostics,
            fields: serde_json::Map::new(),
            source: crate::DomainResourceSource {
                path: file.path.clone(),
                sheet: None,
                row: None,
                headers: Vec::new(),
            },
            dependencies: Vec::new(),
            mappings_applied: manifest.resources.mappings.clone(),
        })
    }

    fn project_domain_resource(
        &self,
        project_id: &str,
        file: &DomainFileRecord,
    ) -> (serde_json::Value, Vec<String>) {
        let extension = file.extension.as_deref().unwrap_or_default();
        if extension.eq_ignore_ascii_case("map") {
            return match self.map_resource_open(project_id, &file.path, None, Some((0, 0, 64))) {
                Ok(map) => (
                    serde_json::json!({"kind":"map","header":map.header,"initialChunk":map.chunk}),
                    Vec::new(),
                ),
                Err(error) => (
                    serde_json::Value::Null,
                    vec![format!("DOMAIN_RESOURCE_MAP_READ_FAILED:{error}")],
                ),
            };
        }
        if extension.eq_ignore_ascii_case("xls") {
            return match self.safe_xls_open(project_id, &file.path) {
                Ok(workbook) => {
                    let sheets = workbook
                        .sheets
                        .iter()
                        .take(4)
                        .filter_map(|sheet| {
                            self.safe_xls_sheet_read(
                                project_id,
                                &file.path,
                                &sheet.name,
                                &workbook.sha256,
                            )
                            .ok()
                            .map(|data| {
                                serde_json::json!({
                                    "name":data.sheet,
                                    "rowCount":data.row_count,
                                    "columnCount":data.column_count,
                                    "rows":data.rows.into_iter().take(100).map(|row| row.into_iter().take(32).collect::<Vec<_>>()).collect::<Vec<_>>()
                                })
                            })
                        })
                        .collect::<Vec<_>>();
                    (
                        serde_json::json!({
                            "kind":"xls",
                            "sha256":workbook.sha256,
                            "sheets":sheets,
                            "truncated":workbook.sheets.len() > 4 || workbook.sheets.iter().any(|sheet| sheet.row_count > 100 || sheet.column_count > 32)
                        }),
                        Vec::new(),
                    )
                }
                Err(error) => (
                    serde_json::Value::Null,
                    vec![format!("DOMAIN_RESOURCE_XLS_READ_FAILED:{error}")],
                ),
            };
        }
        if let Some(content) = self.indexed_file_content(project_id, &file.path) {
            let truncated = content.chars().count() > 64_000;
            let content = content.chars().take(64_000).collect::<String>();
            return (
                serde_json::json!({"kind":"text","content":content,"truncated":truncated}),
                Vec::new(),
            );
        }
        (
            serde_json::Value::Null,
            vec!["DOMAIN_RESOURCE_PROJECTION_UNAVAILABLE".to_string()],
        )
    }

    pub fn resolve_domain_dependencies(
        &self,
        system_id: &str,
    ) -> Result<DomainDependencyGraph, String> {
        let registry = self.runtime_domain_registry()?;
        let manifest = registry
            .packs
            .iter()
            .find(|manifest| manifest.system_id == system_id)
            .ok_or_else(|| format!("DOMAIN_SYSTEM_NOT_FOUND: {system_id}"))?;
        let direct = manifest.dependencies.clone();
        let active_systems = registry
            .packs
            .iter()
            .map(|manifest| manifest.system_id.as_str())
            .collect::<BTreeSet<_>>();
        let missing = direct
            .iter()
            .filter(|dependency| !active_systems.contains(dependency.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let mut transitive = BTreeSet::new();
        let mut cycles = Vec::new();
        let mut stack = vec![(system_id.to_string(), vec![system_id.to_string()])];
        while let Some((current, path)) = stack.pop() {
            let Some(current_manifest) = registry
                .packs
                .iter()
                .find(|candidate| candidate.system_id == current)
            else {
                continue;
            };
            for dependency in &current_manifest.dependencies {
                if let Some(position) = path.iter().position(|entry| entry == dependency) {
                    let mut cycle = path[position..].to_vec();
                    cycle.push(dependency.clone());
                    if !cycles.contains(&cycle) {
                        cycles.push(cycle);
                    }
                    continue;
                }
                if transitive.insert(dependency.clone()) {
                    let mut next_path = path.clone();
                    next_path.push(dependency.clone());
                    stack.push((dependency.clone(), next_path));
                }
            }
        }
        transitive.remove(system_id);
        Ok(DomainDependencyGraph {
            system_id: system_id.to_string(),
            direct,
            transitive: transitive.into_iter().collect(),
            missing,
            cycles,
        })
    }

    /// 已绑定领域的 Draft 只能写该领域明确声明且可写的真实文件类型。
    pub(crate) fn assert_draft_path_writable(
        &self,
        project_id: &str,
        draft_id: &str,
        path: &str,
    ) -> Result<(), String> {
        let binding = self
            .project_connection(project_id)?
            .query_row(
                "SELECT system_id,plugin_version,legacy FROM draft_domains WHERE draft_id=?1",
                [draft_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("DRAFT_DOMAIN_READ_FAILED: {error}"))?;
        let Some((system_id, plugin_version, legacy)) = binding else {
            return Err(
                "DRAFT_DOMAIN_REQUIRED: unscoped and legacy drafts are read-only; clone or bind a domain draft"
                    .to_string(),
            );
        };
        if legacy != 0 {
            return Err(
                "DRAFT_LEGACY_READONLY: clone the legacy draft after source verification"
                    .to_string(),
            );
        }
        let Some(system_id) = system_id else {
            return Err("DRAFT_DOMAIN_REQUIRED: draft is not bound to a domain".to_string());
        };
        if system_id == "__studio_gui__" {
            return Ok(());
        }
        let plugin_version = plugin_version.ok_or_else(|| {
            "DRAFT_DOMAIN_VERSION_REQUIRED: draft has no pinned plugin version".to_string()
        })?;
        let manifest = self.runtime_manifest_at_version(&system_id, Some(&plugin_version))?;
        self.assert_project_engine_compatible(project_id, &manifest)?;
        let extension = std::path::Path::new(path)
            .extension()
            .and_then(|value| value.to_str());
        let path_projection_matches = matches_projection(&manifest, path, extension, None);
        let content_projection_matches = if path_projection_matches {
            false
        } else {
            let project = self.get_project(project_id)?;
            let root = fs::canonicalize(&project.root).ok();
            let target = root.as_ref().and_then(|root| {
                fs::canonicalize(root.join(path))
                    .ok()
                    .map(|target| (root, target))
            });
            target.is_some_and(|(root, target)| {
                crate::path_is_within(root, &target)
                    && fs::read(target)
                        .ok()
                        .and_then(|bytes| {
                            crate::safe_files::decode_supported_text_checked(&bytes).ok()
                        })
                        .is_some_and(|content| {
                            matches_projection(&manifest, path, extension, Some(&content))
                        })
            })
        };
        if !path_projection_matches && !content_projection_matches {
            return Err(format!(
                "DRAFT_DOMAIN_SCOPE_DENIED: {path} is not owned by {system_id}"
            ));
        }
        if self.verified_access_for(project_id, &manifest, path, extension) == "readonly" {
            return Err(format!(
                "DRAFT_DOMAIN_READONLY: {path} has no verified writer"
            ));
        }
        Ok(())
    }

    /// 领域写入必须绑定一个可归一化且命中声明范围的真实引擎版本。
    /// 目录、内容指纹与 Schema 仍由投影及校验门禁分别验证。
    pub fn assert_project_engine_compatible(
        &self,
        project_id: &str,
        manifest: &DomainManifest,
    ) -> Result<(), String> {
        #[cfg(test)]
        if self.trusted_fixture_engine_override {
            return Ok(());
        }
        let project = self.get_project(project_id)?;
        let detected = project.engine_version.as_deref().ok_or_else(|| {
            format!(
                "DOMAIN_ENGINE_VERSION_UNVERIFIED: {} is read-only until the engine version is detected",
                manifest.system_id
            )
        })?;
        let engine = normalize_engine_version(detected, &manifest.engine_compatibility)?;
        let requirement = VersionReq::parse(&manifest.supported_engine_range).map_err(|error| {
            format!(
                "DOMAIN_ENGINE_RANGE_INVALID: {}: {error}",
                manifest.supported_engine_range
            )
        })?;
        if !requirement.matches(&engine) {
            return Err(format!(
                "DOMAIN_ENGINE_INCOMPATIBLE: {engine} does not match {} for {}",
                manifest.supported_engine_range, manifest.system_id
            ));
        }
        Ok(())
    }

    pub fn validate_draft_capability(
        &self,
        project_id: &str,
        draft_id: &str,
        capability_id: &str,
    ) -> Result<OfficialCapability, String> {
        let (system_id, plugin_version) = self
            .project_connection(project_id)?
            .query_row(
                "SELECT system_id,plugin_version FROM draft_domains WHERE draft_id=?1 AND legacy=0",
                [draft_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("DRAFT_DOMAIN_READ_FAILED: {error}"))?
            .and_then(|(system_id, plugin_version)| system_id.zip(plugin_version))
            .ok_or_else(|| {
                "DRAFT_DOMAIN_REQUIRED: generic operations require a versioned scoped draft"
                    .to_string()
            })?;
        let manifest = self.runtime_manifest_at_version(&system_id, Some(&plugin_version))?;
        let capability = manifest
            .capabilities
            .iter()
            .find(|capability| capability.id == capability_id)
            .cloned()
            .ok_or_else(|| {
                format!("DOMAIN_CAPABILITY_DENIED: {capability_id} is not declared by {system_id}")
            })?;
        if !capability
            .write_systems
            .iter()
            .any(|value| value == &system_id)
        {
            return Err(format!(
                "DOMAIN_CAPABILITY_READONLY: {capability_id} cannot write {system_id}"
            ));
        }
        if !capability.reversible
            || !capability.preview_required
            || !capability.validation_required
            || !capability.confirmation_required
        {
            return Err(format!(
                "DOMAIN_CAPABILITY_POLICY_INVALID: {capability_id} must be reversible and gated"
            ));
        }
        Ok(capability)
    }

    /// 返回 Draft 固定版本的完整领域契约，供安全编译器读取 unique key 等写入语义。
    /// 不允许回退到当前激活版本，避免升级后运行中的 Draft 混用新契约。
    pub fn draft_domain_manifest(
        &self,
        project_id: &str,
        draft_id: &str,
    ) -> Result<DomainManifest, String> {
        let (system_id, plugin_version) = self
            .project_connection(project_id)?
            .query_row(
                "SELECT system_id,plugin_version FROM draft_domains WHERE draft_id=?1 AND legacy=0",
                [draft_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("DRAFT_DOMAIN_READ_FAILED: {error}"))?
            .and_then(|(system_id, plugin_version)| system_id.zip(plugin_version))
            .ok_or_else(|| {
                "DRAFT_DOMAIN_REQUIRED: compiler requires a versioned scoped draft".to_string()
            })?;
        self.runtime_manifest_at_version(&system_id, Some(&plugin_version))
    }
}

fn validate_registry(registry: &DomainRegistry) -> Result<(), String> {
    if registry.schema_version != 1 {
        return Err(format!(
            "DOMAIN_REGISTRY_SCHEMA_UNSUPPORTED: {}",
            registry.schema_version
        ));
    }
    if registry.packs.len() != 33 {
        return Err(format!(
            "DOMAIN_REGISTRY_COUNT_INVALID: expected 33, got {}",
            registry.packs.len()
        ));
    }
    let mut ids = BTreeSet::new();
    let allowed_primitives = BTreeSet::from([
        "resource-index-v1",
        "draft-v1",
        "diff-v1",
        "validation-v1",
        "capability-v1",
        "map-binary-v1",
    ]);
    let allowed_roles = BTreeSet::from(["client", "engine", "shared", "generated", "readonly"]);
    let allowed_validators = BTreeSet::from([
        "syntax",
        "schema",
        "unique-range",
        "uniqueness",
        "range",
        "reference-integrity",
        "client-engine-consistency",
        "runtime-diagnostics",
    ]);
    for pack in &registry.packs {
        if pack.kind != "domain" || !ids.insert(pack.system_id.as_str()) {
            return Err(format!("DOMAIN_MANIFEST_INVALID: {}", pack.system_id));
        }
        let version = Version::parse(&pack.version);
        let kernel_requirement = VersionReq::parse(&pack.kernel_api_range);
        let engine_requirement = VersionReq::parse(&pack.supported_engine_range);
        let kernel = Version::parse(DOMAIN_KERNEL_VERSION)
            .map_err(|error| format!("DOMAIN_KERNEL_VERSION_INVALID: {error}"))?;
        let stable_version = version
            .as_ref()
            .is_ok_and(|version| version.pre.is_empty() && version.build.is_empty());
        let compatible_kernel = kernel_requirement
            .as_ref()
            .is_ok_and(|requirement| requirement.matches(&kernel));
        if !stable_version || !compatible_kernel || engine_requirement.is_err() {
            return Err(format!(
                "DOMAIN_MANIFEST_VERSION_INVALID: {}",
                pack.system_id
            ));
        }
        let requires_evidence_contract = version
            .as_ref()
            .is_ok_and(|version| version >= &Version::new(1, 2, 0));
        if requires_evidence_contract
            && (pack.supported_engine_range == "*"
                || pack.engine_compatibility.strategy != "evidence-gated-auto-generalization-v1"
                || pack.engine_compatibility.version_aliases
                    != ["semver", "v-prefixed-semver", "major-minor"]
                || pack.engine_compatibility.required_evidence
                    != [
                        "project-directory-layout",
                        "owned-selector-or-content-fingerprint",
                        "resource-schema-validation",
                    ]
                || pack.engine_compatibility.unknown_version_policy != "readonly"
                || pack.engine_compatibility.incompatible_version_policy != "readonly")
        {
            return Err(format!(
                "DOMAIN_ENGINE_COMPATIBILITY_CONTRACT_INVALID: {}",
                pack.system_id
            ));
        }
        if pack.manifest_schema_version != 1
            || pack.resource_schema_version != 1
            || pack.capability_schema_version != 1
            || pack.memory_schema_version != 1
        {
            return Err(format!(
                "DOMAIN_MANIFEST_SCHEMA_INVALID: {}",
                pack.system_id
            ));
        }
        if pack.required_kernel_primitives.is_empty()
            || pack
                .required_kernel_primitives
                .iter()
                .any(|primitive| !allowed_primitives.contains(primitive.as_str()))
        {
            return Err(format!(
                "DOMAIN_MANIFEST_PRIMITIVE_INVALID: {}",
                pack.system_id
            ));
        }
        if pack.file_projection.owned_selectors.is_empty()
            || pack.file_projection.roles.is_empty()
            || pack.file_projection.content_fingerprints.is_empty()
            || pack.file_projection.path_aliases.is_empty()
            || pack
                .file_projection
                .roles
                .iter()
                .any(|role| !allowed_roles.contains(role.as_str()))
        {
            return Err(format!(
                "DOMAIN_FILE_PROJECTION_INVALID: {}",
                pack.system_id
            ));
        }
        if pack.resources.resource_types.is_empty()
            || !pack.resources.stable_resource_id.starts_with("sha256(")
            || !pack
                .resources
                .stable_resource_id
                .ends_with(":normalizedRelativePath)")
            || pack.resources.schema.is_empty()
            || pack.resources.unique_key.is_empty()
            || pack.presentation.primary_view.is_empty()
            || !pack
                .presentation
                .views
                .iter()
                .any(|view| view == &pack.presentation.primary_view)
            || pack.renderer != pack.presentation.primary_view
            || !matches!(
                pack.presentation.safe_primitive.as_str(),
                "xls" | "graph" | "timeline" | "map"
            )
            || pack.documentation.readme != "README.md"
            || pack.documentation.changelog != "CHANGELOG.md"
            || pack.fixtures.valid.is_empty()
            || pack.fixtures.invalid.is_empty()
            || pack.fixtures.expected_diagnostics.is_empty()
        {
            return Err(format!(
                "DOMAIN_RESOURCE_CONTRACT_INVALID: {}",
                pack.system_id
            ));
        }
        if pack.operations.is_empty()
            || pack.capabilities.is_empty()
            || pack.validators.is_empty()
            || pack.operations.len() != pack.capabilities.len()
        {
            return Err(format!(
                "DOMAIN_BEHAVIOR_CONTRACT_EMPTY: {}",
                pack.system_id
            ));
        }
        for validator in &pack.validators {
            if validator.id.is_empty() || !allowed_validators.contains(validator.kind.as_str()) {
                return Err(format!("DOMAIN_VALIDATOR_INVALID: {}", pack.system_id));
            }
        }
        let mut operation_ids = BTreeSet::new();
        for operation in &pack.operations {
            if !operation_ids.insert(operation.id.as_str())
                || !operation.parameter_schema.is_object()
                || !operation
                    .read_systems
                    .iter()
                    .any(|value| value == &pack.system_id)
                || (!operation.write_systems.is_empty()
                    && operation.write_systems != [pack.system_id.clone()])
                || operation.preconditions.is_empty()
                || operation.steps.is_empty()
                || !operation
                    .steps
                    .iter()
                    .any(|step| step.operation == operation.id)
                || operation.steps.iter().any(|step| {
                    !matches!(
                        step.primitive.as_str(),
                        "text" | "xls" | "graph" | "timeline" | "map"
                    ) || step.action.is_empty()
                        || ["shell", "exec", "command", "script", "absolute"]
                            .iter()
                            .any(|forbidden| {
                                step.action.to_ascii_lowercase().contains(forbidden)
                                    || step.operation.to_ascii_lowercase().contains(forbidden)
                            })
                })
                || !operation.reversible
                || !operation.preview_policy.preview_required
                || !operation.preview_policy.validation_required
                || (!operation.write_systems.is_empty()
                    && !operation.preview_policy.confirmation_required)
            {
                return Err(format!(
                    "DOMAIN_OPERATION_CONTRACT_INVALID: {}:{}",
                    pack.system_id, operation.id
                ));
            }
        }
        let mut capability_ids = BTreeSet::new();
        for capability in &pack.capabilities {
            if capability.id.is_empty()
                || !capability_ids.insert(capability.id.as_str())
                || !capability.parameter_schema.is_object()
                || capability.steps.is_empty()
                || (!capability.write_systems.is_empty()
                    && capability.write_systems != [pack.system_id.clone()])
                || !capability
                    .read_systems
                    .iter()
                    .any(|value| value == &pack.system_id)
                || !capability.reversible
                || !capability.preview_required
                || !capability.validation_required
                || !capability.confirmation_required
                || !operation_ids.contains(capability.id.as_str())
            {
                return Err(format!(
                    "DOMAIN_CAPABILITY_CONTRACT_INVALID: {}:{}",
                    pack.system_id, capability.id
                ));
            }
            if !capability
                .steps
                .iter()
                .any(|step| step.operation == capability.id)
                || capability.steps.iter().any(|step| {
                    !matches!(
                        step.primitive.as_str(),
                        "text" | "xls" | "graph" | "timeline" | "map"
                    ) || step.action.is_empty()
                        || ["shell", "exec", "command", "script", "absolute"]
                            .iter()
                            .any(|forbidden| {
                                step.action.to_ascii_lowercase().contains(forbidden)
                                    || step.operation.to_ascii_lowercase().contains(forbidden)
                            })
                })
            {
                return Err(format!(
                    "DOMAIN_CAPABILITY_STEP_INVALID: {}:{}",
                    pack.system_id, capability.id
                ));
            }
        }
    }
    for pack in &registry.packs {
        let selector_dependencies = pack
            .file_projection
            .dependency_selectors
            .iter()
            .map(|selector| selector.system_id.as_str())
            .collect::<BTreeSet<_>>();
        let declared_dependencies = pack
            .dependencies
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let resource_dependencies = pack
            .resources
            .dependency_edges
            .iter()
            .map(|edge| edge.system_id.as_str())
            .collect::<BTreeSet<_>>();
        if selector_dependencies != declared_dependencies
            || resource_dependencies
                .iter()
                .any(|dependency| !ids.contains(dependency))
        {
            return Err(format!(
                "DOMAIN_DEPENDENCY_CONTRACT_MISMATCH: {}",
                pack.system_id
            ));
        }
        for dependency in &pack.dependencies {
            if !ids.contains(dependency.as_str()) {
                return Err(format!(
                    "DOMAIN_MANIFEST_DEPENDENCY_INVALID: {} -> {dependency}",
                    pack.system_id
                ));
            }
        }
    }
    Ok(())
}

/// 将清单声明允许的有限别名归一化为 SemVer；不猜测厂商字符串或日期格式。
pub fn normalize_engine_version(
    value: &str,
    compatibility: &DomainEngineCompatibility,
) -> Result<Version, String> {
    let aliases = &compatibility.version_aliases;
    let trimmed = value.trim();
    // 1.0/1.1 包没有该字段；仅为已安装旧版本提供同等的保守解析，不放宽未知版本。
    let legacy_contract = aliases.is_empty();
    if legacy_contract || aliases.iter().any(|alias| alias == "semver") {
        if let Ok(version) = Version::parse(trimmed) {
            return Ok(version);
        }
    }
    if legacy_contract || aliases.iter().any(|alias| alias == "v-prefixed-semver") {
        if let Some(normalized) = trimmed
            .strip_prefix('v')
            .or_else(|| trimmed.strip_prefix('V'))
        {
            if let Ok(version) = Version::parse(normalized) {
                return Ok(version);
            }
        }
    }
    if (legacy_contract || aliases.iter().any(|alias| alias == "major-minor"))
        && trimmed.matches('.').count() == 1
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        if let Ok(version) = Version::parse(&format!("{trimmed}.0")) {
            return Ok(version);
        }
    }
    Err(format!(
        "DOMAIN_ENGINE_VERSION_UNVERIFIED: {value} does not match a declared version alias"
    ))
}

fn matches_projection(
    manifest: &DomainManifest,
    path: &str,
    extension: Option<&str>,
    content: Option<&str>,
) -> bool {
    let normalized = path.replace('\\', "/").to_lowercase();
    if manifest
        .file_projection
        .excludes
        .iter()
        .any(|selector| globish_matches(&normalized, selector))
    {
        return false;
    }
    manifest
        .file_projection
        .keywords
        .iter()
        .chain(manifest.file_projection.owned_selectors.iter())
        .any(|keyword| {
            let keyword = keyword.to_lowercase();
            if let Some(expected_extension) = keyword.strip_prefix('.') {
                extension.is_some_and(|value| value.eq_ignore_ascii_case(expected_extension))
            } else {
                selector_contains(&normalized, &keyword)
                    || manifest.file_projection.path_aliases.iter().any(|alias| {
                        selector_contains(
                            &normalized
                                .replace(&alias.to.to_lowercase(), &alias.from.to_lowercase()),
                            &keyword,
                        )
                    })
            }
        })
        || manifest
            .file_projection
            .content_fingerprints
            .iter()
            .any(|fingerprint| {
                content.is_some_and(|content| {
                    if fingerprint.case_sensitive {
                        selector_contains(content, &fingerprint.contains)
                    } else {
                        selector_contains(
                            &content.to_lowercase(),
                            &fingerprint.contains.to_lowercase(),
                        )
                    }
                })
            })
}

/// ASCII 领域词必须出现在路径/内容的词边界上，避免 `mall` 误命中
/// `mimalloc.dll`、`map` 误命中普通单词。带目录分隔符或中文的选择器保留
/// 精确子串语义，以兼容真实 996 中文目录和 `market_def` 等既有命名。
fn selector_contains(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    if needle.contains('/') || needle.chars().any(|value| !value.is_ascii()) {
        return haystack.contains(needle);
    }
    haystack.match_indices(needle).any(|(start, matched)| {
        let before = haystack[..start].chars().next_back();
        let end = start + matched.len();
        let after = haystack[end..].chars().next();
        !before.is_some_and(is_selector_word_character)
            && !after.is_some_and(is_selector_word_character)
    })
}

fn is_selector_word_character(value: char) -> bool {
    value.is_ascii_alphanumeric()
}

fn globish_matches(path: &str, selector: &str) -> bool {
    let needle = selector
        .replace('\\', "/")
        .to_lowercase()
        .trim_matches('*')
        .to_string();
    !needle.is_empty() && path.contains(&needle)
}

fn read_validation_project_file(root: &Path, path: &str) -> Result<Vec<u8>, String> {
    let relative = Path::new(path);
    if relative.is_absolute()
        || relative.components().any(|component| {
            !matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        return Err(format!("DOMAIN_VALIDATION_PATH_INVALID: {path}"));
    }
    fs::read(root.join(relative))
        .map_err(|error| format!("DOMAIN_VALIDATION_FILE_READ_FAILED: {path}: {error}"))
}

fn validation_file_payload(
    path: &str,
    extension: Option<&str>,
    access: &str,
    bytes: Option<&[u8]>,
    indexed_content: Option<String>,
) -> (Option<String>, Option<String>, Option<u64>) {
    if access == "readonly" {
        return (indexed_content, None, bytes.map(|value| value.len() as u64));
    }
    let Some(bytes) = bytes else {
        return (
            None,
            Some(format!("DOMAIN_DRAFT_CONTENT_MISSING:{path}")),
            None,
        );
    };
    let parsed = match extension.unwrap_or_default().to_ascii_lowercase().as_str() {
        "txt" => crate::safe_files::decode_supported_text_checked(bytes)
            .and_then(|content| validate_text_overlay_structure(path, &content, false)),
        "lua" => crate::safe_files::decode_supported_text_checked(bytes)
            .and_then(|content| validate_text_overlay_structure(path, &content, true)),
        "xls" => crate::safe_files::project_xls_validation_content(bytes),
        "map" => mir3_map::MapDocument::parse(bytes.to_vec()).and_then(|document| {
            let header = document.header();
            if !header.capabilities.terrain_editable {
                return Err("MAP_FORMAT_READONLY: unrecognized map format".to_string());
            }
            let map_id = Path::new(path)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            Ok(format!(
                "mapId={map_id}\nwidth={}\nheight={}\n",
                header.width, header.height
            ))
        }),
        _ => Err(format!("DOMAIN_DRAFT_FORMAT_UNMAPPED: {path}")),
    };
    match parsed {
        Ok(content) => (Some(content), None, Some(bytes.len() as u64)),
        Err(error) => (None, Some(error), Some(bytes.len() as u64)),
    }
}

fn validate_text_overlay_structure(
    path: &str,
    content: &str,
    validate_lua: bool,
) -> Result<String, String> {
    if content
        .chars()
        .any(|value| value == '\0' || (value.is_control() && !matches!(value, '\n' | '\r' | '\t')))
    {
        return Err(format!("DOMAIN_TEXT_CONTROL_CHARACTER: {path}"));
    }
    if content.lines().any(|line| line.len() > 1024 * 1024) {
        return Err(format!("DOMAIN_TEXT_LINE_TOO_LONG: {path}"));
    }
    let trimmed = content.trim_start_matches('\u{feff}').trim_start();
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(trimmed).is_err()
    {
        return Err(format!("DOMAIN_TEXT_JSON_STRUCTURE_INVALID: {path}"));
    }
    if validate_lua {
        validate_lua_delimiters(path, content)?;
    }
    Ok(content.to_string())
}

fn validate_lua_delimiters(path: &str, content: &str) -> Result<(), String> {
    let mut stack = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut characters = content.chars().peekable();
    while let Some(character) = characters.next() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }
        if character == '-' && characters.peek() == Some(&'-') {
            characters.next();
            for value in characters.by_ref() {
                if value == '\n' {
                    break;
                }
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '(' | '{' | '[' => stack.push(character),
            ')' | '}' | ']' => {
                let expected = match character {
                    ')' => '(',
                    '}' => '{',
                    _ => '[',
                };
                if stack.pop() != Some(expected) {
                    return Err(format!("DOMAIN_LUA_STRUCTURE_INVALID: {path}"));
                }
            }
            _ => {}
        }
    }
    if quote.is_some() || !stack.is_empty() {
        return Err(format!("DOMAIN_LUA_STRUCTURE_INVALID: {path}"));
    }
    Ok(())
}

fn validate_runtime_overlay_rule(rule: &str, files: &[DomainValidationFile]) -> Result<(), String> {
    let contents = files
        .iter()
        .filter_map(|file| file.content.as_deref())
        .collect::<Vec<_>>();
    let ordered_pairs = [
        ("startEpochSeconds", "endEpochSeconds"),
        ("startMinute", "endMinute"),
        ("minimumValue", "maximumValue"),
        ("minimumQuality", "maximumQuality"),
    ];
    if rule.contains("start-before-end")
        || rule.contains("window")
        || rule.contains("range-ordered")
        || rule.contains("minimum-not-greater")
    {
        for content in &contents {
            for (minimum_field, maximum_field) in ordered_pairs {
                let minimum = extract_field_values(content, minimum_field)
                    .into_iter()
                    .next()
                    .and_then(|value| value.parse::<f64>().ok());
                let maximum = extract_field_values(content, maximum_field)
                    .into_iter()
                    .next()
                    .and_then(|value| value.parse::<f64>().ok());
                if minimum
                    .zip(maximum)
                    .is_some_and(|(left, right)| left > right)
                {
                    return Err(format!("DOMAIN_RUNTIME_RULE_FAILED:{rule}"));
                }
            }
        }
    }
    let monotonic = match rule {
        "level.experience-monotonic" => Some(("requiredExperience", false)),
        "online-reward.duration-monotonic" => Some(("durationSeconds", false)),
        "cumulative-charge.thresholds-strictly-increase" => Some(("chargeThreshold", true)),
        "vip.points-monotonic" => Some(("requiredPoints", false)),
        _ => None,
    };
    if let Some((field, strict)) = monotonic {
        let values = contents
            .iter()
            .flat_map(|content| extract_field_values(content, field))
            .map(|value| {
                value
                    .parse::<f64>()
                    .map_err(|_| format!("DOMAIN_RUNTIME_FIELD_INVALID:{rule}:{field}:{value}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if values.windows(2).any(|pair| {
            if strict {
                pair[0] >= pair[1]
            } else {
                pair[0] > pair[1]
            }
        }) {
            return Err(format!("DOMAIN_RUNTIME_RULE_FAILED:{rule}"));
        }
    }
    Ok(())
}

fn extract_field_values(content: &str, field: &str) -> Vec<String> {
    let mut values = Vec::new();
    let json_marker = format!("\"{field}\"");
    for line in content.lines() {
        let trimmed = line.trim();
        let remainder = [
            format!("{field}="),
            format!("{field}:"),
            format!("{field}\t"),
            json_marker.clone(),
        ]
        .iter()
        .find_map(|marker| {
            trimmed.find(marker).map(|index| {
                let mut value = &trimmed[index + marker.len()..];
                value = value.trim_start_matches([' ', '\t', ':', '=']);
                value
            })
        });
        if let Some(remainder) = remainder {
            let value = remainder
                .split([',', ';', '\t', '}', ']'])
                .next()
                .unwrap_or_default()
                .trim()
                .trim_matches(['"', '\''])
                .to_string();
            if !value.is_empty() {
                values.push(value);
            }
        }
    }
    values
}

fn access_for(manifest: &DomainManifest, extension: Option<&str>) -> &'static str {
    let extension = extension.unwrap_or_default();
    if manifest.system_id == "map" && extension.eq_ignore_ascii_case("map") {
        return "structured";
    }
    if manifest
        .file_projection
        .editable_extensions
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(extension))
    {
        "editable"
    } else if manifest
        .file_projection
        .structured_extensions
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(extension))
    {
        "structured"
    } else {
        "readonly"
    }
}

fn stable_resource_id(manifest: &DomainManifest, path: &str, content: Option<&str>) -> String {
    let normalized = path
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_lowercase();
    let strategy = manifest
        .resources
        .stable_resource_id
        .trim_start_matches("sha256(")
        .trim_end_matches(')')
        .split(':')
        .collect::<Vec<_>>();
    let domain_key = strategy
        .get(1)
        .and_then(|field| {
            content
                .and_then(|content| extract_field_values(content, field).into_iter().next())
                .or_else(|| {
                    std::path::Path::new(path)
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .map(str::to_string)
                })
        })
        .unwrap_or_else(|| normalized.clone());
    let mut digest = Sha256::new();
    digest.update(manifest.system_id.as_bytes());
    digest.update([0]);
    digest.update(domain_key.as_bytes());
    digest.update([0]);
    digest.update(normalized.as_bytes());
    let hash = format!("{:x}", digest.finalize());
    format!("{}:{}", manifest.system_id, &hash[..16])
}

fn stable_unknown_resource_id(path: &str) -> String {
    let normalized = path
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_lowercase();
    let mut digest = Sha256::new();
    digest.update(b"unknown");
    digest.update([0]);
    digest.update(normalized.as_bytes());
    let hash = format!("{:x}", digest.finalize());
    format!("unknown:{}", &hash[..16])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static EXTERNAL_CORPUS_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn registry_contains_exactly_the_product_systems() {
        let registry = bundled_domain_registry().unwrap();
        assert_eq!(registry.packs.len(), 33);
        assert!(registry.packs.iter().all(|pack| {
            pack.version == "1.2.0"
                && pack.supported_engine_range == ">=1.0.0"
                && pack.engine_compatibility.strategy == "evidence-gated-auto-generalization-v1"
                && !pack.engine_compatibility.version_aliases.is_empty()
        }));
        assert!(registry.packs.iter().any(|pack| pack.system_id == "map"));
        assert!(registry
            .packs
            .iter()
            .any(|pack| pack.system_id == "cross_server"));
    }

    #[test]
    fn engine_aliases_are_explicit_and_do_not_guess_vendor_strings() {
        let compatibility = bundled_domain_registry().unwrap().packs[0]
            .engine_compatibility
            .clone();
        assert_eq!(
            normalize_engine_version("1.2.3", &compatibility)
                .unwrap()
                .to_string(),
            "1.2.3"
        );
        assert_eq!(
            normalize_engine_version("v2.7.4", &compatibility)
                .unwrap()
                .to_string(),
            "2.7.4"
        );
        assert_eq!(
            normalize_engine_version("3.9", &compatibility)
                .unwrap()
                .to_string(),
            "3.9.0"
        );
        assert!(normalize_engine_version("V8M2", &compatibility)
            .unwrap_err()
            .starts_with("DOMAIN_ENGINE_VERSION_UNVERIFIED:"));
    }

    #[test]
    fn unknown_engine_is_readonly_and_apply_rechecks_compatibility() {
        let base = std::env::temp_dir().join(format!(
            "mir3-engine-gate-{}-{}",
            std::process::id(),
            crate::now_millis()
        ));
        let root = base.join("木立");
        let relative = "客户端/dev/Level/Level.txt";
        std::fs::create_dir_all(root.join("客户端/dev/Level")).unwrap();
        std::fs::create_dir_all(root.join("引擎/Mir200")).unwrap();
        std::fs::write(root.join(relative), "level=1\nrequiredExperience=100\n").unwrap();

        let store = DomainStore::new(base.join("data")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let files = store
            .query_domain_files(
                &project.id,
                "level",
                &DomainFileQuery {
                    text: String::new(),
                    limit: Some(10),
                    offset: None,
                },
            )
            .unwrap();
        assert_eq!(files[0].access, "readonly");
        let unknown = store.open_draft(&project.id, "unknown engine").unwrap();
        store
            .bind_draft_domain(&project.id, &unknown.id, "level", "1.2.0", None)
            .unwrap();
        assert!(store
            .patch_draft(
                &project.id,
                &unknown.id,
                0,
                &[crate::DraftChangeInput {
                    path: relative.to_string(),
                    content: Some("level=1\nrequiredExperience=200\n".to_string()),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap_err()
            .starts_with("DOMAIN_ENGINE_VERSION_UNVERIFIED:"));

        std::fs::write(root.join("引擎/version.txt"), "v2.7.4\n").unwrap();
        let project = store.validate_project(&project.id).unwrap();
        let draft = store.open_draft(&project.id, "recognized engine").unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "level", "1.2.0", None)
            .unwrap();
        let preview = store
            .patch_draft(
                &project.id,
                &draft.id,
                0,
                &[crate::DraftChangeInput {
                    path: relative.to_string(),
                    content: Some("level=1\nrequiredExperience=200\n".to_string()),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap();

        std::fs::write(root.join("引擎/version.txt"), "0.9.0\n").unwrap();
        store.validate_project(&project.id).unwrap();
        assert!(store
            .apply_draft(
                &project.id,
                &draft.id,
                preview.draft.revision,
                &preview.diff_hash,
            )
            .unwrap_err()
            .starts_with("DOMAIN_ENGINE_INCOMPATIBLE:"));

        std::fs::write(root.join("引擎/version.txt"), "1.0\n").unwrap();
        store.validate_project(&project.id).unwrap();
        store
            .apply_draft(
                &project.id,
                &draft.id,
                preview.draft.revision,
                &preview.diff_hash,
            )
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join(relative)).unwrap(),
            "level=1\nrequiredExperience=200\n"
        );
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn unknown_extensions_are_readonly() {
        let registry = bundled_domain_registry().unwrap();
        let map = registry
            .packs
            .iter()
            .find(|manifest| manifest.system_id == "map")
            .unwrap();
        assert_eq!(access_for(map, Some("map")), "structured");
        assert_eq!(access_for(map, Some("txt")), "editable");
        assert!(matches_projection(
            map,
            "引擎/Mir200/map/0.map",
            Some("map"),
            None,
        ));
    }

    #[test]
    fn ascii_projection_selectors_require_word_boundaries() {
        let registry = bundled_domain_registry().unwrap();
        let shop = registry
            .packs
            .iter()
            .find(|manifest| manifest.system_id == "shop")
            .unwrap();
        assert!(!matches_projection(
            shop,
            "引擎/Mir200/mimalloc.dll",
            Some("dll"),
            None,
        ));
        assert!(matches_projection(
            shop,
            "引擎/Mir200/Envir/Data/cfg_store.xls",
            Some("xls"),
            None,
        ));

        let item = registry
            .packs
            .iter()
            .find(|manifest| manifest.system_id == "item")
            .unwrap();
        assert!(matches_projection(
            item,
            "客户端/dev/data/cfg_item.xls",
            Some("xls"),
            None,
        ));
        assert!(!matches_projection(
            item,
            "客户端/dev/data/legitimate.txt",
            Some("txt"),
            None,
        ));
    }

    #[test]
    fn unclaimed_files_remain_visible_and_readonly() {
        let base = std::env::temp_dir().join(format!("mir3-unclaimed-{}", std::process::id()));
        let root = base.join("木立");
        std::fs::create_dir_all(root.join("客户端/dev/misc")).unwrap();
        std::fs::create_dir_all(root.join("引擎/Mir200")).unwrap();
        std::fs::write(root.join("客户端/dev/misc/opaque.xyz"), b"opaque").unwrap();
        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let files = store
            .query_unclaimed_domain_files(
                &project.id,
                &DomainFileQuery {
                    text: "opaque".to_string(),
                    limit: Some(10),
                    offset: None,
                },
            )
            .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].ownership, "unknown");
        assert_eq!(files[0].access, "readonly");
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn dependency_files_are_visible_but_never_writable_from_the_consumer() {
        let base = std::env::temp_dir().join(format!("mir3-dependency-{}", std::process::id()));
        let root = base.join("木立");
        std::fs::create_dir_all(root.join("客户端/dev/Quest")).unwrap();
        std::fs::create_dir_all(root.join("引擎/Mir200/Item")).unwrap();
        std::fs::write(root.join("客户端/dev/Quest/Main.lua"), "questId=Q1\n").unwrap();
        std::fs::write(root.join("引擎/Mir200/Item/Items.txt"), "itemId=I1\n").unwrap();
        let store = DomainStore::new(base.join("data")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let files = store
            .query_domain_files(
                &project.id,
                "quest",
                &DomainFileQuery {
                    text: String::new(),
                    limit: Some(100),
                    offset: None,
                },
            )
            .unwrap();
        let dependency = files
            .iter()
            .find(|file| file.path.contains("Item/Items.txt"))
            .unwrap();
        assert_eq!(dependency.ownership, "dependency");
        assert_eq!(dependency.access, "readonly");
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn draft_overlay_rejects_invalid_fields_and_preserves_project_bytes() {
        let base = std::env::temp_dir().join(format!(
            "mir3-draft-overlay-{}-{}",
            std::process::id(),
            crate::now_millis()
        ));
        let root = base.join("木立");
        let level_path = "客户端/dev/Level/Level.txt";
        std::fs::create_dir_all(root.join("客户端/dev/Level")).unwrap();
        std::fs::create_dir_all(root.join("引擎/Mir200/Monster")).unwrap();
        let original = b"level=1\nrequiredExperience=100\nrecommendedMonsterId=M1\n";
        std::fs::write(root.join(level_path), original).unwrap();
        std::fs::write(
            root.join("引擎/Mir200/Monster/Monster.txt"),
            b"monsterId=M1\ncombatLevel=1\nhealthPoints=10\n",
        )
        .unwrap();

        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        assert!(
            store
                .validate_domain_system(&project.id, "level")
                .unwrap()
                .valid
        );

        let draft = store
            .open_draft(&project.id, "invalid level overlay")
            .unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "level", "1.2.0", None)
            .unwrap();
        let preview = store
            .patch_draft(
                &project.id,
                &draft.id,
                0,
                &[crate::DraftChangeInput {
                    path: level_path.to_string(),
                    content: Some(
                        "level=1\nrequiredExperience=3000000001\nrecommendedMonsterId=MISSING\n"
                            .to_string(),
                    ),
                    deleted: false,
                    expected_sha256: None,
                }],
            )
            .unwrap();
        let report = store.validate_domain_draft(&project.id, &draft.id).unwrap();
        assert!(!report.valid);
        assert!(report
            .diagnostics
            .iter()
            .any(|value| value.contains("DOMAIN_RANGE_FIELD_INVALID:requiredExperience")));
        assert!(report
            .diagnostics
            .iter()
            .any(|value| value.contains("DOMAIN_REFERENCE_VALUE_MISSING:monster:MISSING")));
        assert!(report
            .diagnostics
            .iter()
            .any(|value| value.starts_with("DOMAIN_DRAFT_OVERLAY_VALIDATED:")));
        assert!(
            store
                .validate_domain_system(&project.id, "level")
                .unwrap()
                .valid
        );

        let error = store
            .apply_validated_domain_draft(
                &project.id,
                &draft.id,
                preview.draft.revision,
                &preview.diff_hash,
            )
            .unwrap_err();
        assert!(error.starts_with("DRAFT_VALIDATION_FAILED:"));
        assert_eq!(std::fs::read(root.join(level_path)).unwrap(), original);
        assert_eq!(
            store.get_draft(&project.id, &draft.id).unwrap().status,
            crate::DraftStatus::Open
        );

        store
            .patch_draft_bytes(
                &project.id,
                &draft.id,
                preview.draft.revision,
                &[crate::DraftBinaryChangeInput {
                    path: level_path.to_string(),
                    content: b"level=1\0requiredExperience=100\n".to_vec(),
                    expected_sha256: None,
                }],
            )
            .unwrap();
        let syntax_report = store.validate_domain_draft(&project.id, &draft.id).unwrap();
        assert!(!syntax_report.valid);
        assert!(syntax_report
            .diagnostics
            .iter()
            .any(|value| value.contains("SAFE_TEXT_NUL_WITHOUT_BOM")));
        assert_eq!(std::fs::read(root.join(level_path)).unwrap(), original);

        store
            .bind_draft_domain(
                &project.id,
                &draft.id,
                "level",
                "1.2.0",
                Some("overlay-composite"),
            )
            .unwrap();
        let companion = store.open_draft(&project.id, "valid companion").unwrap();
        store
            .bind_draft_domain(
                &project.id,
                &companion.id,
                "shop",
                "1.2.0",
                Some("overlay-composite"),
            )
            .unwrap();
        let invalid_preview = store.preview_draft(&project.id, &draft.id).unwrap();
        let companion_preview = store.preview_draft(&project.id, &companion.id).unwrap();
        let composite_error = store
            .apply_validated_composite_drafts(
                &project.id,
                "overlay-composite",
                &[
                    crate::CompositeDraftConfirmation {
                        draft_id: draft.id.clone(),
                        expected_revision: invalid_preview.draft.revision,
                        expected_diff_hash: invalid_preview.diff_hash,
                    },
                    crate::CompositeDraftConfirmation {
                        draft_id: companion.id,
                        expected_revision: companion_preview.draft.revision,
                        expected_diff_hash: companion_preview.diff_hash,
                    },
                ],
            )
            .unwrap_err();
        assert!(composite_error.starts_with("DRAFT_VALIDATION_FAILED:"));
        assert_eq!(std::fs::read(root.join(level_path)).unwrap(), original);
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn historical_release_keeps_old_scope_and_draft_pinned_after_two_upgrades() {
        let base = std::env::temp_dir().join(format!(
            "mir3-runtime-registry-{}-{}",
            std::process::id(),
            crate::now_millis()
        ));
        let data_root = base.join("data");
        let project_root = base.join("木立");
        std::fs::create_dir_all(project_root.join("客户端/dev/Level")).unwrap();
        std::fs::create_dir_all(project_root.join("引擎/Mir200")).unwrap();
        std::fs::write(
            project_root.join("客户端/dev/Level/Level.txt"),
            "level=1\nexp=100\n",
        )
        .unwrap();

        let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../resources/mir3-domain-packs/level");
        let domain_pack_root = base.join("domain-packs");
        let system_root = domain_pack_root.join("level");
        let releases_root = system_root.join("releases");
        std::fs::create_dir_all(&releases_root).unwrap();

        let v1_staging = base.join("level-v1");
        copy_test_directory(&bundled, &v1_staging);
        let v1_hash = hash_runtime_release(&v1_staging).unwrap();
        let v1 = RuntimeDomainPackRelease {
            version: "1.2.0".to_string(),
            directory: format!("level-{}", &v1_hash[..12]),
            hash: v1_hash,
        };
        std::fs::rename(&v1_staging, releases_root.join(&v1.directory)).unwrap();

        let v101_staging = base.join("level-v101");
        copy_test_directory(&bundled, &v101_staging);
        mutate_test_pack_contract(&v101_staging, "1.2.1", "v101");
        let v101_hash = hash_runtime_release(&v101_staging).unwrap();
        let v101 = RuntimeDomainPackRelease {
            version: "1.2.1".to_string(),
            directory: format!("level-{}", &v101_hash[..12]),
            hash: v101_hash,
        };
        std::fs::rename(&v101_staging, releases_root.join(&v101.directory)).unwrap();

        let v102_staging = base.join("level-v102");
        copy_test_directory(&bundled, &v102_staging);
        mutate_test_pack_contract(&v102_staging, "1.2.2", "v102");
        let v102_hash = hash_runtime_release(&v102_staging).unwrap();
        let v102 = RuntimeDomainPackRelease {
            version: "1.2.2".to_string(),
            directory: format!("level-{}", &v102_hash[..12]),
            hash: v102_hash,
        };
        std::fs::rename(&v102_staging, releases_root.join(&v102.directory)).unwrap();

        write_test_runtime_state(&system_root, "level", true, Some(&v1), None, Some(&v1));
        let store =
            DomainStore::new_trusted_fixture_with_domain_pack_root(&data_root, &domain_pack_root)
                .unwrap();
        let project = store.import_project(&project_root).unwrap();
        store.scan_project(&project.id, || false).unwrap();

        let draft = store.open_draft(&project.id, "pinned v1.2.0").unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "level", "1.2.0", None)
            .unwrap();
        let old_lease = store
            .issue_task_scope(
                &project.id,
                "old-task",
                &["level".to_string()],
                &["level".to_string()],
                std::slice::from_ref(&draft.id),
                serde_json::json!({"level":"1.2.0"}),
                crate::now_millis() + 60_000,
            )
            .unwrap();
        store
            .save_domain_memory(
                &project.id,
                &crate::DomainMemory {
                    id: "level-v1-memory".to_string(),
                    system_id: "level".to_string(),
                    scope: "project".to_string(),
                    kind: "rule".to_string(),
                    summary: "pinned memory".to_string(),
                    body: serde_json::json!({"maximumLevel": 80}),
                    status: "active".to_string(),
                    source_task_id: "old-task".to_string(),
                    plugin_version: "1.2.0".to_string(),
                    created_at: crate::now_millis(),
                    updated_at: crate::now_millis(),
                },
            )
            .unwrap();

        // 两次升级后 v1.2.0 已不在 current/previous/LKG，但目录仍保留给旧任务。
        write_test_runtime_state(
            &system_root,
            "level",
            true,
            Some(&v102),
            Some(&v101),
            Some(&v101),
        );
        let active = store
            .list_domain_systems()
            .unwrap()
            .into_iter()
            .find(|manifest| manifest.system_id == "level")
            .unwrap();
        assert_eq!(active.version, "1.2.2");
        assert_eq!(
            store
                .list_domain_memories(&project.id, "level", true)
                .unwrap()
                .len(),
            1
        );
        assert!(active
            .operations
            .iter()
            .any(|operation| operation.id == "scale-experience-v102"));
        let pinned_manifest = store.draft_domain_manifest(&project.id, &draft.id).unwrap();
        assert_eq!(pinned_manifest.version, "1.2.0");
        assert!(pinned_manifest
            .operations
            .iter()
            .any(|operation| operation.id == "scale-experience"));
        assert!(!pinned_manifest
            .operations
            .iter()
            .any(|operation| operation.id == "scale-experience-v102"));
        assert_eq!(
            store
                .validate_draft_capability(&project.id, &draft.id, "scale-experience")
                .unwrap()
                .id,
            "scale-experience"
        );
        assert!(store
            .authorize_task_scope(
                &project.id,
                &old_lease.token,
                Some("level"),
                Some("level"),
                Some(&draft.id),
            )
            .is_ok());
        store
            .bind_draft_domain(&project.id, &draft.id, "level", "1.2.2", None)
            .unwrap();
        assert!(store
            .authorize_task_scope(
                &project.id,
                &old_lease.token,
                Some("level"),
                Some("level"),
                Some(&draft.id),
            )
            .unwrap_err()
            .starts_with("TASK_SCOPE_DRAFT_VERSION_MISMATCH:"));
        store
            .bind_draft_domain(&project.id, &draft.id, "level", "1.2.0", None)
            .unwrap();

        let new_lease = store
            .issue_task_scope(
                &project.id,
                "new-task",
                &["level".to_string()],
                &["level".to_string()],
                &[],
                serde_json::json!({"level":"1.2.2"}),
                crate::now_millis() + 60_000,
            )
            .unwrap();
        assert_eq!(new_lease.plugin_versions["level"], "1.2.2");

        // 保留另一个可用领域，用来证明禁用包从新任务清单消失而非拖垮注册表。
        let shop_bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../resources/mir3-domain-packs/shop");
        let shop_staging = base.join("shop-v1");
        copy_test_directory(&shop_bundled, &shop_staging);
        let shop_hash = hash_runtime_release(&shop_staging).unwrap();
        let shop_release = RuntimeDomainPackRelease {
            version: "1.2.0".to_string(),
            directory: format!("shop-{}", &shop_hash[..12]),
            hash: shop_hash,
        };
        let shop_root = domain_pack_root.join("shop");
        std::fs::create_dir_all(shop_root.join("releases")).unwrap();
        std::fs::rename(
            &shop_staging,
            shop_root.join("releases").join(&shop_release.directory),
        )
        .unwrap();
        write_test_runtime_state(
            &shop_root,
            "shop",
            true,
            Some(&shop_release),
            None,
            Some(&shop_release),
        );

        // 禁用只阻止新任务发现 current，固定版本 Draft 与租约仍按原契约完成。
        write_test_runtime_state(
            &system_root,
            "level",
            false,
            Some(&v102),
            Some(&v101),
            Some(&v101),
        );
        assert!(!store
            .list_domain_systems()
            .unwrap()
            .iter()
            .any(|manifest| manifest.system_id == "level"));
        assert!(store.validate_domain_draft(&project.id, &draft.id).is_ok());
        assert!(store
            .authorize_task_scope(
                &project.id,
                &old_lease.token,
                Some("level"),
                Some("level"),
                Some(&draft.id),
            )
            .is_ok());

        // 历史目录被篡改后完整哈希与内容寻址目录不一致，旧任务必须关闭。
        write_test_runtime_state(
            &system_root,
            "level",
            true,
            Some(&v102),
            Some(&v101),
            Some(&v101),
        );
        std::fs::write(
            releases_root.join(&v1.directory).join("README.md"),
            "tampered",
        )
        .unwrap();
        assert!(store
            .runtime_manifest_at_version("level", Some("1.2.0"))
            .unwrap_err()
            .starts_with("DOMAIN_PACK_VERSION_UNAVAILABLE:"));
        assert!(store
            .list_domain_memories(&project.id, "level", true)
            .unwrap_err()
            .starts_with("MEMORY_DOMAIN_VERSION_INCOMPATIBLE:"));
        assert!(store
            .authorize_task_scope(
                &project.id,
                &old_lease.token,
                Some("level"),
                Some("level"),
                Some(&draft.id),
            )
            .is_err());
        assert!(store
            .authorize_task_scope(
                &project.id,
                &new_lease.token,
                Some("level"),
                Some("level"),
                None,
            )
            .is_ok());

        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    #[ignore = "requires MIR3_DOMAIN_CORPUS_ROOTS and the disposable-corpus acceptance runner"]
    fn external_real_project_corpus_runs_the_full_readonly_domain_matrix() {
        let raw_roots = std::env::var_os("MIR3_DOMAIN_CORPUS_ROOTS").expect(
            "MIR3_DOMAIN_CORPUS_ROOTS must be set by the disposable-corpus acceptance runner",
        );
        let _corpus_guard = EXTERNAL_CORPUS_LOCK.lock().unwrap();
        let roots = std::env::split_paths(&raw_roots).collect::<Vec<_>>();
        assert!(
            roots.len() >= 3,
            "MIR3_DOMAIN_CORPUS_ROOTS requires three project copies"
        );
        let base = std::env::temp_dir().join(format!(
            "mir3-real-corpus-{}-{}",
            std::process::id(),
            crate::now_millis()
        ));
        let store = DomainStore::new(base.join("data")).unwrap();
        let manifests = store.list_domain_systems().unwrap();
        assert_eq!(manifests.len(), 33);
        let mut signatures = std::collections::BTreeSet::new();
        for root in roots {
            let before = readonly_tree_signature(&root);
            let project = store.import_project(&root).unwrap();
            let stats = store.scan_project(&project.id, || false).unwrap();
            let mut detected = 0_usize;
            let mut validated = 0_usize;
            for manifest in &manifests {
                let files = store
                    .query_domain_files(
                        &project.id,
                        &manifest.system_id,
                        &DomainFileQuery {
                            text: String::new(),
                            limit: Some(20),
                            offset: None,
                        },
                    )
                    .unwrap();
                if !files.is_empty() {
                    detected += 1;
                    store
                        .validate_domain_system(&project.id, &manifest.system_id)
                        .unwrap();
                    validated += 1;
                }
            }
            assert!(detected > 0, "{} has no detected domains", root.display());
            assert_eq!(
                validated, detected,
                "every detected domain must be validated"
            );
            assert_eq!(readonly_tree_signature(&root), before);
            signatures.insert((stats.scanned_files, detected));
        }
        assert!(
            signatures.len() >= 3,
            "real project copies are not materially different"
        );
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    #[ignore = "requires MIR3_DOMAIN_CORPUS_ROOTS and the disposable-corpus acceptance runner"]
    fn external_real_project_corpus_applies_and_restores_verified_drafts() {
        let raw_roots = std::env::var_os("MIR3_DOMAIN_CORPUS_ROOTS").expect(
            "MIR3_DOMAIN_CORPUS_ROOTS must be set by the disposable-corpus acceptance runner",
        );
        let _corpus_guard = EXTERNAL_CORPUS_LOCK.lock().unwrap();
        let roots = std::env::split_paths(&raw_roots).collect::<Vec<_>>();
        assert!(
            roots.len() >= 3,
            "MIR3_DOMAIN_CORPUS_ROOTS requires three project copies"
        );
        let base = std::env::temp_dir().join(format!(
            "mir3-real-write-corpus-{}-{}",
            std::process::id(),
            crate::now_millis()
        ));
        let store = DomainStore::new(base.join("data")).unwrap();
        let manifests = store.list_domain_systems().unwrap();
        let mut exercised = 0_usize;
        for root in roots {
            let project = store.import_project(&root).unwrap();
            store.scan_project(&project.id, || false).unwrap();
            let mut selected = None;
            for manifest in &manifests {
                let baseline = store
                    .validate_domain_system(&project.id, &manifest.system_id)
                    .unwrap();
                if !baseline.valid || baseline.writable_files == 0 {
                    continue;
                }
                let files = store
                    .query_domain_files(
                        &project.id,
                        &manifest.system_id,
                        &DomainFileQuery {
                            text: String::new(),
                            limit: Some(500),
                            offset: None,
                        },
                    )
                    .unwrap();
                for file in files {
                    if file.access == "readonly"
                        || file.ownership == "dependency"
                        || !file.systems.contains(&manifest.system_id)
                        || file.size > 1024 * 1024
                        || !matches!(file.extension.as_deref(), Some("lua" | "txt"))
                    {
                        continue;
                    }
                    let target = root.join(&file.path);
                    let Ok(original) = std::fs::read(&target) else {
                        continue;
                    };
                    let Ok(text) = String::from_utf8(original.clone()) else {
                        continue;
                    };
                    selected = Some((manifest.clone(), file.path, target, original, text));
                    break;
                }
                if selected.is_some() {
                    break;
                }
            }
            let Some((manifest, relative, target, original, text)) = selected else {
                panic!(
                    "{} has no verified writable UTF-8 Lua/TXT domain fixture",
                    root.display()
                );
            };
            let draft = store
                .open_draft(&project.id, "真实项目副本 Draft 应用与恢复验收")
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
            let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
            let preview = store
                .patch_draft(
                    &project.id,
                    &draft.id,
                    draft.revision,
                    &[crate::DraftChangeInput {
                        path: relative,
                        content: Some(format!("{text}{newline}")),
                        deleted: false,
                        expected_sha256: None,
                    }],
                )
                .unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), original);
            let validation = store.validate_domain_draft(&project.id, &draft.id).unwrap();
            assert!(
                validation.valid,
                "{}:{} draft validation failed: {:?}",
                root.display(),
                manifest.system_id,
                validation.diagnostics
            );
            let snapshot = store
                .apply_validated_domain_draft(
                    &project.id,
                    &draft.id,
                    preview.draft.revision,
                    &preview.diff_hash,
                )
                .unwrap();
            assert_ne!(std::fs::read(&target).unwrap(), original);
            store.restore_snapshot(&project.id, &snapshot.id).unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), original);
            exercised += 1;
        }
        assert!(exercised >= 3);
        std::fs::remove_dir_all(base).ok();
    }

    fn readonly_tree_signature(root: &Path) -> String {
        fn visit(root: &Path, directory: &Path, entries: &mut Vec<String>) {
            let mut children = std::fs::read_dir(directory)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            children.sort_by_key(std::fs::DirEntry::file_name);
            for entry in children {
                let file_type = entry.file_type().unwrap();
                if file_type.is_symlink() {
                    continue;
                }
                if file_type.is_dir() {
                    visit(root, &entry.path(), entries);
                    continue;
                }
                if file_type.is_file() {
                    let metadata = entry.metadata().unwrap();
                    let relative = entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    let modified = metadata
                        .modified()
                        .ok()
                        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |value| value.as_nanos());
                    entries.push(format!("{relative}:{}:{modified}", metadata.len()));
                }
            }
        }
        let mut entries = Vec::new();
        visit(root, root, &mut entries);
        format!("{:x}", Sha256::digest(entries.join("\n").as_bytes()))
    }

    fn copy_test_directory(source: &Path, destination: &Path) {
        std::fs::create_dir_all(destination).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let target = destination.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_test_directory(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    fn mutate_test_pack_contract(root: &Path, version: &str, suffix: &str) {
        let manifest_path = root.join("domain.json");
        let mut manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        manifest["version"] = serde_json::Value::String(version.to_string());
        for field in ["operations", "capabilities"] {
            let entries = manifest[field].as_array_mut().unwrap();
            let entry = entries
                .iter_mut()
                .find(|entry| entry["id"] == "scale-experience")
                .unwrap();
            let operation_id = format!("scale-experience-{suffix}");
            entry["id"] = serde_json::Value::String(operation_id.clone());
            for step in entry["steps"].as_array_mut().unwrap() {
                if step["operation"] == "scale-experience" {
                    step["operation"] = serde_json::Value::String(operation_id.clone());
                }
            }
        }
        manifest["validators"][0]["id"] =
            serde_json::Value::String(format!("level-runtime-validator-{suffix}"));
        std::fs::write(
            &manifest_path,
            format!("{}\n", serde_json::to_string_pretty(&manifest).unwrap()),
        )
        .unwrap();

        let package_path = root.join("package.json");
        let mut package: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&package_path).unwrap()).unwrap();
        package["version"] = serde_json::Value::String(version.to_string());
        std::fs::write(
            package_path,
            format!("{}\n", serde_json::to_string_pretty(&package).unwrap()),
        )
        .unwrap();
        std::fs::write(
            root.join("CHANGELOG.md"),
            format!("# Changelog\n\n## {version}\n\n- Runtime registry fixture.\n"),
        )
        .unwrap();
    }

    fn write_test_runtime_state(
        system_root: &Path,
        system_id: &str,
        enabled: bool,
        current: Option<&RuntimeDomainPackRelease>,
        previous: Option<&RuntimeDomainPackRelease>,
        lkg: Option<&RuntimeDomainPackRelease>,
    ) {
        std::fs::create_dir_all(system_root).unwrap();
        let state = RuntimeDomainPackState {
            schema_version: DOMAIN_PACK_STATE_SCHEMA,
            system_id: system_id.to_string(),
            enabled,
            current: current.cloned(),
            previous: previous.cloned(),
            lkg: lkg.cloned(),
        };
        std::fs::write(
            system_root.join("state.json"),
            format!("{}\n", serde_json::to_string_pretty(&state).unwrap()),
        )
        .unwrap();
    }
}
