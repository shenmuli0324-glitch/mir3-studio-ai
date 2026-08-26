use crate::{now_millis, path_is_within, path_string, DomainStore};
use encoding_rs::GBK;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::{DirEntry, WalkDir};

const MAX_CONTENT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum FileCategory {
    Map,
    Npc,
    Monster,
    Item,
    Quest,
    Lua,
    Config,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub project_id: String,
    pub scanned_files: usize,
    pub indexed_text_files: usize,
    pub removed_files: usize,
    pub categories: BTreeMap<String, usize>,
    pub completed_at: i64,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub total_files: usize,
    pub indexed_text_files: usize,
    pub categories: BTreeMap<String, usize>,
    pub last_scan_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndexQuery {
    pub text: String,
    pub categories: Vec<String>,
    pub role: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexRecord {
    pub path: String,
    pub role: String,
    pub category: String,
    pub extension: Option<String>,
    pub size: u64,
    pub modified_at: i64,
    pub excerpt: Option<String>,
}

impl DomainStore {
    /// 增量扫描项目；不写项目目录，只更新外置 SQLite 索引。
    pub fn scan_project<F>(&self, project_id: &str, cancelled: F) -> Result<ScanSummary, String>
    where
        F: Fn() -> bool,
    {
        let project = self.get_project(project_id)?;
        let root = PathBuf::from(&project.root);
        let mut connection = self.project_connection(project_id)?;
        let transaction = connection
            .transaction()
            .map_err(|e| format!("INDEX_TRANSACTION_FAILED: {e}"))?;
        let mut seen = HashSet::new();
        let mut scanned_files = 0usize;
        let mut indexed_text_files = 0usize;
        let mut categories = BTreeMap::new();
        let mut was_cancelled = false;

        for entry in WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| !ignored_entry(entry, &root))
            .filter_map(Result::ok)
        {
            if cancelled() {
                was_cancelled = true;
                break;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let relative = path
                .strip_prefix(&root)
                .map_err(|e| format!("INDEX_RELATIVE_PATH_FAILED: {e}"))?;
            let relative_string = relative.to_string_lossy().replace('\\', "/");
            if ignored_file(&relative_string) {
                continue;
            }
            let metadata = entry
                .metadata()
                .map_err(|e| format!("INDEX_METADATA_FAILED: {}: {e}", path.display()))?;
            let role = role_for_path(relative);
            let category = classify(relative);
            let category_name = category_name(&category).to_string();
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_lowercase());
            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_millis().min(i64::MAX as u128) as i64)
                .unwrap_or_default();
            // 二进制和大型资源只记录元数据，避免扫描地图/素材时读取整文件并造成
            // 无意义的内存与磁盘压力；只有可索引文本才读取内容并计算内容哈希。
            let (sha256, content) =
                if metadata.len() <= MAX_CONTENT_BYTES && text_extension(extension.as_deref()) {
                    let bytes = fs::read(path)
                        .map_err(|e| format!("INDEX_READ_FAILED: {}: {e}", path.display()))?;
                    let content = decode_text(&bytes);
                    (content.as_ref().map(|_| hash_bytes(&bytes)), content)
                } else {
                    (None, None)
                };
            if content.is_some() {
                indexed_text_files += 1;
            }
            transaction
                .execute(
                    "INSERT INTO files(path,role,category,extension,size,modified_at,sha256,content) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
                     ON CONFLICT(path) DO UPDATE SET role=excluded.role,category=excluded.category,extension=excluded.extension,size=excluded.size,modified_at=excluded.modified_at,sha256=excluded.sha256,content=excluded.content",
                    params![
                        relative_string,
                        role,
                        category_name,
                        extension,
                        metadata.len() as i64,
                        modified_at,
                        sha256,
                        content,
                    ],
                )
                .map_err(|e| format!("INDEX_WRITE_FAILED: {e}"))?;
            seen.insert(relative_string);
            scanned_files += 1;
            *categories.entry(category_name).or_insert(0) += 1;
        }

        let mut removed_files = 0usize;
        if !was_cancelled {
            let existing = {
                let mut statement = transaction
                    .prepare("SELECT path FROM files")
                    .map_err(|e| format!("INDEX_LIST_FAILED: {e}"))?;
                let rows = statement
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(|e| format!("INDEX_LIST_FAILED: {e}"))?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("INDEX_LIST_FAILED: {e}"))?
            };
            for path in existing {
                if !seen.contains(&path) {
                    removed_files += transaction
                        .execute("DELETE FROM files WHERE path=?1", [path])
                        .map_err(|e| format!("INDEX_DELETE_FAILED: {e}"))?;
                }
            }
        }
        transaction
            .commit()
            .map_err(|e| format!("INDEX_COMMIT_FAILED: {e}"))?;
        let completed_at = now_millis();
        if !was_cancelled {
            self.update_last_scan(project_id, completed_at)?;
        }
        Ok(ScanSummary {
            project_id: project_id.to_string(),
            scanned_files,
            indexed_text_files,
            removed_files,
            categories,
            completed_at,
            cancelled: was_cancelled,
        })
    }

    pub fn index_stats(&self, project_id: &str) -> Result<IndexStats, String> {
        let connection = self.project_connection(project_id)?;
        let total_files = connection
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get::<_, i64>(0))
            .map_err(|e| format!("INDEX_STATS_FAILED: {e}"))? as usize;
        let indexed_text_files = connection
            .query_row(
                "SELECT COUNT(*) FROM files WHERE content IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| format!("INDEX_STATS_FAILED: {e}"))?
            as usize;
        let mut categories = BTreeMap::new();
        let mut statement = connection
            .prepare("SELECT category,COUNT(*) FROM files GROUP BY category ORDER BY category")
            .map_err(|e| format!("INDEX_STATS_FAILED: {e}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| format!("INDEX_STATS_FAILED: {e}"))?;
        for row in rows {
            let (category, count) = row.map_err(|e| format!("INDEX_STATS_FAILED: {e}"))?;
            categories.insert(category, count as usize);
        }
        Ok(IndexStats {
            total_files,
            indexed_text_files,
            categories,
            last_scan_at: self.get_project(project_id)?.last_scan_at,
        })
    }

    /// 查询领域索引。它只返回已分类记录和短摘录，不承担通用文件读取职责。
    pub fn query_index(
        &self,
        project_id: &str,
        query: &IndexQuery,
    ) -> Result<Vec<IndexRecord>, String> {
        let connection = self.project_connection(project_id)?;
        let text = query.text.trim();
        let pattern = format!("%{}%", text.replace('%', "\\%").replace('_', "\\_"));
        let mut statement = connection
            .prepare(
                "SELECT path,role,category,extension,size,modified_at,content FROM files
                 WHERE (?1='' OR path LIKE ?2 ESCAPE '\\' OR content LIKE ?2 ESCAPE '\\')
                 ORDER BY CASE WHEN path LIKE ?2 ESCAPE '\\' THEN 0 ELSE 1 END, category, path
                 LIMIT ?3",
            )
            .map_err(|e| format!("INDEX_QUERY_FAILED: {e}"))?;
        let limit = query.limit.unwrap_or(50).clamp(1, 200) as i64;
        let rows = statement
            .query_map(params![text, pattern, limit], |row| {
                let content: Option<String> = row.get(6)?;
                Ok(IndexRecord {
                    path: row.get(0)?,
                    role: row.get(1)?,
                    category: row.get(2)?,
                    extension: row.get(3)?,
                    size: row.get::<_, i64>(4)?.max(0) as u64,
                    modified_at: row.get(5)?,
                    excerpt: excerpt(content.as_deref(), text),
                })
            })
            .map_err(|e| format!("INDEX_QUERY_FAILED: {e}"))?;
        let allowed_categories: HashSet<String> = query
            .categories
            .iter()
            .map(|value| value.to_lowercase())
            .collect();
        let role = query.role.as_deref().map(str::to_lowercase);
        let mut records = Vec::new();
        for row in rows {
            let record = row.map_err(|e| format!("INDEX_QUERY_FAILED: {e}"))?;
            if !allowed_categories.is_empty()
                && !allowed_categories.contains(&record.category.to_lowercase())
            {
                continue;
            }
            if role
                .as_ref()
                .is_some_and(|value| value != &record.role.to_lowercase())
            {
                continue;
            }
            records.push(record);
        }
        Ok(records)
    }

    pub fn indexed_file_hash(
        &self,
        project_id: &str,
        relative_path: &str,
    ) -> Result<Option<String>, String> {
        self.project_connection(project_id)?
            .query_row(
                "SELECT sha256 FROM files WHERE path=?1",
                [relative_path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("INDEX_HASH_FAILED: {e}"))
    }
}

fn ignored_entry(entry: &DirEntry, root: &Path) -> bool {
    if entry.path() == root {
        return false;
    }
    let name = entry.file_name().to_string_lossy().to_lowercase();
    entry.file_type().is_dir()
        && matches!(
            name.as_str(),
            "cache" | ".git" | "node_modules" | "logs" | "log" | "temp" | "tmp" | "__pycache__"
        )
}

fn ignored_file(relative: &str) -> bool {
    let lower = relative.to_lowercase();
    lower.ends_with(".log") || lower.ends_with(".tmp") || lower.ends_with(".bak")
}

fn role_for_path(path: &Path) -> &'static str {
    match path
        .components()
        .next()
        .and_then(|part| part.as_os_str().to_str())
    {
        Some("客户端") => "client",
        Some("引擎") => "engine",
        _ => "project",
    }
}

fn classify(path: &Path) -> FileCategory {
    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    if extension == "lua" {
        FileCategory::Lua
    } else if contains_any(&normalized, &["/map/", "mapinfo", "地图"]) {
        FileCategory::Map
    } else if contains_any(&normalized, &["npc", "market_def", "merchant", "商人"]) {
        FileCategory::Npc
    } else if contains_any(&normalized, &["monster", "monitems", "怪物"]) {
        FileCategory::Monster
    } else if contains_any(&normalized, &["item", "cfg_item", "物品"]) {
        FileCategory::Item
    } else if contains_any(&normalized, &["quest", "questdiary", "任务"]) {
        FileCategory::Quest
    } else if matches!(
        extension.as_str(),
        "ini" | "json" | "yaml" | "yml" | "toml" | "xml"
    ) || contains_any(&normalized, &["config", "配置"])
    {
        FileCategory::Config
    } else {
        FileCategory::Other
    }
}

fn contains_any(value: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| value.contains(candidate))
}

