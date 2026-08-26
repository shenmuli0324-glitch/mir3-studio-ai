//! 领域记录级资源投影。
//!
//! 真实文件是来源而不是资源本身。本模块把已验证的表格行和文本记录投影为
//! 可分页、可追溯、可跨文件稳定定位的领域资源。

use crate::{
    DomainDependencyEdge, DomainFileQuery, DomainFileRecord, DomainManifest, DomainResourceRecord,
    DomainStore,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainResourceQuery {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub resource_type: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainResourceSource {
    pub path: String,
    pub sheet: Option<String>,
    pub row: Option<usize>,
    #[serde(default)]
    pub headers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainResourceDependency {
    pub field: String,
    pub value: String,
    pub system_id: String,
    pub required: bool,
    pub resolved_resource_id: Option<String>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
struct RawResource {
    record: DomainResourceRecord,
    identity: String,
}

impl DomainStore {
    /// 返回记录级资源；分页在完成重复键和引用诊断后执行。
    pub fn query_domain_resources(
        &self,
        project_id: &str,
        system_id: &str,
        query: &DomainResourceQuery,
    ) -> Result<Vec<DomainResourceRecord>, String> {
        let manifest = self.runtime_manifest(system_id)?;
        let mut resources = self.collect_raw_resources(project_id, &manifest)?;
        diagnose_duplicate_identities(&mut resources);
        self.resolve_resource_dependencies(project_id, &manifest, &mut resources)?;

        let text = query.text.trim().to_lowercase();
        let offset = query.offset.unwrap_or_default();
        let limit = query.limit.unwrap_or(250).clamp(1, 10_000);
        Ok(resources
            .into_iter()
            .map(|resource| resource.record)
            .filter(|resource| {
                query
                    .resource_type
                    .as_ref()
                    .is_none_or(|kind| resource.resource_type == *kind)
                    && (text.is_empty()
                        || resource.id.to_lowercase().contains(&text)
                        || resource.label.to_lowercase().contains(&text)
                        || resource
                            .files
                            .iter()
                            .any(|file| file.path.to_lowercase().contains(&text))
                        || Value::Object(resource.fields.clone())
                            .to_string()
                            .to_lowercase()
                            .contains(&text))
            })
            .skip(offset)
            .take(limit)
            .collect())
    }

    /// 引用解析仅取目标领域原始记录，避免依赖边形成递归查询。
    fn resolve_resource_dependencies(
        &self,
        project_id: &str,
        manifest: &DomainManifest,
        resources: &mut [RawResource],
    ) -> Result<(), String> {
        let mut targets = BTreeMap::<String, Result<BTreeMap<String, String>, String>>::new();
        for edge in &manifest.resources.dependency_edges {
            if targets.contains_key(&edge.system_id) {
                continue;
            }
            let identities = self
                .runtime_manifest(&edge.system_id)
                .and_then(|target_manifest| {
                    let target_resources =
                        self.collect_raw_resources(project_id, &target_manifest)?;
                    let mut identities = BTreeMap::new();
                    for target in target_resources {
                        for field in &target_manifest.resources.unique_key {
                            if let Some(value) = field_value(&target.record.fields, field) {
                                identities
                                    .entry(value)
                                    .or_insert_with(|| target.record.id.clone());
                            }
                        }
                    }
                    Ok(identities)
                });
            targets.insert(edge.system_id.clone(), identities);
        }
        for resource in resources {
            for edge in &manifest.resources.dependency_edges {
                append_dependency(resource, edge, targets.get(&edge.system_id));
            }
        }
        Ok(())
    }

    fn collect_raw_resources(
        &self,
        project_id: &str,
        manifest: &DomainManifest,
    ) -> Result<Vec<RawResource>, String> {
        let files = self.query_domain_files(
            project_id,
            &manifest.system_id,
            &DomainFileQuery {
                text: String::new(),
                limit: Some(10_000),
                offset: None,
            },
        )?;
        let mut records = Vec::new();
        for file in files
            .into_iter()
            .filter(|file| file.ownership != "dependency")
        {
            match file.extension.as_deref().unwrap_or_default() {
                extension if extension.eq_ignore_ascii_case("xls") => {
                    if let Err(error) =
                        self.collect_xls_resources(project_id, manifest, file.clone(), &mut records)
                    {
                        let mut readonly_file = file.clone();
                        readonly_file.access = "readonly".to_string();
                        let mut fallback = build_record(
                            manifest,
                            readonly_file,
                            Map::new(),
                            DomainResourceSource {
                                path: file.path,
                                sheet: None,
                                row: None,
                                headers: Vec::new(),
                            },
                        );
                        fallback
                            .record
                            .diagnostics
                            .push(format!("DOMAIN_RESOURCE_XLS_READ_FAILED:{error}"));
                        records.push(fallback);
                    }
                }
                extension if extension.eq_ignore_ascii_case("map") => {
                    records.push(map_resource(manifest, file));
                }
                _ => {
                    if let Some(content) = self.indexed_resource_content(project_id, &file.path) {
                        collect_text_resources(manifest, file, &content, &mut records);
                    }
                }
            }
        }
        records.sort_by(|left, right| {
            left.record
                .files
                .first()
                .map(|file| file.path.as_str())
                .cmp(&right.record.files.first().map(|file| file.path.as_str()))
                .then_with(|| left.record.source.row.cmp(&right.record.source.row))
        });
        Ok(records)
    }

    fn collect_xls_resources(
        &self,
        project_id: &str,
        manifest: &DomainManifest,
        file: DomainFileRecord,
        records: &mut Vec<RawResource>,
    ) -> Result<(), String> {
        let workbook = self.safe_xls_open(project_id, &file.path)?;
        for sheet in workbook.sheets {
            let data =
                self.safe_xls_sheet_read(project_id, &file.path, &sheet.name, &workbook.sha256)?;
            let Some(headers) = data.rows.first().map(|row| normalized_headers(row)) else {
                continue;
            };
            for (index, row) in data.rows.iter().enumerate().skip(1) {
                if row.iter().all(|value| value.trim().is_empty()) {
                    continue;
                }
                let fields = apply_field_mappings(manifest, fields_from_row(&headers, row));
                records.push(build_record(
                    manifest,
                    file.clone(),
                    fields,
                    DomainResourceSource {
                        path: file.path.clone(),
                        sheet: Some(data.sheet.clone()),
                        row: Some(index + 1),
                        headers: headers.clone(),
                    },
                ));
            }
        }
        Ok(())
    }

    fn indexed_resource_content(&self, project_id: &str, path: &str) -> Option<String> {
        self.project_connection(project_id)
            .ok()?
            .query_row("SELECT content FROM files WHERE path=?1", [path], |row| {
                row.get(0)
            })
            .ok()
            .flatten()
    }
}

fn collect_text_resources(
    manifest: &DomainManifest,
    file: DomainFileRecord,
    content: &str,
    records: &mut Vec<RawResource>,
) {
    let lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with(';'))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return;
    }
    let delimiter = if lines[0].contains('\t') {
        Some('\t')
    } else if lines[0].contains(',') {
        Some(',')
    } else {
        None
    };
    if let Some(delimiter) = delimiter {
        let headers = normalized_headers(
            &lines[0]
                .split(delimiter)
                .map(str::to_string)
                .collect::<Vec<_>>(),
        );
        for (index, line) in lines.iter().enumerate().skip(1) {
            let values = line
                .split(delimiter)
                .map(str::to_string)
                .collect::<Vec<_>>();
            records.push(build_record(
                manifest,
                file.clone(),
                apply_field_mappings(manifest, fields_from_row(&headers, &values)),
                DomainResourceSource {
                    path: file.path.clone(),
                    sheet: None,
                    row: Some(index + 1),
                    headers: headers.clone(),
                },
            ));
        }
        return;
    }

    let blocks = content.split("\n\n").collect::<Vec<_>>();
    for (index, block) in blocks.iter().enumerate() {
        let fields = apply_field_mappings(manifest, parse_text_fields(block));
        if fields.is_empty() {
            continue;
        }
        records.push(build_record(
            manifest,
            file.clone(),
            fields,
            DomainResourceSource {
                path: file.path.clone(),
                sheet: None,
                row: Some(index + 1),
                headers: Vec::new(),
            },
        ));
    }
}

fn parse_text_fields(block: &str) -> Map<String, Value> {
    let mut fields = Map::new();
    for line in block.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let segments = line.split(['\t', ',', ';']);
        let mut matched = false;
        for segment in segments {
            if let Some((key, value)) = segment.split_once('=').or_else(|| segment.split_once(':'))
            {
                let key = key.trim();
                if !key.is_empty() {
                    fields.insert(key.to_string(), Value::String(value.trim().to_string()));
                    matched = true;
                }
            }
        }
        if !matched && !fields.contains_key("value") {
            fields.insert("value".to_string(), Value::String(line.to_string()));
        }
    }
    fields
}

fn normalized_headers(row: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    row.iter()
        .enumerate()
        .map(|(index, value)| {
            let base = if value.trim().is_empty() {
                format!("column{}", index + 1)
            } else {
                value.trim().to_string()
            };
            let mut name = base.clone();
            let mut suffix = 2;
            while !seen.insert(normalize_field(&name)) {
                name = format!("{base}_{suffix}");
                suffix += 1;
            }
            name
        })
        .collect()
}

fn fields_from_row(headers: &[String], row: &[String]) -> Map<String, Value> {
    headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            (
                header.clone(),
                Value::String(row.get(index).cloned().unwrap_or_default()),
            )
        })
        .collect()
}

