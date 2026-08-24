//! Installer-embedded, platform-locked runtime baseline.
//!
//! The build step materializes only the selected target's archives under the Tauri resource
//! directory. First run verifies every archive again before extraction; the network installer is
//! retained only as a repair fallback for older or development packages without this bundle.

use crate::config;
use serde::Deserialize;
use std::fs;
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Manager};

const MANIFEST_NAME: &str = "manifest.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineComponent {
    Node,
    Core,
    Pnpm,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineCore {
    pub version: String,
    pub tag: String,
    pub commit: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineVersion {
    pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineArtifact {
    pub archive: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BaselineArtifacts {
    pub node: BaselineArtifact,
    pub core: BaselineArtifact,
    pub pnpm: BaselineArtifact,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineManifest {
    pub schema_version: u32,
    pub baseline_id: String,
    pub platform: String,
    pub target: String,
    pub validation: String,
    pub core: BaselineCore,
    pub node: BaselineVersion,
    pub pnpm: BaselineVersion,
    pub artifacts: BaselineArtifacts,
}

#[derive(Debug, Clone)]
pub struct BaselineBundle {
    root: PathBuf,
    pub manifest: BaselineManifest,
}

#[derive(Debug)]
pub struct BaselinePayload {
    pub archive: String,
    pub bytes: Vec<u8>,
}

impl BaselineBundle {
    pub fn load(app: &AppHandle) -> Result<Option<Self>, String> {
        let resource = app
            .path()
            .resource_dir()
            .map_err(|e| format!("BASELINE_RESOURCE_DIR_FAILED: {e}"))?;
        for root in [
            resource.join("resources").join("runtime-baseline"),
            resource.join("runtime-baseline"),
        ] {
            if root.join(MANIFEST_NAME).is_file() {
                return Self::load_from_root(root).map(Some);
            }
        }
        Ok(None)
    }

    fn load_from_root(root: PathBuf) -> Result<Self, String> {
        let raw = fs::read_to_string(root.join(MANIFEST_NAME))
            .map_err(|e| format!("BASELINE_MANIFEST_READ_FAILED: {e}"))?;
        let manifest: BaselineManifest =
            serde_json::from_str(&raw).map_err(|e| format!("BASELINE_MANIFEST_INVALID: {e}"))?;
        if manifest.schema_version != 1 {
            return Err(format!(
                "BASELINE_SCHEMA_UNSUPPORTED: {}",
                manifest.schema_version
            ));
        }
        let expected = expected_platform();
        if manifest.platform != expected {
            return Err(format!(
                "BASELINE_PLATFORM_MISMATCH: expected {expected}, got {}",
                manifest.platform
            ));
        }
        let expected_target = expected_target();
        if manifest.target != expected_target {
            return Err(format!(
                "BASELINE_TARGET_MISMATCH: expected {expected_target}, got {}",
                manifest.target
            ));
        }
        if !matches!(manifest.validation.as_str(), "approved" | "testing") {
            return Err(format!(
                "BASELINE_VALIDATION_INVALID: {}",
                manifest.validation
            ));
        }
        for (component, version) in [
            ("core", manifest.core.version.as_str()),
            ("node", manifest.node.version.as_str()),
            ("pnpm", manifest.pnpm.version.as_str()),
        ] {
            if version.trim().is_empty() {
                return Err(format!("BASELINE_VERSION_MISSING: {component}"));
            }
        }
        for artifact in [
            &manifest.artifacts.node,
            &manifest.artifacts.core,
            &manifest.artifacts.pnpm,
        ] {
            validate_archive_name(&artifact.archive)?;
            let path = root.join(&artifact.archive);
            let size = path
                .metadata()
                .map_err(|e| format!("BASELINE_ARCHIVE_MISSING: {}: {e}", path.display()))?
                .len();
            if size != artifact.size {
                return Err(format!(
                    "BASELINE_ARCHIVE_SIZE_MISMATCH: {} expected {}, got {size}",
                    artifact.archive, artifact.size
                ));
            }
        }
        Ok(Self { root, manifest })
    }

    pub fn read(&self, component: BaselineComponent) -> Result<BaselinePayload, String> {
        let artifact = self.artifact(component);
        let path = self.root.join(&artifact.archive);
        let bytes = fs::read(&path)
            .map_err(|e| format!("BASELINE_ARCHIVE_READ_FAILED: {}: {e}", path.display()))?;
        super::verify_sha256(&bytes, &artifact.sha256)
            .map_err(|e| format!("BASELINE_{component:?}_INTEGRITY_FAILED: {e}"))?;
        Ok(BaselinePayload {
            archive: artifact.archive.clone(),
            bytes,
        })
    }

    pub fn artifact(&self, component: BaselineComponent) -> &BaselineArtifact {
        match component {
            BaselineComponent::Node => &self.manifest.artifacts.node,
            BaselineComponent::Core => &self.manifest.artifacts.core,
            BaselineComponent::Pnpm => &self.manifest.artifacts.pnpm,
        }
    }

    pub fn record_core_install(&self, app: &AppHandle) {
        let mut setting = config::get_store_dat_setting(app);
        setting.runtime_baseline_id = Some(self.manifest.baseline_id.clone());
        setting.dsh_pkg_tag = Some(self.manifest.core.tag.clone());
        setting.dsh_pkg_commit = Some(self.manifest.core.commit.clone());
        setting.active_core = Some("app".to_string());
        config::set_store_dat_setting(app, setting);
    }
}

fn validate_archive_name(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(format!("BASELINE_ARCHIVE_NAME_INVALID: {value}"));
    }
    Ok(())
}

fn expected_platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x86_64",
        ("macos", "aarch64") => "macos-aarch64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "x86_64") => "linux-x86_64",
        _ => "unsupported",
    }
}

fn expected_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        _ => "unsupported",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn rejects_archive_path_escape() {
        assert!(validate_archive_name("../core.zip").is_err());
        assert!(validate_archive_name("nested/core.zip").is_err());
        assert!(validate_archive_name("core.zip").is_ok());
    }

    #[test]
    fn loads_and_verifies_a_platform_manifest() {
        let root = std::env::temp_dir().join(format!(
            "mir3-baseline-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("runtime")
        ));
        fs::create_dir_all(&root).unwrap();
        let data = b"locked-runtime";
        let sha = format!("{:x}", Sha256::digest(data));
        for name in ["node.zip", "core.zip", "pnpm.tgz"] {
            fs::write(root.join(name), data).unwrap();
        }
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "baselineId": "test",
            "platform": expected_platform(),
            "target": expected_target(),
            "validation": "testing",
            "core": {"version":"1", "tag":"core-1", "commit":"abc"},
            "node": {"version":"22"},
            "pnpm": {"version":"11"},
            "artifacts": {
                "node": {"archive":"node.zip", "sha256":sha, "size":data.len()},
                "core": {"archive":"core.zip", "sha256":sha, "size":data.len()},
                "pnpm": {"archive":"pnpm.tgz", "sha256":sha, "size":data.len()}
            }
        });
        fs::write(
            root.join(MANIFEST_NAME),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let bundle = BaselineBundle::load_from_root(root.clone()).unwrap();
        assert_eq!(bundle.read(BaselineComponent::Core).unwrap().bytes, data);
        fs::remove_dir_all(root).ok();
    }
}