fn category_name(category: &FileCategory) -> &'static str {
    match category {
        FileCategory::Map => "Map",
        FileCategory::Npc => "NPC",
        FileCategory::Monster => "Monster",
        FileCategory::Item => "Item",
        FileCategory::Quest => "Quest",
        FileCategory::Lua => "Lua",
        FileCategory::Config => "Config",
        FileCategory::Other => "Other",
    }
}

fn text_extension(extension: Option<&str>) -> bool {
    matches!(
        extension,
        Some(
            "txt"
                | "lua"
                | "ini"
                | "json"
                | "yaml"
                | "yml"
                | "toml"
                | "xml"
                | "csv"
                | "cfg"
                | "map"
        )
    )
}

fn decode_text(bytes: &[u8]) -> Option<String> {
    if bytes.contains(&0) {
        if bytes.starts_with(&[0xff, 0xfe]) {
            let units: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            return String::from_utf16(&units).ok();
        }
        return None;
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Some(text.trim_start_matches('\u{feff}').to_string());
    }
    let (text, _, had_errors) = GBK.decode(bytes);
    (!had_errors).then(|| text.into_owned())
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn excerpt(content: Option<&str>, query: &str) -> Option<String> {
    let content = content?;
    if query.is_empty() {
        return Some(content.chars().take(240).collect());
    }
    let query = query.to_lowercase();
    let matching_line = content
        .lines()
        .find(|line| line.to_lowercase().contains(&query))
        .unwrap_or(content);
    Some(matching_line.chars().take(240).collect())
}

pub fn canonical_project_path(project_root: &str, relative_path: &str) -> Result<PathBuf, String> {
    let root = fs::canonicalize(project_root).map_err(|e| format!("PROJECT_PATH_INVALID: {e}"))?;
    let candidate = root.join(relative_path);
    let parent = candidate.parent().unwrap_or(&root);
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|e| format!("PROJECT_PARENT_INVALID: {}: {e}", parent.display()))?;
    if !path_is_within(&root, &canonical_parent) {
        return Err("PROJECT_PATH_OUTSIDE: path escapes project root".to_string());
    }
    Ok(candidate)
}

