//! 官方领域包远程候选的下载、验签与安全暂存。
//!
//! 远程能力只有在构建时同时注入 HTTPS 索引地址和 Ed25519 公钥后才启用；
//! 下载结果只会进入现有 candidate，不会绕过用户确认或激活 canary。

use super::system;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use futures_util::StreamExt;
use reqwest::Url;
use ring::signature::{UnparsedPublicKey, ED25519};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

const INDEX_SCHEMA_VERSION: u32 = 1;
const SIGNATURE_SCHEMA_VERSION: u32 = 1;
const SUPPORTED_PACK_SCHEMA_VERSION: u32 = 1;
const MAX_INDEX_BYTES: u64 = 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 4096;
const COMPILED_INDEX_URL: Option<&str> = option_env!("MIR3_DOMAIN_PACK_INDEX_URL");
const COMPILED_PUBLIC_KEY: Option<&str> = option_env!("MIR3_DOMAIN_PACK_ED25519_PUBLIC_KEY");

#[derive(Debug, Clone)]
struct RemoteConfig {
    index_url: Url,
    public_key: [u8; 32],
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemoteUpdateIndex {
    schema_version: u32,
    releases: Vec<RemoteRelease>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemoteRelease {
    system_id: String,
    version: String,
    kernel_api_range: String,
    supported_engine_range: String,
    manifest_schema_version: u32,
    resource_schema_version: u32,
    capability_schema_version: u32,
    memory_schema_version: u32,
    archive_url: String,
    archive_size: u64,
    archive_sha256: String,
    signature: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SignedRelease<'a> {
    signature_schema_version: u32,
    system_id: &'a str,
    version: &'a str,
    kernel_api_range: &'a str,
    supported_engine_range: &'a str,
    manifest_schema_version: u32,
    resource_schema_version: u32,
    capability_schema_version: u32,
    memory_schema_version: u32,
    archive_url: &'a str,
    archive_size: u64,
    archive_sha256: &'a str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainPackRemoteCandidate {
    pub system_id: String,
    pub version: String,
    pub current_version: Option<String>,
    pub archive_size: u64,
    pub archive_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainPackUpdateCheck {
    pub schema_version: u32,
    pub updates: Vec<DomainPackRemoteCandidate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StagedDescriptor {
    system_id: String,
    version: String,
    kernel_api_range: String,
    supported_engine_range: String,
    manifest_schema_version: u32,
    resource_schema_version: u32,
    capability_schema_version: u32,
    memory_schema_version: u32,
}

impl RemoteConfig {
    fn compiled() -> Result<Self, String> {
        Self::from_values(COMPILED_INDEX_URL, COMPILED_PUBLIC_KEY)
    }

    fn from_values(index_url: Option<&str>, public_key: Option<&str>) -> Result<Self, String> {
        let (Some(index_url), Some(public_key)) = (index_url, public_key) else {
            return Err(
                "DOMAIN_PACK_UPDATE_NOT_CONFIGURED: build must provide the update index and Ed25519 public key"
                    .to_string(),
            );
        };
        let index_url = validate_https_url(index_url, "index")?;
        let decoded = BASE64_STANDARD
            .decode(public_key.trim())
            .map_err(|error| format!("DOMAIN_PACK_UPDATE_KEY_INVALID: {error}"))?;
        let public_key: [u8; 32] = decoded.try_into().map_err(|_| {
            "DOMAIN_PACK_UPDATE_KEY_INVALID: Ed25519 public key must contain 32 bytes".to_string()
        })?;
        Ok(Self {
            index_url,
            public_key,
        })
    }
}

/// 查询已签名索引中比本地 current 更新的候选，不写入任何文件。
pub async fn check(
    destination_root: &Path,
    system_id: Option<&str>,
) -> Result<DomainPackUpdateCheck, String> {
    if let Some(system_id) = system_id {
        validate_system_id(system_id)?;
    }
    let config = RemoteConfig::compiled()?;
    let index = fetch_index(&config).await?;
    let mut updates = Vec::new();
    for release in index.releases {
        if system_id.is_some_and(|expected| expected != release.system_id) {
            continue;
        }
        assert_installed_system(destination_root, &release.system_id)?;
        let state = system::domain_pack_state(destination_root, &release.system_id)?.state;
        let current_version = state
            .current
            .as_ref()
            .map(|current| current.version.clone());
        if is_newer_than_current(&release.version, current_version.as_deref())? {
            updates.push(DomainPackRemoteCandidate {
                system_id: release.system_id,
                version: release.version,
                current_version,
                archive_size: release.archive_size,
                archive_sha256: release.archive_sha256,
            });
        }
    }
    updates.sort_by(|left, right| {
        left.system_id.cmp(&right.system_id).then_with(|| {
            Version::parse(&right.version)
                .ok()
                .cmp(&Version::parse(&left.version).ok())
        })
    });
    Ok(DomainPackUpdateCheck {
        schema_version: INDEX_SCHEMA_VERSION,
        updates,
    })
}

/// 下载指定版本并暂存为 candidate；激活仍必须走现有确认接口。
pub async fn stage(
    destination_root: &Path,
    system_id: &str,
    version: &str,
) -> Result<system::DomainPackStateView, String> {
    validate_system_id(system_id)?;
    validate_stable_version(version)?;
    let config = RemoteConfig::compiled()?;
    let index = fetch_index(&config).await?;
    let release = index
        .releases
        .into_iter()
        .find(|release| release.system_id == system_id && release.version == version)
        .ok_or_else(|| format!("DOMAIN_PACK_UPDATE_RELEASE_NOT_FOUND: {system_id}@{version}"))?;
    assert_installed_system(destination_root, system_id)?;
    let state = system::domain_pack_state(destination_root, system_id)?.state;
    let current_version = state
        .current
        .as_ref()
        .map(|current| current.version.as_str());
    if !is_newer_than_current(version, current_version)? {
        return Err(format!(
            "DOMAIN_PACK_UPDATE_NOT_NEWER: {system_id}@{version} is not newer than current"
        ));
    }
    let archive_url = validate_release_metadata(&release, &config)?;
    let client = http_client()?;
    let archive = fetch_limited(
        &client,
        &archive_url,
        MAX_ARCHIVE_BYTES,
        Some(release.archive_size),
        "archive",
    )
    .await?;
    validate_archive(&release, &archive, &config)?;

    fs::create_dir_all(destination_root)
        .map_err(|error| format!("DOMAIN_PACK_UPDATE_ROOT_CREATE_FAILED: {error}"))?;
    let staging = destination_root.join(format!(
        ".remote-stage-{}-{}-{}",
        std::process::id(),
        system_id,
        unique_suffix()
    ));
    remove_staging(&staging)?;
    let result = (|| {
        unpack_archive(&archive, &staging)?;
        validate_staged_descriptor(&staging, &release)?;
        system::stage_domain_pack_candidate(destination_root, &staging)?;
        system::domain_pack_state(destination_root, system_id)
    })();
    let cleanup = remove_staging(&staging);
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

async fn fetch_index(config: &RemoteConfig) -> Result<RemoteUpdateIndex, String> {
    let client = http_client()?;
    let bytes = fetch_limited(&client, &config.index_url, MAX_INDEX_BYTES, None, "index").await?;
    validate_index(&bytes, config)
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .user_agent("MIR3-Studio-Domain-Pack-Updater/1")
        .build()
        .map_err(|error| format!("DOMAIN_PACK_UPDATE_CLIENT_FAILED: {error}"))
}

async fn fetch_limited(
    client: &reqwest::Client,
    url: &Url,
    maximum: u64,
    expected_size: Option<u64>,
    label: &str,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|error| format!("DOMAIN_PACK_UPDATE_DOWNLOAD_FAILED: {label}: {error}"))?;
    if response.url() != url {
        return Err(format!(
            "DOMAIN_PACK_UPDATE_REDIRECT_FORBIDDEN: {label} changed origin or URL"
        ));
    }
    if !response.status().is_success() {
        return Err(format!(
            "DOMAIN_PACK_UPDATE_HTTP_FAILED: {label}: {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > maximum || expected_size.is_some_and(|size| size != length))
    {
        return Err(format!(
            "DOMAIN_PACK_UPDATE_SIZE_INVALID: {label} Content-Length is not allowed"
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|error| format!("DOMAIN_PACK_UPDATE_DOWNLOAD_FAILED: {label}: {error}"))?;
        if bytes.len().saturating_add(chunk.len()) as u64 > maximum {
            return Err(format!(
                "DOMAIN_PACK_UPDATE_SIZE_INVALID: {label} exceeds {maximum} bytes"
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    if expected_size.is_some_and(|size| bytes.len() as u64 != size) {
        return Err(format!(
            "DOMAIN_PACK_UPDATE_SIZE_INVALID: {label} body length differs from the signed manifest"
        ));
    }
    Ok(bytes)
}

fn validate_index(bytes: &[u8], config: &RemoteConfig) -> Result<RemoteUpdateIndex, String> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_INDEX_BYTES {
        return Err(
            "DOMAIN_PACK_UPDATE_INDEX_SIZE_INVALID: index is empty or too large".to_string(),
        );
    }
    let index: RemoteUpdateIndex = serde_json::from_slice(bytes)
        .map_err(|error| format!("DOMAIN_PACK_UPDATE_INDEX_INVALID: {error}"))?;
    if index.schema_version != INDEX_SCHEMA_VERSION {
        return Err(format!(
            "DOMAIN_PACK_UPDATE_INDEX_SCHEMA_UNSUPPORTED: {}",
            index.schema_version
        ));
    }
    if index.releases.len() > 256 {
        return Err("DOMAIN_PACK_UPDATE_INDEX_TOO_MANY_RELEASES: maximum is 256".to_string());
    }
    let mut unique = BTreeSet::new();
    for release in &index.releases {
        if !unique.insert((release.system_id.as_str(), release.version.as_str())) {
            return Err(format!(
                "DOMAIN_PACK_UPDATE_DUPLICATE_RELEASE: {}@{}",
                release.system_id, release.version
            ));
        }
        validate_release_metadata(release, config)?;
    }
    Ok(index)
}

fn validate_release_metadata(
    release: &RemoteRelease,
    config: &RemoteConfig,
) -> Result<Url, String> {
    validate_system_id(&release.system_id)?;
    validate_stable_version(&release.version)?;
    let kernel = Version::parse(system::DOMAIN_KERNEL_VERSION)
        .map_err(|error| format!("DOMAIN_KERNEL_VERSION_INVALID: {error}"))?;
    let requirement = VersionReq::parse(&release.kernel_api_range)
        .map_err(|error| format!("DOMAIN_PACK_UPDATE_KERNEL_RANGE_INVALID: {error}"))?;
    if !requirement.matches(&kernel) {
        return Err(format!(
            "DOMAIN_PACK_UPDATE_KERNEL_INCOMPATIBLE: {} does not match {}",
            release.kernel_api_range,
            system::DOMAIN_KERNEL_VERSION
        ));
    }
    VersionReq::parse(&release.supported_engine_range)
        .map_err(|error| format!("DOMAIN_PACK_UPDATE_ENGINE_RANGE_INVALID: {error}"))?;
    if [
        release.manifest_schema_version,
        release.resource_schema_version,
        release.capability_schema_version,
        release.memory_schema_version,
    ]
    .iter()
    .any(|version| *version != SUPPORTED_PACK_SCHEMA_VERSION)
    {
        return Err(
            "DOMAIN_PACK_UPDATE_SCHEMA_UNSUPPORTED: manifest/resource/capability/memory must be v1"
                .to_string(),
        );
    }
    if release.archive_size == 0 || release.archive_size > MAX_ARCHIVE_BYTES {
        return Err(format!(
            "DOMAIN_PACK_UPDATE_SIZE_INVALID: {}@{}",
            release.system_id, release.version
        ));
    }
    if !is_lower_hex_sha256(&release.archive_sha256) {
        return Err(format!(
            "DOMAIN_PACK_UPDATE_SHA256_INVALID: {}@{}",
            release.system_id, release.version
        ));
    }
    let archive_url = validate_https_url(&release.archive_url, "archive")?;
    if !same_origin(&config.index_url, &archive_url) {
        return Err(
            "DOMAIN_PACK_UPDATE_HOST_DENIED: archive must use the configured index origin"
                .to_string(),
        );
    }
    verify_signature(release, &config.public_key)?;
    Ok(archive_url)
}

fn validate_archive(
    release: &RemoteRelease,
    bytes: &[u8],
    config: &RemoteConfig,
) -> Result<(), String> {
    validate_release_metadata(release, config)?;
    if bytes.len() as u64 != release.archive_size {
        return Err("DOMAIN_PACK_UPDATE_ARCHIVE_SIZE_MISMATCH: signed size differs".to_string());
    }
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != release.archive_sha256 {
        return Err(
            "DOMAIN_PACK_UPDATE_ARCHIVE_SHA256_MISMATCH: signed digest differs".to_string(),
        );
    }
    Ok(())
}

fn verify_signature(release: &RemoteRelease, public_key: &[u8; 32]) -> Result<(), String> {
    let signature = BASE64_STANDARD
        .decode(release.signature.trim())
        .map_err(|error| format!("DOMAIN_PACK_UPDATE_SIGNATURE_INVALID: {error}"))?;
    if signature.len() != 64 {
        return Err(
            "DOMAIN_PACK_UPDATE_SIGNATURE_INVALID: Ed25519 signature must contain 64 bytes"
                .to_string(),
        );
    }
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(&signed_release_payload(release)?, &signature)
        .map_err(|_| "DOMAIN_PACK_UPDATE_SIGNATURE_INVALID: verification failed".to_string())
}

fn signed_release_payload(release: &RemoteRelease) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&SignedRelease {
        signature_schema_version: SIGNATURE_SCHEMA_VERSION,
        system_id: &release.system_id,
        version: &release.version,
        kernel_api_range: &release.kernel_api_range,
        supported_engine_range: &release.supported_engine_range,
        manifest_schema_version: release.manifest_schema_version,
        resource_schema_version: release.resource_schema_version,
        capability_schema_version: release.capability_schema_version,
        memory_schema_version: release.memory_schema_version,
        archive_url: &release.archive_url,
        archive_size: release.archive_size,
        archive_sha256: &release.archive_sha256,
    })
    .map_err(|error| format!("DOMAIN_PACK_UPDATE_SIGNATURE_PAYLOAD_FAILED: {error}"))
}

fn unpack_archive(bytes: &[u8], destination: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("DOMAIN_PACK_UPDATE_ARCHIVE_INVALID: {error}"))?;
    if archive.len() == 0 || archive.len() > MAX_ARCHIVE_FILES {
        return Err(format!(
            "DOMAIN_PACK_UPDATE_ARCHIVE_FILE_COUNT_INVALID: expected 1..{MAX_ARCHIVE_FILES} entries"
        ));
    }
    fs::create_dir_all(destination)
        .map_err(|error| format!("DOMAIN_PACK_UPDATE_UNPACK_CREATE_FAILED: {error}"))?;
    let mut total_unpacked = 0_u64;
    let mut regular_files = 0_usize;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("DOMAIN_PACK_UPDATE_ARCHIVE_ENTRY_INVALID: {error}"))?;
        let relative = safe_archive_path(entry.name())?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!(
                "DOMAIN_PACK_UPDATE_ARCHIVE_SYMLINK_FORBIDDEN: {}",
                entry.name()
            ));
        }
        let target = destination.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|error| format!("DOMAIN_PACK_UPDATE_UNPACK_CREATE_FAILED: {error}"))?;
            continue;
        }
        let extension = relative
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !matches!(extension, "json" | "md") {
            return Err(format!(
                "DOMAIN_PACK_UPDATE_ARCHIVE_FILE_TYPE_FORBIDDEN: {}",
                relative.display()
            ));
        }
        regular_files += 1;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("DOMAIN_PACK_UPDATE_UNPACK_CREATE_FAILED: {error}"))?;
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|error| format!("DOMAIN_PACK_UPDATE_UNPACK_WRITE_FAILED: {error}"))?;
        let remaining = MAX_UNPACKED_BYTES.saturating_sub(total_unpacked);
        let copied = std::io::copy(&mut entry.by_ref().take(remaining + 1), &mut output)
            .map_err(|error| format!("DOMAIN_PACK_UPDATE_UNPACK_WRITE_FAILED: {error}"))?;
        total_unpacked = total_unpacked.saturating_add(copied);
        if total_unpacked > MAX_UNPACKED_BYTES {
            return Err(format!(
                "DOMAIN_PACK_UPDATE_UNPACKED_SIZE_INVALID: exceeds {MAX_UNPACKED_BYTES} bytes"
            ));
        }
        output
            .flush()
            .map_err(|error| format!("DOMAIN_PACK_UPDATE_UNPACK_WRITE_FAILED: {error}"))?;
    }
    if regular_files == 0 {
        return Err("DOMAIN_PACK_UPDATE_ARCHIVE_EMPTY: no regular files".to_string());
    }
    Ok(())
}