fn apply_field_mappings(
    manifest: &DomainManifest,
    fields: Map<String, Value>,
) -> Map<String, Value> {
    let aliases = manifest
        .resources
        .field_mappings
        .iter()
        .flat_map(|mapping| {
            mapping
                .aliases
                .iter()
                .map(move |alias| (normalize_field(alias), mapping))
        })
        .collect::<BTreeMap<_, _>>();
    let mut mapped = Map::new();
    for (source_field, value) in fields {
        let Some(mapping) = aliases.get(&normalize_field(&source_field)) else {
            mapped.insert(source_field, value);
            continue;
        };
        let field = mapping.field.clone();
        let value = typed_mapped_value(&mapping.value_type, value);
        if mapped.contains_key(&field) {
            mapped.insert(source_field, value);
        } else {
            mapped.insert(field, value);
        }
    }
    mapped
}

fn typed_mapped_value(value_type: &str, value: Value) -> Value {
    let Some(text) = value.as_str() else {
        return value;
    };
    match value_type {
        "integer" => text
            .trim()
            .parse::<i64>()
            .ok()
            .map(Value::from)
            .unwrap_or(value),
        "number" => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(value),
        "boolean" => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Value::Bool(true),
            "false" | "0" => Value::Bool(false),
            _ => value,
        },
        _ => value,
    }
}

