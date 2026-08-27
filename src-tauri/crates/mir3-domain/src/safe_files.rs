use crate::{DomainStore, DraftBinaryChangeInput, DraftPreview};
use calamine::{Reader, Xls};
use easyexcel_xls::biff8::{Biff8Cell, Biff8TemplatePackage, Biff8Value};
use encoding_rs::{GB18030, GBK};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use similar::{DiffTag, TextDiff};
use std::collections::BTreeSet;
use std::fs;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

const OLE2_MAGIC: &[u8; 8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";
const MAX_XLS_FILE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_XLS_ROWS: usize = 20_000;
const MAX_XLS_COLUMNS: usize = 256;
const MAX_XLS_CELLS: usize = 500_000;
const MAX_XLS_CACHE_ENTRIES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextEncoding {
    Ascii,
    Utf8,
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
    Gbk,
    Gb18030,
}

#[derive(Debug, Clone)]
struct DetectedText {
    encoding: TextEncoding,
    bom: Vec<u8>,
    content: String,
    newline: Option<String>,
    mixed_newlines: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeTextOpen {
    pub project_id: String,
    pub relative_path: String,
    pub content: String,
    pub encoding: String,
    pub bom: String,
    pub newline: Option<String>,
    pub mixed_newlines: bool,
    pub sha256: String,
    pub draft_id: Option<String>,
    pub revision: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeTextPatch {
    pub relative_path: String,
    pub draft_id: Option<String>,
    #[serde(default)]
    pub expected_revision: i64,
    pub expected_sha256: String,
    pub original_content: String,
    pub new_content: String,
    pub newline: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeTextPatchResult {
    pub draft_id: String,
    pub revision: i64,
    pub sha256: String,
    pub preview: DraftPreview,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeXlsSheetMeta {
    pub name: String,
    pub row_count: usize,
    pub column_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeXlsWorkbook {
    pub relative_path: String,
    pub sha256: String,
    pub sheets: Vec<SafeXlsSheetMeta>,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeXlsSheet {
    pub sheet: String,
    pub row_count: usize,
    pub column_count: usize,
    pub rows: Vec<Vec<String>>,
    pub source_sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeXlsCellUpdate {
    pub sheet: String,
    pub row: u32,
    pub column: usize,
    #[serde(default)]
    pub expected_value: Option<String>,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeXlsDraftPatch {
    pub relative_path: String,
    pub draft_id: String,
    pub expected_revision: i64,
    pub expected_sha256: String,
    pub updates: Vec<SafeXlsCellUpdate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeXlsPatchResult {
    pub draft_id: String,
    pub revision: i64,
    pub sha256: String,
    pub preview: DraftPreview,
}

#[derive(Debug)]
pub(crate) struct CachedXlsWorkbook {
    file_len: u64,
    modified_nanos: u128,
    workbook: SafeXlsWorkbook,
    sheets: Vec<SafeXlsSheet>,
}

impl DomainStore {
    pub fn safe_text_open(
        &self,
        project_id: &str,
        relative_path: &str,
        draft_id: Option<&str>,
    ) -> Result<SafeTextOpen, String> {
        validate_safe_text_path(relative_path)?;
        let target = self.safe_file_target(project_id, relative_path)?;
        let source = fs::read(&target)
            .map_err(|e| format!("SAFE_FILE_READ_FAILED: {}: {e}", target.display()))?;
        let source_sha = hash_bytes(&source);
        let draft = draft_id
            .map(|id| self.get_draft(project_id, id))
            .transpose()?;
        let bytes = match draft_id {
            Some(id) => self
                .draft_change_bytes(project_id, id, relative_path)?
                .unwrap_or(source),
            None => source,
        };
        let detected = detect_text(&bytes)?;
        Ok(SafeTextOpen {
            project_id: project_id.to_string(),
            relative_path: relative_path.replace('\\', "/"),
            content: detected.content,
            encoding: encoding_name(detected.encoding).to_string(),
            bom: bom_name(&detected.bom).to_string(),
            newline: detected.newline,
            mixed_newlines: detected.mixed_newlines,
            sha256: source_sha,
            draft_id: draft.as_ref().map(|value| value.id.clone()),
            revision: draft.map_or(0, |value| value.revision),
        })
    }

    pub fn safe_text_patch(
        &self,
        project_id: &str,
        request: &SafeTextPatch,
    ) -> Result<SafeTextPatchResult, String> {
        validate_safe_text_path(&request.relative_path)?;
        let target = self.safe_file_target(project_id, &request.relative_path)?;
        let source = fs::read(&target)
            .map_err(|e| format!("SAFE_FILE_READ_FAILED: {}: {e}", target.display()))?;
        let source_sha = hash_bytes(&source);
        if source_sha != request.expected_sha256 {
            return Err(
                "SAFE_FILE_SOURCE_CONFLICT: source changed since it was opened".to_string(),
            );
        }
        let draft_id = request.draft_id.as_deref().ok_or_else(|| {
            "DOMAIN_DRAFT_SCOPE_REQUIRED: open and bind a domain Draft before editing".to_string()
        })?;
        let draft = self.get_draft(project_id, draft_id)?;
        if draft.revision != request.expected_revision {
            return Err(format!(
                "DRAFT_REVISION_CONFLICT: expected {}, current {}",
                request.expected_revision, draft.revision
            ));
        }
        let current = self
            .draft_change_bytes(project_id, &draft.id, &request.relative_path)?
            .unwrap_or(source);
        let output = splice_document(
            &current,
            &request.original_content,
            &request.new_content,
            request.newline.as_deref(),
        )?;
        let output_sha = hash_bytes(&output);
        let preview = self.patch_draft_bytes(
            project_id,
            &draft.id,
            draft.revision,
            &[DraftBinaryChangeInput {
                path: request.relative_path.clone(),
                content: output,
                expected_sha256: Some(source_sha),
            }],
        )?;
        Ok(SafeTextPatchResult {
            draft_id: draft.id,
            revision: preview.draft.revision,
            sha256: output_sha,
            preview,
        })
    }

    pub fn safe_xls_open(
        &self,
        project_id: &str,
        relative_path: &str,
    ) -> Result<SafeXlsWorkbook, String> {
        validate_xls_path(relative_path)?;
        let target = self.safe_file_target(project_id, relative_path)?;
        let metadata = xls_metadata(&target)?;
        let cache_key = xls_cache_key(project_id, relative_path);
        let bytes = fs::read(&target)
            .map_err(|e| format!("SAFE_XLS_READ_FAILED: {}: {e}", target.display()))?;
        ensure_ole2(&bytes)?;
        let sha256 = hash_bytes(&bytes);
        if let Some(cached) = self.cached_xls(&cache_key, &metadata, Some(&sha256))? {
            return Ok(cached.workbook.clone());
        }
        let mut source =
            Xls::new(Cursor::new(bytes)).map_err(|e| format!("SAFE_XLS_PARSE_FAILED: {e}"))?;
        let sheet_names = source.sheet_names().to_vec();
        let mut sheets = Vec::with_capacity(sheet_names.len());
        let mut sheet_meta = Vec::with_capacity(sheet_names.len());
        for name in sheet_names {
            let range = source
                .worksheet_range(&name)
                .map_err(|e| format!("SAFE_XLS_SHEET_FAILED: {e}"))?;
            let rows = crop_effective_rows(
                range
                    .rows()
                    .map(|row| row.iter().map(ToString::to_string).collect()),
            );
            let row_count = rows.len();
            let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
            validate_xls_dimensions(&name, row_count, column_count)?;
            let mut rows = rows;
            for row in &mut rows {
                row.resize(column_count, String::new());
            }
            sheet_meta.push(SafeXlsSheetMeta {
                name: name.clone(),
                row_count,
                column_count,
            });
            sheets.push(SafeXlsSheet {
                sheet: name,
                row_count,
                column_count,
                rows,
                source_sha256: sha256.clone(),
            });
        }
        let workbook = SafeXlsWorkbook {
            relative_path: relative_path.replace('\\', "/"),
            sha256,
            sheets: sheet_meta,
            read_only: false,
        };
        let cached = CachedXlsWorkbook {
            file_len: metadata.file_len,
            modified_nanos: metadata.modified_nanos,
            workbook: workbook.clone(),
            sheets,
        };
        let mut cache = self
            .xls_cache
            .lock()
            .map_err(|_| "SAFE_XLS_CACHE_FAILED: cache lock poisoned".to_string())?;
        if cache.len() >= MAX_XLS_CACHE_ENTRIES && !cache.contains_key(&cache_key) {
            cache.clear();
        }
        cache.insert(cache_key, Arc::new(cached));
        Ok(workbook)
    }

    pub fn safe_xls_sheet_read(
        &self,
        project_id: &str,
        relative_path: &str,
        sheet: &str,
        expected_sha256: &str,
    ) -> Result<SafeXlsSheet, String> {
        validate_xls_path(relative_path)?;
        let target = self.safe_file_target(project_id, relative_path)?;
        let metadata = xls_metadata(&target)?;
        let cache_key = xls_cache_key(project_id, relative_path);
        let cached = match self.cached_xls(&cache_key, &metadata, Some(expected_sha256))? {
            Some(cached) => cached,
            None => {
                let workbook = self.safe_xls_open(project_id, relative_path)?;
                if workbook.sha256 != expected_sha256 {
                    return Err(
                        "SAFE_FILE_SOURCE_CONFLICT: XLS changed since it was opened".to_string()
                    );
                }
                self.cached_xls(&cache_key, &xls_metadata(&target)?, Some(expected_sha256))?
                    .ok_or_else(|| "SAFE_XLS_CACHE_FAILED: workbook was not cached".to_string())?
            }
        };
        if cached.workbook.sha256 != expected_sha256 {
            return Err("SAFE_FILE_SOURCE_CONFLICT: XLS changed since it was opened".to_string());
        }
        cached
            .sheets
            .iter()
            .find(|value| value.sheet == sheet)
            .cloned()
            .ok_or_else(|| format!("SAFE_XLS_SHEET_NOT_FOUND: {sheet}"))
    }

    /// 只在外置 Draft 中覆盖 BIFF8 单元格，并保留未修改记录和原单元格样式。
    pub fn safe_xls_patch(
        &self,
        project_id: &str,
        request: &SafeXlsDraftPatch,
    ) -> Result<SafeXlsPatchResult, String> {
        validate_xls_path(&request.relative_path)?;
        if request.updates.is_empty() || request.updates.len() > 10_000 {
            return Err("SAFE_XLS_UPDATE_COUNT_INVALID: expected 1..10000 updates".to_string());
        }
        let target = self.safe_file_target(project_id, &request.relative_path)?;
        let source = fs::read(&target)
            .map_err(|error| format!("SAFE_XLS_READ_FAILED: {}: {error}", target.display()))?;
        ensure_ole2(&source)?;
        let source_sha = hash_bytes(&source);
        if source_sha != request.expected_sha256 {
            return Err("SAFE_FILE_SOURCE_CONFLICT: XLS changed since it was opened".to_string());
        }
        let draft = self.get_draft(project_id, &request.draft_id)?;
        if draft.revision != request.expected_revision {
            return Err(format!(
                "DRAFT_REVISION_CONFLICT: expected {}, current {}",
                request.expected_revision, draft.revision
            ));
        }
        let current = self
            .draft_change_bytes(project_id, &draft.id, &request.relative_path)?
            .unwrap_or_else(|| source.clone());
        let mut current_reader = Xls::new(Cursor::new(current.clone()))
            .map_err(|error| format!("SAFE_XLS_PARSE_FAILED: {error}"))?;
        let mut package = Biff8TemplatePackage::from_bytes(&current)
            .map_err(|error| format!("SAFE_XLS_TEMPLATE_UNSUPPORTED: {error}"))?;
        let sheet_names = package.sheet_names().into_iter().collect::<BTreeSet<_>>();
        let mut coordinates = BTreeSet::new();
        for update in &request.updates {
            if !sheet_names.contains(&update.sheet) {
                return Err(format!("SAFE_XLS_SHEET_NOT_FOUND: {}", update.sheet));
            }
            if update.row as usize >= MAX_XLS_ROWS || update.column >= MAX_XLS_COLUMNS {
                return Err(format!(
                    "SAFE_XLS_CELL_RANGE_INVALID: {}!R{}C{}",
                    update.sheet, update.row, update.column
                ));
            }
            if !coordinates.insert((update.sheet.clone(), update.row, update.column)) {
                return Err(format!(
                    "SAFE_XLS_CELL_DUPLICATE: {}!R{}C{}",
                    update.sheet, update.row, update.column
                ));
            }
            if let Some(expected) = &update.expected_value {
                let range = current_reader
                    .worksheet_range(&update.sheet)
                    .map_err(|error| format!("SAFE_XLS_SHEET_FAILED: {error}"))?;
                let current_value = range
                    .get_value((update.row, update.column as u32))
                    .map(ToString::to_string)
                    .unwrap_or_default();
                if &current_value != expected {
                    return Err(format!(
                        "SAFE_XLS_CELL_CONFLICT: {}!R{}C{} expected value does not match",
                        update.sheet, update.row, update.column
                    ));
                }
            }
            let value = safe_xls_value(&update.value)?;
            package
                .set_cell(
                    &update.sheet,
                    update.row,
                    update.column,
                    &Biff8Cell::general(value),
                )
                .map_err(|error| format!("SAFE_XLS_WRITE_FAILED: {error}"))?;
        }
        let output = package
            .to_bytes()
            .map_err(|error| format!("SAFE_XLS_WRITE_FAILED: {error}"))?;
        let output_sha = hash_bytes(&output);
        let preview = self.patch_draft_bytes(
            project_id,
            &draft.id,
            draft.revision,
            &[DraftBinaryChangeInput {
                path: request.relative_path.clone(),
                content: output,
                expected_sha256: Some(source_sha),
            }],
        )?;
        Ok(SafeXlsPatchResult {
            draft_id: draft.id,
            revision: preview.draft.revision,
            sha256: output_sha,
            preview,
        })
    }

    fn cached_xls(
        &self,
        cache_key: &str,
        metadata: &XlsMetadata,
        expected_sha256: Option<&str>,
    ) -> Result<Option<Arc<CachedXlsWorkbook>>, String> {
        let mut cache = self
            .xls_cache
            .lock()
            .map_err(|_| "SAFE_XLS_CACHE_FAILED: cache lock poisoned".to_string())?;
        let matches = cache.get(cache_key).is_some_and(|value| {
            value.file_len == metadata.file_len
                && value.modified_nanos == metadata.modified_nanos
                && expected_sha256
                    .map(|expected| value.workbook.sha256 == expected)
                    .unwrap_or(true)
        });
        if matches {
            Ok(cache.get(cache_key).map(Arc::clone))
        } else {
            cache.remove(cache_key);
            Ok(None)
        }
    }

    fn safe_file_target(&self, project_id: &str, relative_path: &str) -> Result<PathBuf, String> {
        validate_relative(relative_path)?;
        let project = self.get_project(project_id)?;
        let root =
            fs::canonicalize(&project.root).map_err(|e| format!("PROJECT_PATH_INVALID: {e}"))?;
        let candidate = root.join(relative_path);
        let canonical = fs::canonicalize(&candidate)
            .map_err(|e| format!("SAFE_FILE_PATH_INVALID: {}: {e}", candidate.display()))?;
        if !canonical.starts_with(&root) || !canonical.is_file() {
            return Err("SAFE_FILE_PATH_OUTSIDE: file is outside the active project".to_string());
        }
        Ok(canonical)
    }
}

fn safe_xls_value(value: &serde_json::Value) -> Result<Biff8Value, String> {
    match value {
        serde_json::Value::Null => Ok(Biff8Value::Blank),
        serde_json::Value::Bool(value) => Ok(Biff8Value::Bool(*value)),
        serde_json::Value::Number(value) => value
            .as_f64()
            .filter(|value| value.is_finite())
            .map(Biff8Value::Number)
            .ok_or_else(|| "SAFE_XLS_VALUE_INVALID: number must be finite".to_string()),
        serde_json::Value::String(value) => {
            if value.starts_with('=') {
                return Err(
                    "SAFE_XLS_FORMULA_DENIED: formulas are not accepted by generic operations"
                        .to_string(),
                );
            }
            Ok(Biff8Value::Text(value.clone()))
        }
        _ => Err("SAFE_XLS_VALUE_INVALID: expected string, number, boolean or null".to_string()),
    }
}

#[derive(Debug, Clone, Copy)]
struct XlsMetadata {
    file_len: u64,
    modified_nanos: u128,
}

fn xls_metadata(path: &Path) -> Result<XlsMetadata, String> {
    let metadata = fs::metadata(path)
        .map_err(|e| format!("SAFE_XLS_METADATA_FAILED: {}: {e}", path.display()))?;
    if metadata.len() > MAX_XLS_FILE_BYTES {
        return Err(format!(
            "SAFE_XLS_FILE_TOO_LARGE: maximum is {} MiB",
            MAX_XLS_FILE_BYTES / 1024 / 1024
        ));
    }
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |value| value.as_nanos());
    Ok(XlsMetadata {
        file_len: metadata.len(),
        modified_nanos,
    })
}

fn xls_cache_key(project_id: &str, relative_path: &str) -> String {
    format!("{project_id}:{}", relative_path.replace('\\', "/"))
}

fn crop_effective_rows(rows: impl Iterator<Item = Vec<String>>) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = rows.collect();
    while rows
        .last()
        .is_some_and(|row| row.iter().all(|cell| cell.is_empty()))
    {
        rows.pop();
    }
    let column_count = rows
        .iter()
        .filter_map(|row| {
            row.iter()
                .rposition(|cell| !cell.is_empty())
                .map(|value| value + 1)
        })
        .max()
        .unwrap_or(0);
    for row in &mut rows {
        row.truncate(column_count);
    }
    rows
}

fn validate_xls_dimensions(
    sheet: &str,
    row_count: usize,
    column_count: usize,
) -> Result<(), String> {
    if row_count > MAX_XLS_ROWS {
        return Err(format!(
            "SAFE_XLS_SHEET_TOO_LARGE: {sheet} has {row_count} rows; maximum is {MAX_XLS_ROWS}"
        ));
    }
    if column_count > MAX_XLS_COLUMNS {
        return Err(format!(
            "SAFE_XLS_SHEET_TOO_WIDE: {sheet} has {column_count} columns; maximum is {MAX_XLS_COLUMNS}"
        ));
    }
    if row_count.saturating_mul(column_count) > MAX_XLS_CELLS {
        return Err(format!(
            "SAFE_XLS_SHEET_TOO_LARGE: {sheet} exceeds {MAX_XLS_CELLS} cells"
        ));
    }
    Ok(())
}

fn validate_relative(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("SAFE_FILE_PATH_INVALID: path must be a project-relative file".to_string());
    }
    Ok(())
}

fn validate_safe_text_path(value: &str) -> Result<(), String> {
    validate_relative(value)?;
    match Path::new(value)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("txt" | "lua") => Ok(()),
        _ => Err("SAFE_FILE_TYPE_UNSUPPORTED: only TXT and Lua are editable".to_string()),
    }
}

fn validate_xls_path(value: &str) -> Result<(), String> {
    validate_relative(value)?;
    if Path::new(value)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("xls"))
    {
        Ok(())
    } else {
        Err("SAFE_XLS_TYPE_UNSUPPORTED: only BIFF .xls is supported".to_string())
    }
}

fn ensure_ole2(bytes: &[u8]) -> Result<(), String> {
    if bytes.starts_with(OLE2_MAGIC) {
        Ok(())
    } else if bytes.starts_with(b"PK\x03\x04") {
        Err("SAFE_XLS_XLSX_REJECTED: OOXML/XLSX is not a BIFF .xls file".to_string())
    } else {
        Err("SAFE_XLS_CONTAINER_INVALID: expected an OLE2/BIFF workbook".to_string())
    }
}

fn detect_text(bytes: &[u8]) -> Result<DetectedText, String> {
    let (bom, encoding) = if bytes.starts_with(b"\x00\x00\xFE\xFF") {
        (&bytes[..4], TextEncoding::Utf32Be)
    } else if bytes.starts_with(b"\xFF\xFE\x00\x00") {
        (&bytes[..4], TextEncoding::Utf32Le)
    } else if bytes.starts_with(b"\xEF\xBB\xBF") {
        (&bytes[..3], TextEncoding::Utf8)
    } else if bytes.starts_with(b"\xFE\xFF") {
        (&bytes[..2], TextEncoding::Utf16Be)
    } else if bytes.starts_with(b"\xFF\xFE") {
        (&bytes[..2], TextEncoding::Utf16Le)
    } else {
        (&bytes[..0], TextEncoding::Ascii)
    };
    let payload = &bytes[bom.len()..];
    let (encoding, content) = if !bom.is_empty() {
        (encoding, decode_as(payload, encoding)?)
    } else {
        if payload.contains(&0) {
            return Err(
                "SAFE_TEXT_NUL_WITHOUT_BOM: NUL bytes require an explicit UTF BOM".to_string(),
            );
        }
        if payload.is_ascii() {
            (
                TextEncoding::Ascii,
                String::from_utf8(payload.to_vec()).expect("ASCII is UTF-8"),
            )
        } else if let Ok(value) = std::str::from_utf8(payload) {
            (TextEncoding::Utf8, value.to_string())
        } else {
            let (gbk_value, gbk_errors) = GBK.decode_without_bom_handling(payload);
            if !gbk_errors {
                (TextEncoding::Gbk, gbk_value.into_owned())
            } else {
                let (value, had_errors) = GB18030.decode_without_bom_handling(payload);
                if had_errors {
                    return Err(
                        "SAFE_TEXT_ENCODING_UNSUPPORTED: expected UTF-8, GBK or GB18030"
                            .to_string(),
                    );
                }
                (TextEncoding::Gb18030, value.into_owned())
            }
        }
    };
    let (newline, mixed_newlines) = newline_style(&content);
    Ok(DetectedText {
        encoding,
        bom: bom.to_vec(),
        content,
        newline,
        mixed_newlines,
    })
}

pub(crate) fn decode_supported_text(bytes: &[u8]) -> Option<String> {
    detect_text(bytes).ok().map(|value| value.content)
}

/// Draft 校验需要保留具体解码错误，不能像索引摘录那样把未知编码静默当成无内容。
pub(crate) fn decode_supported_text_checked(bytes: &[u8]) -> Result<String, String> {
    detect_text(bytes).map(|value| value.content)
}

/// 将 BIFF8 Draft 安全解析为领域校验可消费的 `字段=值` 投影。
/// 这里只读取受限有效区域，不缓存、不写文件，也不接受伪装成 XLS 的其他容器。
pub(crate) fn project_xls_validation_content(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() as u64 > MAX_XLS_FILE_BYTES {
        return Err("SAFE_XLS_TOO_LARGE: XLS exceeds 20 MiB".to_string());
    }
    ensure_ole2(bytes)?;
    let mut source =
        Xls::new(Cursor::new(bytes.to_vec())).map_err(|e| format!("SAFE_XLS_PARSE_FAILED: {e}"))?;
    let sheet_names = source.sheet_names().to_vec();
    if sheet_names.is_empty() {
        return Err("SAFE_XLS_SHEET_MISSING: workbook contains no sheets".to_string());
    }
    let mut projection = String::new();
    for name in sheet_names {
        let range = source
            .worksheet_range(&name)
            .map_err(|e| format!("SAFE_XLS_SHEET_FAILED: {e}"))?;
        let rows = crop_effective_rows(
            range
                .rows()
                .map(|row| row.iter().map(ToString::to_string).collect()),
        );
        let row_count = rows.len();
        let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
        validate_xls_dimensions(&name, row_count, column_count)?;
        projection.push_str("sheet=");
        projection.push_str(&name);
        projection.push('\n');
        let Some(header_index) = xls_validation_header_index(&rows) else {
            continue;
        };
        let headers = &rows[header_index];
        for row in rows.iter().skip(header_index + 1) {
            if xls_validation_comment_row(row) {
                continue;
            }
            for column in 0..column_count {
                let header = headers.get(column).map(String::as_str).unwrap_or_default();
                let value = row.get(column).map(String::as_str).unwrap_or_default();
                if !header.is_empty() && !value.is_empty() {
                    projection.push_str(header);
                    projection.push('=');
                    projection.push_str(value);
                    projection.push('\n');
                }
            }
        }
    }
    Ok(projection)
}

/// 旧 996 表格可能先放编号和中文说明；机器字段行通常拥有最多 ASCII 字段标识符。
fn xls_validation_header_index(rows: &[Vec<String>]) -> Option<usize> {
    let mut best = None;
    for (index, row) in rows.iter().take(32).enumerate() {
        let score = row
            .iter()
            .filter(|value| {
                value
                    .chars()
                    .any(|character| character.is_ascii_alphabetic())
            })
            .count();
        if score > 0 && best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((index, score));
        }
    }
    best.map(|(index, _)| index)
}

fn xls_validation_comment_row(row: &[String]) -> bool {
    row.iter()
        .find(|value| !value.trim().is_empty())
        .is_some_and(|value| {
            let value = value.trim();
            value.starts_with("//") || value.starts_with('#') || value.starts_with(';')
        })
}

fn decode_as(payload: &[u8], encoding: TextEncoding) -> Result<String, String> {
    match encoding {
        TextEncoding::Ascii | TextEncoding::Utf8 => std::str::from_utf8(payload)
            .map(str::to_string)
            .map_err(|e| format!("SAFE_TEXT_UTF8_INVALID: {e}")),
        TextEncoding::Gbk => {
            let (value, had_errors) = GBK.decode_without_bom_handling(payload);
            (!had_errors)
                .then(|| value.into_owned())
                .ok_or_else(|| "SAFE_TEXT_GBK_INVALID: undecodable input".to_string())
        }
        TextEncoding::Gb18030 => {
            let (value, had_errors) = GB18030.decode_without_bom_handling(payload);
            (!had_errors)
                .then(|| value.into_owned())
                .ok_or_else(|| "SAFE_TEXT_GB18030_INVALID: undecodable input".to_string())
        }
        TextEncoding::Utf16Le | TextEncoding::Utf16Be => {
            if !payload.len().is_multiple_of(2) {
                return Err("SAFE_TEXT_UTF16_INVALID: odd byte length".to_string());
            }
            let units = payload.chunks_exact(2).map(|pair| {
                if encoding == TextEncoding::Utf16Le {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            });
            char::decode_utf16(units)
                .collect::<Result<String, _>>()
                .map_err(|e| format!("SAFE_TEXT_UTF16_INVALID: {e}"))
        }
        TextEncoding::Utf32Le | TextEncoding::Utf32Be => {
            if !payload.len().is_multiple_of(4) {
                return Err("SAFE_TEXT_UTF32_INVALID: invalid byte length".to_string());
            }
            payload
                .chunks_exact(4)
                .map(|quad| {
                    let value = if encoding == TextEncoding::Utf32Le {
                        u32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]])
                    } else {
                        u32::from_be_bytes([quad[0], quad[1], quad[2], quad[3]])
                    };
                    char::from_u32(value)
                        .ok_or_else(|| format!("SAFE_TEXT_UTF32_INVALID: {value:#x}"))
                })
                .collect()
        }
    }
}

fn encode_as(value: &str, encoding: TextEncoding) -> Result<Vec<u8>, String> {
    match encoding {
        TextEncoding::Ascii => {
            if value.is_ascii() {
                Ok(value.as_bytes().to_vec())
            } else {
                Err("SAFE_TEXT_NOT_REPRESENTABLE: replacement is not ASCII".to_string())
            }
        }
        TextEncoding::Utf8 => Ok(value.as_bytes().to_vec()),
        TextEncoding::Gbk => {
            let (bytes, _, had_errors) = GBK.encode(value);
            (!had_errors).then(|| bytes.into_owned()).ok_or_else(|| {
                "SAFE_TEXT_NOT_REPRESENTABLE: replacement is not GBK/CP936".to_string()
            })
        }
        TextEncoding::Gb18030 => {
            let (bytes, _, had_errors) = GB18030.encode(value);
            (!had_errors).then(|| bytes.into_owned()).ok_or_else(|| {
                "SAFE_TEXT_NOT_REPRESENTABLE: replacement is not GB18030".to_string()
            })
        }
        TextEncoding::Utf16Le => Ok(value.encode_utf16().flat_map(u16::to_le_bytes).collect()),
        TextEncoding::Utf16Be => Ok(value.encode_utf16().flat_map(u16::to_be_bytes).collect()),
        TextEncoding::Utf32Le => Ok(value
            .chars()
            .flat_map(|ch| u32::from(ch).to_le_bytes())
            .collect()),
        TextEncoding::Utf32Be => Ok(value
            .chars()
            .flat_map(|ch| u32::from(ch).to_be_bytes())
            .collect()),
    }
}

fn splice_document(
    bytes: &[u8],
    original: &str,
    updated: &str,
    explicit_newline: Option<&str>,
) -> Result<Vec<u8>, String> {
    let detected = detect_text(bytes)?;
    if detected.content != original {
        return Err("SAFE_TEXT_CONTENT_CONFLICT: editor content is stale".to_string());
    }
    let payload = &bytes[detected.bom.len()..];
    let old_char_offsets = encoded_char_offsets(original, detected.encoding)?;
    if old_char_offsets.last().copied() != Some(payload.len())
        || encode_as(original, detected.encoding)? != payload
    {
        return Err("SAFE_TEXT_BYTE_STABILITY_FAILED: encoded edit range is ambiguous".to_string());
    }

    let mut output = Vec::with_capacity(bytes.len().max(updated.len()));
    output.extend_from_slice(&detected.bom);
    for operation in TextDiff::from_chars(original, updated).ops() {
        let old_range = operation.old_range();
        let new_range = operation.new_range();
        if operation.tag() == DiffTag::Equal {
            output.extend_from_slice(
                &payload[old_char_offsets[old_range.start]..old_char_offsets[old_range.end]],
            );
            continue;
        }
        let new_start = char_byte_index(updated, new_range.start);
        let new_end = char_byte_index(updated, new_range.end);
        let inserted = normalize_newlines(
            &updated[new_start..new_end],
            detected.newline.as_deref(),
            detected.mixed_newlines,
            explicit_newline,
        )?;
        output.extend_from_slice(&encode_as(&inserted, detected.encoding)?);
    }
    let verified = detect_text(&output)?;
    if verified.encoding != detected.encoding || verified.bom != detected.bom {
        return Err("SAFE_TEXT_FORMAT_CHANGED: encoding or BOM changed".to_string());
    }
    Ok(output)
}

/// 将编辑器的完整 Working Source 转换为保持原编码的原始字节。
///
/// 实现会按字符 Diff 拆成多个区间；相等区间直接复制原字节，只对真正修改的
/// token 重新编码。调用方可把多个文件的结果一次交给 `patch_draft_bytes`，从而
/// 获得单 revision 的原子多文件 Draft。
pub fn patch_supported_text_bytes(
    bytes: &[u8],
    original: &str,
    updated: &str,
    explicit_newline: Option<&str>,
) -> Result<Vec<u8>, String> {
    splice_document(bytes, original, updated, explicit_newline)
}

fn encoded_char_offsets(value: &str, encoding: TextEncoding) -> Result<Vec<usize>, String> {
    let mut offsets = Vec::with_capacity(value.chars().count() + 1);
    let mut length = 0;
    offsets.push(length);
    for character in value.chars() {
        length += encode_as(&character.to_string(), encoding)?.len();
        offsets.push(length);
    }
    Ok(offsets)
}

fn char_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map_or(value.len(), |(index, _)| index)
}

fn normalize_newlines(
    value: &str,
    detected: Option<&str>,
    mixed: bool,
    explicit: Option<&str>,
) -> Result<String, String> {
    if !value.contains(['\r', '\n']) {
        return Ok(value.to_string());
    }
    let selected = explicit.or(detected);
    if mixed && explicit.is_none() {
        return Err(
            "SAFE_TEXT_MIXED_NEWLINES: choose a newline style before inserting lines".to_string(),
        );
    }
    let selected = selected.ok_or_else(|| {
        "SAFE_TEXT_NEWLINE_REQUIRED: source has no newline convention".to_string()
    })?;
    if !matches!(selected, "\r\n" | "\n" | "\r") {
        return Err("SAFE_TEXT_NEWLINE_INVALID: expected CRLF, LF or CR".to_string());
    }
    Ok(value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', selected))
}

fn newline_style(value: &str) -> (Option<String>, bool) {
    let crlf = value.matches("\r\n").count();
    let remainder = value.replace("\r\n", "");
    let lf = remainder.matches('\n').count();
    let cr = remainder.matches('\r').count();
    let styles: Vec<&str> = [("\r\n", crlf), ("\n", lf), ("\r", cr)]
        .into_iter()
        .filter_map(|(style, count)| (count > 0).then_some(style))
        .collect();
    (
        styles
            .first()
            .filter(|_| styles.len() == 1)
            .map(|value| (*value).to_string()),
        styles.len() > 1,
    )
}

fn encoding_name(encoding: TextEncoding) -> &'static str {
    match encoding {
        TextEncoding::Ascii => "ASCII",
        TextEncoding::Utf8 => "UTF-8",
        TextEncoding::Utf16Le => "UTF-16LE",
        TextEncoding::Utf16Be => "UTF-16BE",
        TextEncoding::Utf32Le => "UTF-32LE",
        TextEncoding::Utf32Be => "UTF-32BE",
        TextEncoding::Gbk => "GBK",
        TextEncoding::Gb18030 => "GB18030",
    }
}

fn bom_name(bom: &[u8]) -> &'static str {
    match bom {
        b"\xEF\xBB\xBF" => "UTF-8",
        b"\xFF\xFE" => "UTF-16LE",
        b"\xFE\xFF" => "UTF-16BE",
        b"\xFF\xFE\x00\x00" => "UTF-32LE",
        b"\x00\x00\xFE\xFF" => "UTF-32BE",
        _ => "none",
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyexcel_xls::biff8::{Biff8Book, Biff8Sheet};

    #[test]
    fn validation_header_prefers_machine_fields_after_legacy_annotations() {
        let rows = vec![
            vec!["///key".to_string(), "idx".to_string(), "1".to_string()],
            vec!["//".to_string(), "传出地图".to_string(), "坐标".to_string()],
            vec![
                "///idx".to_string(),
                "MapId".to_string(),
                "EventPosX".to_string(),
            ],
            vec!["//1".to_string(), "//2".to_string(), "416".to_string()],
        ];
        assert_eq!(xls_validation_header_index(&rows), Some(2));
        assert!(xls_validation_comment_row(&rows[3]));
    }

    #[test]
    fn validation_header_keeps_the_first_row_when_data_has_the_same_ascii_score() {
        let rows = vec![
            vec!["MapId".to_string(), "EventPosX".to_string()],
            vec!["town".to_string(), "portal".to_string()],
        ];
        assert_eq!(xls_validation_header_index(&rows), Some(0));
    }

    #[test]
    fn gbk_and_crlf_are_preserved_byte_for_byte_outside_edit() {
        let (payload, _, had_errors) = GBK.encode("名称=木立\r\n等级=1\r\n");
        assert!(!had_errors);
        let original = payload.into_owned();
        let output = splice_document(
            &original,
            "名称=木立\r\n等级=1\r\n",
            "名称=木立\r\n等级=2\r\n",
            None,
        )
        .unwrap();
        let detected = detect_text(&output).unwrap();
        assert_eq!(detected.encoding, TextEncoding::Gbk);
        assert_eq!(detected.newline.as_deref(), Some("\r\n"));
        assert_eq!(detected.content, "名称=木立\r\n等级=2\r\n");
    }

    #[test]
    fn gbk_edit_rejects_characters_that_legacy_engine_cannot_represent() {
        let error = encode_as("任务😀", TextEncoding::Gbk).unwrap_err();
        assert!(error.starts_with("SAFE_TEXT_NOT_REPRESENTABLE"));
    }

    #[test]
    fn utf8_bom_is_preserved() {
        let original = b"\xEF\xBB\xBFreturn 1\n";
        let output = splice_document(original, "return 1\n", "return 2\n", None).unwrap();
        assert!(output.starts_with(b"\xEF\xBB\xBF"));
    }

    #[test]
    fn utf16le_bom_and_surrogate_bytes_are_preserved() {
        let source = "标题=传奇😀\r数值=1\r";
        let mut original = b"\xFF\xFE".to_vec();
        original.extend(encode_as(source, TextEncoding::Utf16Le).unwrap());
        let output =
            splice_document(&original, source, "标题=传奇😀\r数值=9\r", Some("\r")).unwrap();
        let detected = detect_text(&output).unwrap();
        assert_eq!(detected.encoding, TextEncoding::Utf16Le);
        assert_eq!(detected.newline.as_deref(), Some("\r"));
        assert_eq!(detected.content, "标题=传奇😀\r数值=9\r");
        assert!(output.starts_with(b"\xFF\xFE"));
    }

    #[test]
    fn pure_cr_insert_keeps_the_source_newline_style() {
        let original = b"a=1\rb=2\r";
        let output = splice_document(original, "a=1\rb=2\r", "a=1\rnew=3\rb=2\r", None).unwrap();
        assert_eq!(output, b"a=1\rnew=3\rb=2\r");
        assert!(!output.contains(&b'\n'));
    }

    #[test]
    fn distant_edits_preserve_intermediate_raw_bytes() {
        let original = b"x = 1\n-- untouched comment\ny = 2\n";
        let output = splice_document(
            original,
            "x = 1\n-- untouched comment\ny = 2\n",
            "x = 8\n-- untouched comment\ny = 9\n",
            None,
        )
        .unwrap();
        let marker = b"\n-- untouched comment\n";
        let old_start = original
            .windows(marker.len())
            .position(|window| window == marker)
            .unwrap();
        let new_start = output
            .windows(marker.len())
            .position(|window| window == marker)
            .unwrap();
        assert_eq!(
            &original[old_start..old_start + marker.len()],
            &output[new_start..new_start + marker.len()]
        );
    }

    #[test]
    fn mixed_newline_insert_fails_closed() {
        let result = splice_document(b"a\r\nb\nc", "a\r\nb\nc", "a\r\nb\nnew\nc", None);
        assert!(result.unwrap_err().starts_with("SAFE_TEXT_MIXED_NEWLINES"));
    }

    #[test]
    fn xlsx_disguised_as_xls_is_rejected() {
        assert!(ensure_ole2(b"PK\x03\x04fake")
            .unwrap_err()
            .contains("XLSX_REJECTED"));
    }

    #[test]
    fn xls_effective_range_removes_only_empty_tail() {
        let rows = vec![
            vec!["编号".to_string(), "名称".to_string(), String::new()],
            vec!["1".to_string(), "木立".to_string(), String::new()],
            vec![String::new(), String::new(), String::new()],
        ];
        let cropped = crop_effective_rows(rows.into_iter());
        assert_eq!(cropped.len(), 2);
        assert_eq!(cropped[0], vec!["编号", "名称"]);
        assert_eq!(cropped[1], vec!["1", "木立"]);
    }

    #[test]
    fn xls_limits_fail_closed() {
        assert!(validate_xls_dimensions("大表", MAX_XLS_ROWS + 1, 1)
            .unwrap_err()
            .contains("TOO_LARGE"));
        assert!(validate_xls_dimensions("宽表", 1, MAX_XLS_COLUMNS + 1)
            .unwrap_err()
            .contains("TOO_WIDE"));
        assert!(validate_xls_dimensions("密集表", 2_000, 251)
            .unwrap_err()
            .contains("500000"));
    }

    #[test]
    fn ten_thousand_row_xls_opens_with_bounded_dimensions_and_cache_reuse() {
        const ROWS: usize = 10_000;
        const COLUMNS: usize = 8;
        let started = std::time::Instant::now();
        let base = std::env::temp_dir().join(format!(
            "mir3-large-xls-{}-{}",
            std::process::id(),
            crate::now_millis()
        ));
        let project_root = base.join("项目/大表");
        let relative = "引擎/Mir200/Envir/Shop/large-shop.xls";
        let target = project_root.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::create_dir_all(project_root.join("客户端/dev")).unwrap();
        let mut sheet = Biff8Sheet::new("商品");
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                sheet
                    .set(
                        row as u32,
                        column,
                        Biff8Cell::general(Biff8Value::Text(format!("{row}:{column}"))),
                    )
                    .unwrap();
            }
        }
        let mut book = Biff8Book::default();
        book.sheets.push(sheet);
        fs::write(&target, book.to_cfb_bytes().unwrap()).unwrap();

        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let project = store.import_project(&project_root).unwrap();
        let workbook = store.safe_xls_open(&project.id, relative).unwrap();
        assert_eq!(workbook.sheets[0].row_count, ROWS);
        assert_eq!(workbook.sheets[0].column_count, COLUMNS);
        let sheet = store
            .safe_xls_sheet_read(&project.id, relative, "商品", &workbook.sha256)
            .unwrap();
        assert_eq!(sheet.rows.len(), ROWS);
        assert_eq!(sheet.rows[ROWS - 1][COLUMNS - 1], "9999:7");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "10k-row XLS fixture exceeded the 60 second G4 gate"
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn safe_patch_creates_external_draft_before_applying_preserved_bytes() {
        let base = std::env::temp_dir().join(format!("mir3-safe-file-{}", std::process::id()));
        let project_root = base.join("项目/木立");
        fs::create_dir_all(project_root.join("客户端/dev")).unwrap();
        fs::create_dir_all(project_root.join("引擎")).unwrap();
        let target = project_root.join("客户端/dev/Quest/任务配置.txt");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let (encoded, _, had_errors) = GBK.encode("名称=木立\r\n等级=1\r\n");
        assert!(!had_errors);
        let original = encoded.into_owned();
        fs::write(&target, &original).unwrap();

        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let project = store.import_project(&project_root).unwrap();
        let opened = store
            .safe_text_open(&project.id, "客户端/dev/Quest/任务配置.txt", None)
            .unwrap();
        let draft = store.open_draft(&project.id, "安全编辑任务配置").unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "quest", "1.3.1", None)
            .unwrap();
        let result = store
            .safe_text_patch(
                &project.id,
                &SafeTextPatch {
                    relative_path: "客户端/dev/Quest/任务配置.txt".to_string(),
                    draft_id: Some(draft.id),
                    expected_revision: 0,
                    expected_sha256: opened.sha256,
                    original_content: opened.content,
                    new_content: "名称=木立\r\n等级=2\r\n".to_string(),
                    newline: None,
                },
            )
            .unwrap();
        assert_eq!(fs::read(&target).unwrap(), original);
        assert!(result.preview.changes[0].unified_diff.is_some());

        store
            .apply_draft(
                &project.id,
                &result.draft_id,
                result.preview.draft.revision,
                &result.preview.diff_hash,
            )
            .unwrap();
        let applied = fs::read(&target).unwrap();
        let detected = detect_text(&applied).unwrap();
        assert_eq!(detected.encoding, TextEncoding::Gbk);
        assert_eq!(detected.newline.as_deref(), Some("\r\n"));
        assert_eq!(detected.content, "名称=木立\r\n等级=2\r\n");
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn safe_patch_without_scoped_draft_fails_without_creating_one() {
        let base =
            std::env::temp_dir().join(format!("mir3-safe-file-unscoped-{}", std::process::id()));
        let project_root = base.join("项目/木立");
        fs::create_dir_all(project_root.join("客户端/dev/Quest")).unwrap();
        fs::create_dir_all(project_root.join("引擎")).unwrap();
        let relative_path = "客户端/dev/Quest/任务配置.txt";
        fs::write(project_root.join(relative_path), "等级=1\r\n").unwrap();

        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let project = store.import_project(&project_root).unwrap();
        let opened = store
            .safe_text_open(&project.id, relative_path, None)
            .unwrap();
        let error = store
            .safe_text_patch(
                &project.id,
                &SafeTextPatch {
                    relative_path: relative_path.to_string(),
                    draft_id: None,
                    expected_revision: 0,
                    expected_sha256: opened.sha256,
                    original_content: opened.content,
                    new_content: "等级=2\r\n".to_string(),
                    newline: None,
                },
            )
            .unwrap_err();

        assert!(error.starts_with("DOMAIN_DRAFT_SCOPE_REQUIRED:"));
        assert!(store.list_drafts(&project.id).unwrap().is_empty());
        assert_eq!(
            fs::read_to_string(project_root.join(relative_path)).unwrap(),
            "等级=1\r\n"
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn xls_cell_update_writes_only_the_scoped_draft() {
        let base = std::env::temp_dir().join(format!("mir3-safe-xls-{}", std::process::id()));
        let project_root = base.join("项目/木立");
        let target = project_root.join("引擎/Mir200/Envir/Shop/商品表.xls");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::create_dir_all(project_root.join("客户端/dev")).unwrap();
        let mut sheet = Biff8Sheet::new("商品");
        sheet
            .set(
                0,
                0,
                Biff8Cell::general(Biff8Value::Text("旧价格".to_string())),
            )
            .unwrap();
        let mut book = Biff8Book::default();
        book.sheets.push(sheet);
        let original = book.to_cfb_bytes().unwrap();
        fs::write(&target, &original).unwrap();

        let store = DomainStore::new_trusted_fixture(base.join("data")).unwrap();
        let project = store.import_project(&project_root).unwrap();
        let opened = store
            .safe_xls_open(&project.id, "引擎/Mir200/Envir/Shop/商品表.xls")
            .unwrap();
        let draft = store.open_draft(&project.id, "修改商品价格").unwrap();
        store
            .bind_draft_domain(&project.id, &draft.id, "shop", "1.3.1", None)
            .unwrap();
        let result = store
            .safe_xls_patch(
                &project.id,
                &SafeXlsDraftPatch {
                    relative_path: "引擎/Mir200/Envir/Shop/商品表.xls".to_string(),
                    draft_id: draft.id.clone(),
                    expected_revision: 0,
                    expected_sha256: opened.sha256,
                    updates: vec![SafeXlsCellUpdate {
                        sheet: "商品".to_string(),
                        row: 0,
                        column: 0,
                        expected_value: Some("旧价格".to_string()),
                        value: serde_json::json!("新价格"),
                    }],
                },
            )
            .unwrap();
        assert_eq!(result.revision, 1);
        assert_eq!(fs::read(&target).unwrap(), original);
        let draft_bytes = store
            .draft_change_bytes(&project.id, &draft.id, "引擎/Mir200/Envir/Shop/商品表.xls")
            .unwrap()
            .unwrap();
        let mut parsed = Xls::new(Cursor::new(draft_bytes)).unwrap();
        let range = parsed.worksheet_range("商品").unwrap();
        assert_eq!(range.get_value((0, 0)).unwrap().to_string(), "新价格");
        fs::remove_dir_all(base).ok();
    }
}