pub fn project_path_string(root: &str, relative: &str) -> Result<String, String> {
    canonical_project_path(root, relative).map(|path| path_string(&path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DomainFileQuery;

    #[test]
    fn scan_indexes_domain_text_without_loading_binary_content() {
        let base = std::env::temp_dir().join(format!("mir3-scan-{}", std::process::id()));
        let project_root = base.join("木立传奇");
        fs::create_dir_all(project_root.join("客户端/dev/Lua")).unwrap();
        fs::create_dir_all(project_root.join("引擎/Mir200/Envir/Market_Def")).unwrap();
        fs::write(
            project_root.join("客户端/dev/Lua/任务.lua"),
            "local 标题 = '测试任务'\n",
        )
        .unwrap();
        fs::write(
            project_root.join("引擎/Mir200/Envir/Market_Def/商人.txt"),
            "欢迎来到木立传奇\n",
        )
        .unwrap();
        fs::write(project_root.join("客户端/map.pkg"), vec![7_u8; 1024]).unwrap();

        let store = DomainStore::new(base.join("data")).unwrap();
        let project = store.import_project(&project_root).unwrap();
        let summary = store.scan_project(&project.id, || false).unwrap();
        assert_eq!(summary.scanned_files, 3);

        let results = store
            .query_index(
                &project.id,
                &IndexQuery {
                    text: "测试任务".to_string(),
                    categories: vec!["Lua".to_string()],
                    role: Some("client".to_string()),
                    limit: Some(10),
                },
            )
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].excerpt.as_deref().unwrap().contains("测试任务"));

        let binary_hash: Option<String> = store
            .project_connection(&project.id)
            .unwrap()
            .query_row(
                "SELECT sha256 FROM files WHERE path='客户端/map.pkg'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(binary_hash.is_none());
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn unicode_excerpt_never_uses_lowercased_byte_offsets() {
        let result = excerpt(Some("İstanbul\n传奇项目"), "传奇").unwrap();
        assert_eq!(result, "传奇项目");
    }

    #[test]
    fn ten_thousand_file_index_has_bounded_queries_and_stable_pagination() {
        const FILE_COUNT: usize = 10_025;
        let base = std::env::temp_dir().join(format!(
            "mir3-large-index-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let project_root = base.join("大型项目");
        fs::create_dir_all(project_root.join("客户端/dev")).unwrap();
        let item_root = project_root.join("引擎/Mir200/Envir/Item");
        fs::create_dir_all(&item_root).unwrap();
        for group in 0..101 {
            let directory = item_root.join(format!("group-{group:03}"));
            fs::create_dir_all(&directory).unwrap();
            for index in 0..100 {
                let ordinal = group * 100 + index;
                if ordinal >= FILE_COUNT {
                    break;
                }
                fs::write(
                    directory.join(format!("item-{ordinal:05}.txt")),
                    format!("itemId={ordinal}\tprice={}\n", ordinal + 1),
                )
                .unwrap();
            }
        }

        let started = std::time::Instant::now();
        let store = DomainStore::new(base.join("data")).unwrap();
        let project = store.import_project(&project_root).unwrap();
        let summary = store.scan_project(&project.id, || false).unwrap();
        assert_eq!(summary.scanned_files, FILE_COUNT);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "10k-file fixture indexing exceeded the 60 second G4 gate"
        );

        let bounded = store
            .query_index(
                &project.id,
                &IndexQuery {
                    text: String::new(),
                    categories: Vec::new(),
                    role: None,
                    limit: Some(usize::MAX),
                },
            )
            .unwrap();
        assert_eq!(bounded.len(), 200);

        let first_page = store
            .query_domain_files(
                &project.id,
                "item",
                &DomainFileQuery {
                    text: String::new(),
                    limit: Some(125),
                    offset: Some(0),
                },
            )
            .unwrap();
        let last_page = store
            .query_domain_files(
                &project.id,
                "item",
                &DomainFileQuery {
                    text: String::new(),
                    limit: Some(125),
                    offset: Some(10_000),
                },
            )
            .unwrap();
        let clamped = store
            .query_domain_files(
                &project.id,
                "item",
                &DomainFileQuery {
                    text: String::new(),
                    limit: Some(usize::MAX),
                    offset: Some(0),
                },
            )
            .unwrap();
        assert_eq!(first_page.len(), 125);
        assert_eq!(last_page.len(), 25);
        assert_eq!(clamped.len(), 10_000);
        assert!(first_page.last().unwrap().path < last_page.first().unwrap().path);
        fs::remove_dir_all(base).ok();
    }
}