fn build_record(
    manifest: &DomainManifest,
    file: DomainFileRecord,
    fields: Map<String, Value>,
    source: DomainResourceSource,
) -> RawResource {
    let resource_type = manifest
        .resources
        .resource_types
        .first()
        .cloned()
        .unwrap_or_else(|| format!("{}.record", manifest.system_id));
    let mut diagnostics = Vec::new();
    let unique = manifest
        .resources
        .unique_key
        .iter()
        .filter_map(|key| field_value(&fields, key).map(|value| (key.clone(), value)))
        .collect::<Vec<_>>();
    let identity = if unique.len() == manifest.resources.unique_key.len() && !unique.is_empty() {
        unique
            .iter()
            .map(|(key, value)| format!("{}={}", normalize_field(key), value.trim()))
            .collect::<Vec<_>>()
            .join("|")
    } else {
        diagnostics.push("DOMAIN_RESOURCE_IDENTITY_FALLBACK: unique key unavailable; canonical record content used".to_string());
        canonical_fields(&fields)
    };
    let id = record_id(&manifest.system_id, &resource_type, &identity);
    let label = unique
        .first()
        .map(|(_, value)| value.clone())
        .or_else(|| fields.values().find_map(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| id.chars().take(12).collect());
    let writable = file.access != "readonly";
    RawResource {
        record: DomainResourceRecord {
            id,
            system_id: manifest.system_id.clone(),
            resource_type,
            label,
            files: vec![file],
            dependency_systems: manifest.dependencies.clone(),
            writable,
            projection: serde_json::json!({"kind":"record","fields":fields.clone(),"source":source.clone()}),
            diagnostics,
            fields,
            source,
            dependencies: Vec::new(),
            mappings_applied: manifest.resources.mappings.clone(),
        },
        identity,
    }
}

fn map_resource(manifest: &DomainManifest, file: DomainFileRecord) -> RawResource {
    let map_id = file
        .path
        .rsplit('/')
        .next()
        .unwrap_or(&file.path)
        .split('.')
        .next()
        .unwrap_or_default()
        .to_string();
    let mut fields = Map::new();
    fields.insert("mapId".to_string(), Value::String(map_id));
    build_record(
        manifest,
        file.clone(),
        fields,
        DomainResourceSource {
            path: file.path,
            sheet: None,
            row: None,
            headers: Vec::new(),
        },
    )
}

fn append_dependency(
    resource: &mut RawResource,
    edge: &DomainDependencyEdge,
    target: Option<&Result<BTreeMap<String, String>, String>>,
) {
    let Some(value) = field_value(&resource.record.fields, &edge.field) else {
        if edge.required {
            let diagnostic = format!("DOMAIN_RESOURCE_REFERENCE_FIELD_MISSING:{}", edge.field);
            resource.record.diagnostics.push(diagnostic.clone());
            resource.record.dependencies.push(DomainResourceDependency {
                field: edge.field.clone(),
                value: String::new(),
                system_id: edge.system_id.clone(),
                required: true,
                resolved_resource_id: None,
                diagnostics: vec![diagnostic],
            });
        }
        return;
    };
    if value.trim().is_empty() && !edge.required {
        return;
    }
    let resolved_resource_id = target
        .and_then(|result| result.as_ref().ok())
        .and_then(|values| values.get(value.trim()))
        .cloned();
    let mut diagnostics = target
        .and_then(|result| result.as_ref().err())
        .map(|error| {
            vec![format!(
                "DOMAIN_RESOURCE_DEPENDENCY_UNAVAILABLE:{}:{error}",
                edge.system_id
            )]
        })
        .unwrap_or_default();
    if resolved_resource_id.is_none() && edge.required {
        diagnostics.push(format!(
            "DOMAIN_RESOURCE_REFERENCE_MISSING:{}:{}:{}",
            edge.system_id,
            edge.field,
            value.trim()
        ));
    }
    resource.record.diagnostics.extend(diagnostics.clone());
    resource.record.dependencies.push(DomainResourceDependency {
        field: edge.field.clone(),
        value,
        system_id: edge.system_id.clone(),
        required: edge.required,
        resolved_resource_id,
        diagnostics,
    });
}

fn diagnose_duplicate_identities(resources: &mut [RawResource]) {
    let mut counts = BTreeMap::<String, usize>::new();
    for resource in resources.iter() {
        *counts.entry(resource.identity.clone()).or_default() += 1;
    }
    let mut occurrences = BTreeMap::<String, usize>::new();
    for resource in resources {
        if counts.get(&resource.identity).copied().unwrap_or_default() <= 1 {
            continue;
        }
        let occurrence = occurrences.entry(resource.identity.clone()).or_default();
        *occurrence += 1;
        resource.record.diagnostics.push(format!(
            "DOMAIN_RESOURCE_UNIQUE_KEY_DUPLICATE:{}:{}",
            resource.identity, occurrence
        ));
        resource.record.id = format!("{}:duplicate:{}", resource.record.id, occurrence);
    }
}

fn field_value(fields: &Map<String, Value>, expected: &str) -> Option<String> {
    let expected = normalize_field(expected);
    fields
        .iter()
        .find(|(field, _)| normalize_field(field) == expected)
        .and_then(|(_, value)| match value {
            Value::String(value) => (!value.trim().is_empty()).then(|| value.trim().to_string()),
            Value::Number(value) => Some(value.to_string()),
            Value::Bool(value) => Some(value.to_string()),
            _ => None,
        })
}

fn normalize_field(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn canonical_fields(fields: &Map<String, Value>) -> String {
    let sorted = fields
        .iter()
        .map(|(key, value)| {
            (
                normalize_field(key),
                value.as_str().unwrap_or_default().trim(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    serde_json::to_string(&sorted).unwrap_or_default()
}

fn record_id(system_id: &str, resource_type: &str, identity: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(system_id.as_bytes());
    hasher.update([0]);
    hasher.update(resource_type.as_bytes());
    hasher.update([0]);
    hasher.update(identity.as_bytes());
    format!("{system_id}:{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyexcel_xls::biff8::{Biff8Book, Biff8Cell, Biff8Sheet, Biff8Value};
    use std::fs;
    use std::path::Path;

    fn text_cell(value: &str) -> Biff8Cell {
        Biff8Cell::general(Biff8Value::Text(value.to_string()))
    }

    #[test]
    fn executable_field_mappings_canonicalize_aliases_and_scalar_types() {
        let registry = crate::bundled_domain_registry().unwrap();
        let manifest = registry
            .packs
            .iter()
            .find(|manifest| manifest.system_id == "level")
            .unwrap();
        let source = Map::from_iter([
            ("Level".to_string(), Value::String("7".to_string())),
            (
                "required_experience".to_string(),
                Value::String("1000".to_string()),
            ),
            ("stat-points".to_string(), Value::String("3".to_string())),
        ]);
        let mapped = apply_field_mappings(manifest, source);
        assert_eq!(mapped.get("level").and_then(Value::as_i64), Some(7));
        assert_eq!(
            mapped.get("requiredExperience").and_then(Value::as_i64),
            Some(1000)
        );
        assert_eq!(mapped.get("statPoints").and_then(Value::as_i64), Some(3));
        assert!(!mapped.contains_key("required_experience"));
    }

    fn write_shop_workbook(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut sheet = Biff8Sheet::new("商品");
        let rows = [
            ["offerId", "itemId", "currencyItemId", "price"],
            ["OFFER_A", "ITEM_OK", "ITEM_OK", "10"],
            ["OFFER_B", "ITEM_MISSING", "ITEM_OK", "20"],
            ["DUPLICATE", "ITEM_OK", "ITEM_OK", "30"],
            ["DUPLICATE", "ITEM_OK", "ITEM_OK", "40"],
        ];
        for (row_index, row) in rows.iter().enumerate() {
            for (column_index, value) in row.iter().enumerate() {
                sheet
                    .set(row_index as u32, column_index, text_cell(value))
                    .unwrap();
            }
        }
        let mut book = Biff8Book::default();
        book.sheets.push(sheet);
        fs::write(path, book.to_cfb_bytes().unwrap()).unwrap();
    }

    fn create_project(root: &Path, shop_path: &str) {
        fs::create_dir_all(root.join("客户端/dev/Item")).unwrap();
        fs::create_dir_all(root.join("引擎/Mir200")).unwrap();
        fs::write(
            root.join("客户端/dev/Item/cfg_item.txt"),
            "itemId\tlinkedBuffId\nITEM_OK\t\n",
        )
        .unwrap();
        write_shop_workbook(&root.join(shop_path));
        fs::write(root.join("引擎/Mir200/Envir/Shop/opaque.bin"), b"opaque").unwrap();
        fs::write(
            root.join("引擎/Mir200/Envir/Shop/unknown.xls"),
            b"not-an-xls",
        )
        .unwrap();
    }

    #[test]
    fn record_resources_are_stable_traceable_and_dependency_aware() {
        let base = std::env::temp_dir().join(format!(
            "mir3-record-resource-{}-{}",
            std::process::id(),
            crate::now_millis()
        ));
        let first_root = base.join("first");
        let second_root = base.join("second");
        let first_path = "引擎/Mir200/Envir/Shop/cfg_store.xls";
        let second_path = "引擎/Mir200/Envir/Shop/moved/cfg_store.xls";
        create_project(&first_root, first_path);
        create_project(&second_root, second_path);

        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let first = store.import_project(&first_root).unwrap();
        let second = store.import_project(&second_root).unwrap();
        store.scan_project(&first.id, || false).unwrap();
        store.scan_project(&second.id, || false).unwrap();
        let query = DomainResourceQuery {
            text: String::new(),
            resource_type: None,
            limit: Some(100),
            offset: None,
        };
        let first_records = store
            .query_domain_resources(&first.id, "shop", &query)
            .unwrap();
        let second_records = store
            .query_domain_resources(&second.id, "shop", &query)
            .unwrap();

        assert_eq!(first_records.len(), 5);
        assert!(first_records.iter().all(|record| record.files.len() == 1));
        let known_records = first_records
            .iter()
            .filter(|record| record.files[0].path == first_path)
            .collect::<Vec<_>>();
        assert_eq!(known_records.len(), 4);
        assert_ne!(known_records[0].id, known_records[0].files[0].resource_id);
        let first_offer = first_records
            .iter()
            .find(|record| record.label == "OFFER_A")
            .unwrap();
        let moved_offer = second_records
            .iter()
            .find(|record| record.label == "OFFER_A")
            .unwrap();
        assert_eq!(first_offer.id, moved_offer.id);
        assert_eq!(first_offer.source.sheet.as_deref(), Some("商品"));
        assert_eq!(first_offer.source.row, Some(2));
        assert_eq!(first_offer.mappings_applied.len(), 2);
        assert!(first_offer
            .dependencies
            .iter()
            .all(|dependency| dependency.resolved_resource_id.is_some()));

        let missing = first_records
            .iter()
            .find(|record| record.label == "OFFER_B")
            .unwrap();
        assert!(missing.diagnostics.iter().any(|diagnostic| diagnostic
            .contains("DOMAIN_RESOURCE_REFERENCE_MISSING:item:itemId:ITEM_MISSING")));
        let duplicates = first_records
            .iter()
            .filter(|record| record.label == "DUPLICATE")
            .collect::<Vec<_>>();
        assert_eq!(duplicates.len(), 2);
        assert_ne!(duplicates[0].id, duplicates[1].id);
        assert!(duplicates.iter().all(|record| record
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.starts_with("DOMAIN_RESOURCE_UNIQUE_KEY_DUPLICATE:"))));

        let fetched = store
            .get_domain_resource(&first.id, "shop", &first_offer.id)
            .unwrap();
        assert_eq!(
            fetched.fields.get("offerId").and_then(Value::as_str),
            Some("OFFER_A")
        );
        assert!(known_records.iter().all(|record| record.writable));
        assert!(first_records
            .iter()
            .all(|record| record.files[0].path != "引擎/Mir200/Envir/Shop/opaque.bin"));
        let unknown = first_records
            .iter()
            .find(|record| record.files[0].path.ends_with("unknown.xls"))
            .unwrap();
        assert!(!unknown.writable);
        assert!(unknown
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.starts_with("DOMAIN_RESOURCE_XLS_READ_FAILED:")));

        let page = store
            .query_domain_resources(
                &first.id,
                "shop",
                &DomainResourceQuery {
                    text: "offer".to_string(),
                    resource_type: Some("shop.shop-offer-record".to_string()),
                    limit: Some(1),
                    offset: Some(1),
                },
            )
            .unwrap();
        assert_eq!(page.len(), 1);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn missing_unique_key_uses_content_identity_with_explicit_diagnostic() {
        let manifest = crate::bundled_domain_registry()
            .unwrap()
            .packs
            .iter()
            .find(|manifest| manifest.system_id == "shop")
            .unwrap();
        let file = DomainFileRecord {
            path: "first/cfg_store.txt".to_string(),
            role: "engine".to_string(),
            category: "other".to_string(),
            extension: Some("txt".to_string()),
            size: 1,
            modified_at: 0,
            resource_id: "file-id".to_string(),
            ownership: "owned".to_string(),
            access: "structured".to_string(),
            systems: vec!["shop".to_string()],
        };
        let mut fields = Map::new();
        fields.insert("price".to_string(), Value::String("50".to_string()));
        let first = build_record(
            manifest,
            file.clone(),
            fields.clone(),
            DomainResourceSource {
                path: file.path.clone(),
                sheet: None,
                row: Some(1),
                headers: Vec::new(),
            },
        );
        let mut moved = file;
        moved.path = "second/cfg_store.txt".to_string();
        let second = build_record(
            manifest,
            moved.clone(),
            fields,
            DomainResourceSource {
                path: moved.path,
                sheet: None,
                row: Some(1),
                headers: Vec::new(),
            },
        );
        assert_eq!(first.record.id, second.record.id);
        assert!(first
            .record
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.starts_with("DOMAIN_RESOURCE_IDENTITY_FALLBACK:")));
    }
}
