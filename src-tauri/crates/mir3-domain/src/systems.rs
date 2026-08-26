//! 33 个领域包的稳定契约、真实文件投影与通用能力目录。
//!
//! 领域包只描述安全的资源与操作，不拥有 Harness 生命周期，也不能直接写项目。

use crate::DomainStore;
use rusqlite::{params, OptionalExtension};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
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
    pub dependencies: Vec<String>,
    pub capabilities: Vec<OfficialCapability>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainDependencyGraph {
    pub system_id: String,
    pub direct: Vec<String>,
    pub transitive: Vec<String>,
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
        let mut diagnostics = Vec::new();
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
            let systems = registry
                .packs
                .iter()
                .filter(|candidate| {
                    matches_projection(candidate, &path, extension.as_deref(), content.as_deref())
                })
                .map(|candidate| candidate.system_id.clone())
                .collect::<Vec<_>>();
            let owns_file = systems.iter().any(|system| system == system_id);
            let dependency_file = !owns_file
                && systems
                    .iter()
                    .any(|system| manifest.dependencies.contains(system));
            if !owns_file && !dependency_file {
                continue;
            }
            let ownership = if dependency_file {
                "dependency"
            } else if systems.len() > 1 {
                "shared"
            } else {
                "owned"
            };
            let access = if dependency_file {
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
        let description = self.describe_domain_system(project_id, system_id)?;
        let registry = self.runtime_domain_registry()?;
        let known: BTreeSet<&str> = registry
            .packs
            .iter()
            .map(|pack| pack.system_id.as_str())
            .collect();
        let missing_dependencies = description
            .manifest
            .dependencies
            .iter()
            .filter(|dependency| !known.contains(dependency.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let files = self.query_domain_files(
            project_id,
            system_id,
            &DomainFileQuery {
                text: String::new(),
                limit: Some(10_000),
                offset: None,
            },
        )?;
        let mut validators = Vec::with_capacity(description.manifest.validators.len());
        let owned_files = files
            .iter()
            .filter(|file| file.ownership != "dependency")
            .cloned()
            .collect::<Vec<_>>();
        for validator in &description.manifest.validators {
            validators.push(self.execute_domain_validator(
                project_id,
                &description.manifest,
                &owned_files,
                validator,
                &missing_dependencies,
            ));
        }
        let mut diagnostics = description.diagnostics.clone();
        if !missing_dependencies.is_empty() {
            diagnostics.push("DOMAIN_DEPENDENCY_MISSING".to_string());
        }
        diagnostics.extend(
            validators
                .iter()
                .flat_map(|validator| validator.diagnostics.iter().cloned()),
        );
        Ok(DomainValidationReport {
            system_id: system_id.to_string(),
            valid: missing_dependencies.is_empty() && validators.iter().all(|value| value.valid),
            owned_files: description.owned_files + description.shared_files,
            writable_files: description.writable_files,
            readonly_files: description.readonly_files,
            missing_dependencies,
            validators,
            diagnostics,
        })
    }

    fn execute_domain_validator(
        &self,
        project_id: &str,
        manifest: &DomainManifest,
        files: &[DomainFileRecord],
        validator: &DomainValidatorContract,
        missing_dependencies: &[String],
    ) -> DomainValidatorResult {
        let mut valid = true;
        let mut checked = 0;
        let mut diagnostics = Vec::new();
        match validator.kind.as_str() {
            "syntax" => {
                for file in files.iter().filter(|file| {
                    file.access != "readonly"
                        && (validator.extensions.is_empty()
                            || file.extension.as_ref().is_some_and(|extension| {
                                validator
                                    .extensions
                                    .iter()
                                    .any(|value| value.eq_ignore_ascii_case(extension))
                            }))
                }) {
                    checked += 1;
                    let result = match file.extension.as_deref().unwrap_or_default() {
                        extension if extension.eq_ignore_ascii_case("xls") => {
                            self.safe_xls_open(project_id, &file.path).map(|_| ())
                        }
                        extension if extension.eq_ignore_ascii_case("map") => self
                            .map_resource_open(project_id, &file.path, None, None)
                            .map(|_| ()),
                        _ => Ok(()),
                    };
                    if let Err(error) = result {
                        valid = false;
                        diagnostics.push(format!("{}:{error}", file.path));
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
                    let content = self.indexed_file_content(project_id, &file.path);
                    if file.resource_id
                        != stable_resource_id(manifest, &file.path, content.as_deref())
                        || !matches!(file.access.as_str(), "editable" | "structured" | "readonly")
                    {
                        valid = false;
                        diagnostics.push(format!("DOMAIN_SCHEMA_FILE_INVALID:{}", file.path));
                    }
                }
            }
            "uniqueness" | "unique-range" => {
                let mut paths = BTreeSet::new();
                let mut resource_ids = BTreeSet::new();
                checked = files.len();
                for file in files {
                    if !paths.insert(file.path.to_lowercase())
                        || !resource_ids.insert(file.resource_id.as_str())
                    {
                        valid = false;
                        diagnostics.push(format!("DOMAIN_UNIQUENESS_CONFLICT:{}", file.path));
                    }
                }
                if let Some(fields) = validator.fields.as_array() {
                    for field in fields.iter().filter_map(serde_json::Value::as_str) {
                        let mut values = BTreeSet::new();
                        for file in files {
                            if let Some(content) = self.indexed_file_content(project_id, &file.path)
                            {
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
                    let maximum = match file.extension.as_deref().unwrap_or_default() {
                        extension if extension.eq_ignore_ascii_case("map") => 64 * 1024 * 1024,
                        extension if extension.eq_ignore_ascii_case("xls") => 20 * 1024 * 1024,
                        _ => 16 * 1024 * 1024,
                    };
                    if file.size > maximum {
                        valid = false;
                        diagnostics.push(format!("DOMAIN_FILE_RANGE_EXCEEDED:{}", file.path));
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
                            if let Some(content) = self.indexed_file_content(project_id, &file.path)
                            {
                                for value in extract_field_values(&content, field) {
                                    checked += 1;
                                    if value
                                        .parse::<f64>()
                                        .is_ok_and(|value| value < minimum || value > maximum)
                                    {
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
                        .filter_map(|file| self.indexed_file_content(project_id, &file.path))
                        .flat_map(|content| extract_field_values(&content, &reference.field))
                        .collect::<BTreeSet<_>>();
                    if referenced_values.is_empty() {
                        continue;
                    }
                    let dependency_content = self
                        .query_domain_files(
                            project_id,
                            &reference.system_id,
                            &DomainFileQuery {
                                text: String::new(),
                                limit: Some(10_000),
                                offset: None,
                            },
                        )
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|file| self.indexed_file_content(project_id, &file.path))
                        .collect::<Vec<_>>()
                        .join("\n");
                    for value in referenced_values {
                        checked += 1;
                        if reference.required && !dependency_content.contains(&value) {
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
                let client = files.iter().any(|file| file.role == "client");
                let engine = files.iter().any(|file| file.role == "engine");
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
                            .filter(|file| file.role == role)
                            .filter_map(|file| self.indexed_file_content(project_id, &file.path))
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
                }
            }
            "runtime-diagnostics" => {
                checked = files.len();
                if files.is_empty() {
                    diagnostics.push("DOMAIN_RUNTIME_NO_MATCHED_FILES".to_string());
                }
                if files.iter().all(|file| file.access == "readonly") && !files.is_empty() {
                    diagnostics.push("DOMAIN_RUNTIME_READONLY_ONLY".to_string());
                }
                if validator.rule.is_empty() || validator.target.is_empty() {
                    valid = false;
                    diagnostics.push("DOMAIN_RUNTIME_RULE_INVALID".to_string());
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
            files: vec![file],
            dependency_systems: manifest.dependencies.clone(),
            projection,
            diagnostics,
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
        let extension = std::path::Path::new(path)
            .extension()
            .and_then(|value| value.to_str());
        if !matches_projection(&manifest, path, extension, None) {
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
                normalized.contains(&keyword)
                    || manifest.file_projection.path_aliases.iter().any(|alias| {
                        normalized
                            .replace(&alias.to.to_lowercase(), &alias.from.to_lowercase())
                            .contains(&keyword)
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
                        content.contains(&fingerprint.contains)
                    } else {
                        content
                            .to_lowercase()
                            .contains(&fingerprint.contains.to_lowercase())
                    }
                })
            })
}

fn globish_matches(path: &str, selector: &str) -> bool {
    let needle = selector
        .replace('\\', "/")
        .to_lowercase()
        .trim_matches('*')
        .to_string();
    !needle.is_empty() && path.contains(&needle)
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

    #[test]
    fn registry_contains_exactly_the_product_systems() {
        let registry = bundled_domain_registry().unwrap();
        assert_eq!(registry.packs.len(), 33);
        assert!(registry.packs.iter().any(|pack| pack.system_id == "map"));
        assert!(registry
            .packs
            .iter()
            .any(|pack| pack.system_id == "cross_server"));
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
    fn unclaimed_files_remain_visible_and_readonly() {
        let base = std::env::temp_dir().join(format!("mir3-unclaimed-{}", std::process::id()));
        let root = base.join("木立");
        std::fs::create_dir_all(root.join("客户端/dev/misc")).unwrap();
        std::fs::create_dir_all(root.join("引擎/Mir200")).unwrap();
        std::fs::write(root.join("客户端/dev/misc/opaque.xyz"), b"opaque").unwrap();
        let store = DomainStore::new(base.join("data")).unwrap();
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
    fn runtime_registry_switches_contracts_and_pins_draft_version() {
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
            version: "1.0.0".to_string(),
            directory: format!("level-{}", &v1_hash[..12]),
            hash: v1_hash,
        };
        std::fs::rename(&v1_staging, releases_root.join(&v1.directory)).unwrap();

        let v2_staging = base.join("level-v2");
        copy_test_directory(&bundled, &v2_staging);
        mutate_test_pack_contract(&v2_staging, "1.1.0");
        let v2_hash = hash_runtime_release(&v2_staging).unwrap();
        let v2 = RuntimeDomainPackRelease {
            version: "1.1.0".to_string(),
            directory: format!("level-{}", &v2_hash[..12]),
            hash: v2_hash,
        };
        std::fs::rename(&v2_staging, releases_root.join(&v2.directory)).unwrap();

        write_test_runtime_state(&system_root, Some(&v2), Some(&v1), Some(&v1));
        let store = DomainStore::new_with_domain_pack_root(&data_root, &domain_pack_root).unwrap();
        let project = store.import_project(&project_root).unwrap();
        store.scan_project(&project.id, || false).unwrap();

        let active = store
            .list_domain_systems()
            .unwrap()
            .into_iter()
            .find(|manifest| manifest.system_id == "level")
            .unwrap();
        assert_eq!(active.version, "1.1.0");
        assert!(active
            .operations
            .iter()
            .any(|operation| operation.id == "scale-experience-v2"));
        assert!(active
            .validators
            .iter()
            .any(|validator| validator.id == "level-runtime-validator-v2"));

        let draft = store.open_draft(&project.id, "pinned v2").unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "level", "1.1.0", None)
            .unwrap();
        assert_eq!(
            store
                .validate_draft_capability(&project.id, &draft.id, "scale-experience-v2")
                .unwrap()
                .id,
            "scale-experience-v2"
        );

        // 回滚只改变新任务所见 current；旧 Draft 继续使用 previous 中固定的 v2。
        write_test_runtime_state(&system_root, Some(&v1), Some(&v2), Some(&v1));
        let rolled_back = store
            .list_domain_systems()
            .unwrap()
            .into_iter()
            .find(|manifest| manifest.system_id == "level")
            .unwrap();
        assert_eq!(rolled_back.version, "1.0.0");
        assert!(!rolled_back
            .validators
            .iter()
            .any(|validator| validator.id == "level-runtime-validator-v2"));
        assert_eq!(
            store
                .validate_draft_capability(&project.id, &draft.id, "scale-experience-v2")
                .unwrap()
                .id,
            "scale-experience-v2"
        );

        // current 内容损坏时只退到经哈希校验的 LKG，不能继续执行损坏契约。
        write_test_runtime_state(&system_root, Some(&v2), Some(&v1), Some(&v1));
        std::fs::write(
            releases_root.join(&v2.directory).join("README.md"),
            "tampered",
        )
        .unwrap();
        let recovered = store
            .list_domain_systems()
            .unwrap()
            .into_iter()
            .find(|manifest| manifest.system_id == "level")
            .unwrap();
        assert_eq!(recovered.version, "1.0.0");

        std::fs::remove_dir_all(base).ok();
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

    fn mutate_test_pack_contract(root: &Path, version: &str) {
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
            entry["id"] = serde_json::Value::String("scale-experience-v2".to_string());
            for step in entry["steps"].as_array_mut().unwrap() {
                if step["operation"] == "scale-experience" {
                    step["operation"] =
                        serde_json::Value::String("scale-experience-v2".to_string());
                }
            }
        }
        manifest["validators"][0]["id"] =
            serde_json::Value::String("level-runtime-validator-v2".to_string());
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
        current: Option<&RuntimeDomainPackRelease>,
        previous: Option<&RuntimeDomainPackRelease>,
        lkg: Option<&RuntimeDomainPackRelease>,
    ) {
        std::fs::create_dir_all(system_root).unwrap();
        let state = RuntimeDomainPackState {
            schema_version: DOMAIN_PACK_STATE_SCHEMA,
            system_id: "level".to_string(),
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
