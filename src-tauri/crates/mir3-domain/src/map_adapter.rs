//! 地图二进制算法到统一领域 Draft 的安全适配层。
//!
//! 这里只提供可逆的解析和 Draft 写入原语，不维护地图专属 Session，也不绕过
//! Kernel 的 revision、Diff、校验和确认链路。

use crate::{DomainStore, DraftBinaryChangeInput, DraftPreview};
use mir3_map::{MapChunk, MapDocument, MapEditOperation, MapHeader};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_MAP_BYTES: u64 = 64 * 1024 * 1024;
const MAX_MAP_OPERATIONS: usize = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapResourceOpen {
    pub path: String,
    pub header: MapHeader,
    pub chunk: Option<MapChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapDraftOperation {
    pub path: String,
    pub expected_sha256: String,
    pub operations: Vec<MapEditOperation>,
}

impl DomainStore {
    pub fn map_resource_open(
        &self,
        project_id: &str,
        path: &str,
        draft_id: Option<&str>,
        chunk: Option<(u16, u16, u16)>,
    ) -> Result<MapResourceOpen, String> {
        validate_map_path(path)?;
        let bytes = match draft_id {
            Some(draft_id) => self
                .draft_change_bytes(project_id, draft_id, path)?
                .unwrap_or(self.read_map_source(project_id, path)?),
            None => self.read_map_source(project_id, path)?,
        };
        let document = MapDocument::parse(bytes)?;
        let selected = chunk
            .map(|(x, y, size)| document.chunk(x, y, size))
            .transpose()?;
        Ok(MapResourceOpen {
            path: path.to_string(),
            header: document.header().clone(),
            chunk: selected,
        })
    }

    pub fn map_draft_operate(
        &self,
        project_id: &str,
        draft_id: &str,
        expected_revision: i64,
        operation: &MapDraftOperation,
    ) -> Result<DraftPreview, String> {
        validate_map_path(&operation.path)?;
        if operation.operations.is_empty() || operation.operations.len() > MAX_MAP_OPERATIONS {
            return Err(
                "MAP_OPERATION_COUNT_INVALID: operations must contain 1..20000 edits".to_string(),
            );
        }
        self.assert_draft_path_writable(project_id, draft_id, &operation.path)?;
        let draft_source = self.draft_change_bytes(project_id, draft_id, &operation.path)?;
        let had_draft_source = draft_source.is_some();
        let source = match draft_source {
            Some(bytes) => bytes,
            None => self.read_map_source(project_id, &operation.path)?,
        };
        if hash_bytes(&source) != operation.expected_sha256 {
            return Err("MAP_SOURCE_CONFLICT: map changed since it was opened".to_string());
        }
        let mut document = MapDocument::parse(source)?;
        document.apply(&operation.operations)?;
        let content = document.into_bytes();
        self.patch_draft_bytes(
            project_id,
            draft_id,
            expected_revision,
            &[DraftBinaryChangeInput {
                path: operation.path.clone(),
                content,
                expected_sha256: (!had_draft_source).then(|| operation.expected_sha256.clone()),
            }],
        )
    }

    fn read_map_source(&self, project_id: &str, path: &str) -> Result<Vec<u8>, String> {
        let project = self.get_project(project_id)?;
        let root = fs::canonicalize(&project.root)
            .map_err(|error| format!("PROJECT_PATH_INVALID: {error}"))?;
        let target = safe_target(&root, path)?;
        let metadata = fs::metadata(&target)
            .map_err(|error| format!("MAP_RESOURCE_METADATA_FAILED: {error}"))?;
        if metadata.len() > MAX_MAP_BYTES {
            return Err("MAP_RESOURCE_TOO_LARGE: map exceeds 64 MiB".to_string());
        }
        fs::read(target).map_err(|error| format!("MAP_RESOURCE_READ_FAILED: {error}"))
    }
}

fn validate_map_path(path: &str) -> Result<(), String> {
    let value = Path::new(path);
    if value.is_absolute()
        || value
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || !path.to_ascii_lowercase().ends_with(".map")
    {
        return Err("MAP_PATH_INVALID: expected a relative .map path".to_string());
    }
    Ok(())
}

fn safe_target(root: &Path, path: &str) -> Result<PathBuf, String> {
    validate_map_path(path)?;
    let target = root.join(path);
    let parent = target
        .parent()
        .ok_or_else(|| "MAP_PATH_INVALID: missing parent".to_string())?;
    let canonical_parent =
        fs::canonicalize(parent).map_err(|error| format!("MAP_PATH_INVALID: {error}"))?;
    if !canonical_parent.starts_with(root) {
        return Err("MAP_PATH_OUTSIDE: map escapes project root".to_string());
    }
    Ok(target)
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir3_map::MapEditOperation;

    #[test]
    fn map_edits_use_the_generic_scoped_draft_without_touching_source() {
        let base = std::env::temp_dir().join(format!("mir3-map-adapter-{}", std::process::id()));
        let root = base.join("木立");
        let relative = "引擎/Mir200/map/0.map";
        fs::create_dir_all(root.join("客户端/dev")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200/map")).unwrap();
        let mut bytes = vec![0_u8; 28 + 4 * 3 + 16 * 14];
        bytes[22..24].copy_from_slice(&4_u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&4_u16.to_le_bytes());
        fs::write(root.join(relative), &bytes).unwrap();
        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let project = store.import_project(&root).unwrap();
        store.scan_project(&project.id, || false).unwrap();
        let resources = store
            .query_domain_resources(&project.id, "map", &crate::DomainResourceQuery::default())
            .unwrap();
        let resource = store
            .get_domain_resource(&project.id, "map", &resources[0].id)
            .unwrap();
        assert_eq!(resource.projection["kind"], "map");
        assert_eq!(resource.projection["header"]["width"], 4);
        assert_eq!(
            resource.projection["initialChunk"]["cells"]
                .as_array()
                .unwrap()
                .len(),
            16
        );
        let draft = store.open_draft(&project.id, "编辑地图碰撞").unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "map", "1.3.0", None)
            .unwrap();
        let opened = store
            .map_resource_open(&project.id, relative, None, Some((0, 0, 4)))
            .unwrap();
        assert_eq!(opened.chunk.unwrap().cells.len(), 16);
        let preview = store
            .map_draft_operate(
                &project.id,
                &draft.id,
                0,
                &MapDraftOperation {
                    path: relative.to_string(),
                    expected_sha256: opened.header.source_sha256,
                    operations: vec![MapEditOperation::SetCollision {
                        x: 1,
                        y: 1,
                        walkable: true,
                        front_blocked: false,
                    }],
                },
            )
            .unwrap();
        assert_eq!(preview.changes.len(), 1);
        assert_eq!(fs::read(root.join(relative)).unwrap(), bytes);
        fs::remove_dir_all(base).ok();
    }
}