fn safe_archive_path(name: &str) -> Result<PathBuf, String> {
    if name.is_empty()
        || name.len() > 512
        || name.contains('\\')
        || name.contains(':')
        || name.contains('\0')
    {
        return Err(format!("DOMAIN_PACK_UPDATE_ARCHIVE_PATH_FORBIDDEN: {name}"));
    }
    let path = Path::new(name);
    if path.components().any(|component| {
        !matches!(component, Component::Normal(_))
            && !(matches!(component, Component::CurDir) && name.ends_with('/'))
    }) {
        return Err(format!("DOMAIN_PACK_UPDATE_ARCHIVE_PATH_FORBIDDEN: {name}"));
    }
    let normalized = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value),
            _ => None,
        })
        .collect::<PathBuf>();
    if normalized.as_os_str().is_empty() {
        return Err(format!("DOMAIN_PACK_UPDATE_ARCHIVE_PATH_FORBIDDEN: {name}"));
    }
    Ok(normalized)
}

fn validate_staged_descriptor(root: &Path, release: &RemoteRelease) -> Result<(), String> {
    let descriptor: StagedDescriptor = serde_json::from_slice(
        &fs::read(root.join("domain.json"))
            .map_err(|error| format!("DOMAIN_PACK_UPDATE_MANIFEST_READ_FAILED: {error}"))?,
    )
    .map_err(|error| format!("DOMAIN_PACK_UPDATE_MANIFEST_INVALID: {error}"))?;
    if descriptor.system_id != release.system_id
        || descriptor.version != release.version
        || descriptor.kernel_api_range != release.kernel_api_range
        || descriptor.supported_engine_range != release.supported_engine_range
        || descriptor.manifest_schema_version != release.manifest_schema_version
        || descriptor.resource_schema_version != release.resource_schema_version
        || descriptor.capability_schema_version != release.capability_schema_version
        || descriptor.memory_schema_version != release.memory_schema_version
    {
        return Err(
            "DOMAIN_PACK_UPDATE_MANIFEST_MISMATCH: signed metadata differs from domain.json"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_https_url(raw: &str, label: &str) -> Result<Url, String> {
    let url = Url::parse(raw)
        .map_err(|error| format!("DOMAIN_PACK_UPDATE_URL_INVALID: {label}: {error}"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(format!(
            "DOMAIN_PACK_UPDATE_URL_INVALID: {label} must be an HTTPS URL without credentials or fragment"
        ));
    }
    Ok(url)
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn validate_system_id(system_id: &str) -> Result<(), String> {
    if !system_id.is_empty()
        && system_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        Ok(())
    } else {
        Err(format!("DOMAIN_PACK_UPDATE_SYSTEM_ID_INVALID: {system_id}"))
    }
}

fn assert_installed_system(destination_root: &Path, system_id: &str) -> Result<(), String> {
    if destination_root.join(system_id).is_dir() {
        Ok(())
    } else {
        Err(format!(
            "DOMAIN_PACK_UPDATE_SYSTEM_UNKNOWN: {system_id} is not an installed official domain"
        ))
    }
}

fn validate_stable_version(version: &str) -> Result<Version, String> {
    let version = Version::parse(version)
        .map_err(|error| format!("DOMAIN_PACK_UPDATE_VERSION_INVALID: {error}"))?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err("DOMAIN_PACK_UPDATE_VERSION_INVALID: stable SemVer is required".to_string());
    }
    Ok(version)
}

fn is_newer_than_current(candidate: &str, current: Option<&str>) -> Result<bool, String> {
    let candidate = validate_stable_version(candidate)?;
    current
        .map(validate_stable_version)
        .transpose()
        .map(|current| current.is_none_or(|current| candidate > current))
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn remove_staging(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path)
            .map_err(|error| format!("DOMAIN_PACK_UPDATE_STAGE_REMOVE_FAILED: {error}"))?;
    }
    Ok(())
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use zip::write::FileOptions;

    #[test]
    fn missing_build_configuration_keeps_remote_updates_disabled() {
        let error = RemoteConfig::from_values(None, None).unwrap_err();
        assert!(error.starts_with("DOMAIN_PACK_UPDATE_NOT_CONFIGURED:"));
        assert!(RemoteConfig::from_values(
            Some("http://updates.example/index.json"),
            Some(&BASE64_STANDARD.encode([0_u8; 32]))
        )
        .unwrap_err()
        .starts_with("DOMAIN_PACK_UPDATE_URL_INVALID:"));
    }

    #[test]
    fn signed_index_and_archive_stage_only_a_candidate() {
        let root = test_directory("remote-candidate");
        let source =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/mir3-domain-packs/level");
        let archive = zip_directory(&source);
        let key = test_key();
        let config = test_config(&key);
        let release = signed_release(
            "level",
            "1.0.0",
            "https://updates.example/packs/level-1.0.0.zip",
            &archive,
            &key,
        );
        let index = serde_json::to_vec(&RemoteUpdateIndex {
            schema_version: INDEX_SCHEMA_VERSION,
            releases: vec![release.clone()],
        })
        .unwrap();
        let validated = validate_index(&index, &config).unwrap();
        validate_archive(&validated.releases[0], &archive, &config).unwrap();
        let mut corrupted_archive = archive.clone();
        corrupted_archive.push(0);
        assert!(validate_archive(&release, &corrupted_archive, &config)
            .unwrap_err()
            .starts_with("DOMAIN_PACK_UPDATE_ARCHIVE_SIZE_MISMATCH:"));
        let unpacked = root.join("unpacked");
        unpack_archive(&archive, &unpacked).unwrap();
        validate_staged_descriptor(&unpacked, &release).unwrap();
        let mut mismatched = release.clone();
        mismatched.version = "1.0.1".to_string();
        assert!(validate_staged_descriptor(&unpacked, &mismatched)
            .unwrap_err()
            .starts_with("DOMAIN_PACK_UPDATE_MANIFEST_MISMATCH:"));
        let state = system::stage_domain_pack_candidate(&root, &unpacked).unwrap();
        assert!(state.current.is_none());
        assert_eq!(state.candidate.unwrap().version, "1.0.0");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn index_rejects_cross_origin_and_signature_tampering() {
        let key = test_key();
        let config = test_config(&key);
        let bytes = b"fixture";
        let mut release =
            signed_release("map", "1.0.1", "https://evil.example/map.zip", bytes, &key);
        let error = validate_release_metadata(&release, &config).unwrap_err();
        assert!(error.starts_with("DOMAIN_PACK_UPDATE_HOST_DENIED:"));

        release.archive_url = "https://updates.example/map.zip".to_string();
        release.signature = BASE64_STANDARD.encode([0_u8; 64]);
        let error = validate_release_metadata(&release, &config).unwrap_err();
        assert!(error.starts_with("DOMAIN_PACK_UPDATE_SIGNATURE_INVALID:"));
    }

    #[test]
    fn archive_rejects_traversal_symlink_and_unknown_file_types() {
        let hostile = [
            zip_entry("../escape.json", b"fixture"),
            zip_symlink("link.json", "target.json"),
            zip_entry("native.dll", b"fixture"),
        ];
        for bytes in hostile {
            let root = test_directory("remote-hostile-archive");
            let error = unpack_archive(&bytes, &root).unwrap_err();
            assert!(error.contains("FORBIDDEN"));
            assert!(!root.parent().unwrap().join("escape.json").exists());
            fs::remove_dir_all(root).ok();
        }
    }

    fn test_key() -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&[7_u8; 32]).unwrap()
    }

    fn test_config(key: &Ed25519KeyPair) -> RemoteConfig {
        RemoteConfig::from_values(
            Some("https://updates.example/index.json"),
            Some(&BASE64_STANDARD.encode(key.public_key().as_ref())),
        )
        .unwrap()
    }

    fn signed_release(
        system_id: &str,
        version: &str,
        archive_url: &str,
        archive: &[u8],
        key: &Ed25519KeyPair,
    ) -> RemoteRelease {
        let mut release = RemoteRelease {
            system_id: system_id.to_string(),
            version: version.to_string(),
            kernel_api_range: "^1.0.0".to_string(),
            supported_engine_range: "*".to_string(),
            manifest_schema_version: 1,
            resource_schema_version: 1,
            capability_schema_version: 1,
            memory_schema_version: 1,
            archive_url: archive_url.to_string(),
            archive_size: archive.len() as u64,
            archive_sha256: format!("{:x}", Sha256::digest(archive)),
            signature: String::new(),
        };
        release.signature =
            BASE64_STANDARD.encode(key.sign(&signed_release_payload(&release).unwrap()));
        release
    }

    fn zip_directory(source: &Path) -> Vec<u8> {
        let mut files = Vec::new();
        collect_fixture_files(source, source, &mut files);
        files.sort();
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        for relative in files {
            writer
                .start_file(
                    relative.to_string_lossy().replace('\\', "/"),
                    FileOptions::default().unix_permissions(0o100644),
                )
                .unwrap();
            writer
                .write_all(&fs::read(source.join(relative)).unwrap())
                .unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn collect_fixture_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_fixture_files(root, &path, files);
            } else {
                files.push(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }

    fn zip_entry(name: &str, content: &[u8]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        writer
            .start_file(name, FileOptions::default().unix_permissions(0o100644))
            .unwrap();
        writer.write_all(content).unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn zip_symlink(name: &str, target: &str) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        writer
            .add_symlink(name, target, FileOptions::default())
            .unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn test_directory(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mir3-{label}-{}-{}",
            std::process::id(),
            unique_suffix()
        ))
    }
}
