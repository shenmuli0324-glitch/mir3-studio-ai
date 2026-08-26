//! 领域包候选的可执行 fixture 与操作 dry-run 门禁。
//!
//! 该模块只解释公共 Schema、唯一键、范围、引用和运行时断言契约，不包含任何
//! 领域 ID 分支，因此 33 个包和后续兼容包共享同一条激活前验证路径。

use crate::{validate_domain_pack_manifest, DomainManifest, DomainValidatorContract};
use regex::Regex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_CONTRACT_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DomainFixtureCanaryReport {
    pub system_id: String,
    pub version: String,
    pub valid_records: usize,
    pub invalid_records: usize,
    pub expected_diagnostics: Vec<String>,
    pub operations_dry_run: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureCorpus {
    system_id: String,
    fixture: String,
    records: Vec<Map<String, Value>>,
    #[serde(default)]
    reference_catalog: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    runtime_assertions: Vec<RuntimeAssertion>,
}

#[derive(Debug, Deserialize)]
struct RuntimeAssertion {
    rule: String,
    expected: bool,
}

#[derive(Debug, Deserialize)]
struct ExpectedDiagnostic {
    code: String,
    severity: String,
}

/// 在候选包目录内执行 valid/invalid fixture 和公共操作 dry-run。
pub fn execute_domain_pack_fixture_canary(
    pack_root: &Path,
    expected_system_id: &str,
    expected_version: &str,
) -> Result<DomainFixtureCanaryReport, String> {
    let manifest = validate_domain_pack_manifest(
        &pack_root.join("domain.json"),
        expected_system_id,
        expected_version,
    )?;
    let schema: Value = read_contract_json(pack_root, &manifest.resources.schema)?;
    let valid: FixtureCorpus = read_contract_json(pack_root, &manifest.fixtures.valid)?;
    let invalid: FixtureCorpus = read_contract_json(pack_root, &manifest.fixtures.invalid)?;
    let expected: Vec<ExpectedDiagnostic> =
        read_contract_json(pack_root, &manifest.fixtures.expected_diagnostics)?;

    validate_fixture_identity(&manifest, &valid, "valid")?;
    validate_fixture_identity(&manifest, &invalid, "invalid")?;
    validate_schema_contract(&manifest, &schema)?;
    if valid.records.len() < 2 || invalid.records.len() < 2 {
        return Err(format!(
            "DOMAIN_PACK_FIXTURE_CORPUS_TOO_SMALL: {} requires at least two valid and invalid records",
            manifest.system_id
        ));
    }

    let valid_diagnostics = execute_fixture(&manifest, &schema, &valid)?;
    if !valid_diagnostics.is_empty() {
        return Err(format!(
            "DOMAIN_PACK_FIXTURE_VALID_REJECTED: {}: {}",
            manifest.system_id,
            valid_diagnostics.into_iter().collect::<Vec<_>>().join(",")
        ));
    }
    let invalid_diagnostics = execute_fixture(&manifest, &schema, &invalid)?;
    let expected_diagnostics = expected
        .iter()
        .map(|diagnostic| {
            if diagnostic.severity != "error" {
                return Err(format!(
                    "DOMAIN_PACK_FIXTURE_DIAGNOSTIC_SEVERITY_INVALID: {}: {}",
                    manifest.system_id, diagnostic.code
                ));
            }
            Ok(diagnostic.code.clone())
        })
        .collect::<Result<BTreeSet<_>, String>>()?;
    if expected_diagnostics.len() != expected.len() || invalid_diagnostics != expected_diagnostics {
        return Err(format!(
            "DOMAIN_PACK_FIXTURE_DIAGNOSTICS_MISMATCH: {}: expected [{}], got [{}]",
            manifest.system_id,
            expected_diagnostics
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(","),
            invalid_diagnostics
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(",")
        ));
    }

    let operations_dry_run = dry_run_operations(pack_root, &manifest)?;
    Ok(DomainFixtureCanaryReport {
        system_id: manifest.system_id,
        version: manifest.version,
        valid_records: valid.records.len(),
        invalid_records: invalid.records.len(),
        expected_diagnostics: expected_diagnostics.into_iter().collect(),
        operations_dry_run,
    })
}

fn validate_fixture_identity(
    manifest: &DomainManifest,
    fixture: &FixtureCorpus,
    expected_kind: &str,
) -> Result<(), String> {
    if fixture.system_id != manifest.system_id || fixture.fixture != expected_kind {
        return Err(format!(
            "DOMAIN_PACK_FIXTURE_IDENTITY_MISMATCH: {}:{}",
            manifest.system_id, expected_kind
        ));
    }
    Ok(())
}

fn validate_schema_contract(manifest: &DomainManifest, schema: &Value) -> Result<(), String> {
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            format!(
                "DOMAIN_PACK_FIXTURE_SCHEMA_INVALID: {}: properties are required",
                manifest.system_id
            )
        })?;
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            format!(
                "DOMAIN_PACK_FIXTURE_SCHEMA_INVALID: {}: required is missing",
                manifest.system_id
            )
        })?;
    if schema.get("type").and_then(Value::as_str) != Some("object")
        || schema.get("additionalProperties").and_then(Value::as_bool) != Some(false)
        || required.len() != properties.len()
    {
        return Err(format!(
            "DOMAIN_PACK_FIXTURE_SCHEMA_INVALID: {}: a closed fully-required object is required",
            manifest.system_id
        ));
    }
    let unique_fields = validator(manifest, "uniqueness")?
        .fields
        .as_array()
        .ok_or_else(|| {
            format!(
                "DOMAIN_PACK_FIXTURE_UNIQUE_CONTRACT_INVALID: {}",
                manifest.system_id
            )
        })?;
    let schema_unique = schema
        .pointer("/x-mir3/uniqueKey")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            format!(
                "DOMAIN_PACK_FIXTURE_UNIQUE_CONTRACT_INVALID: {}",
                manifest.system_id
            )
        })?;
    if unique_fields != schema_unique {
        return Err(format!(
            "DOMAIN_PACK_FIXTURE_UNIQUE_CONTRACT_INVALID: {}",
            manifest.system_id
        ));
    }
    let runtime = validator(manifest, "runtime-diagnostics")?;
    if schema
        .pointer("/x-mir3/runtimeRule")
        .and_then(Value::as_str)
        != Some(runtime.rule.as_str())
    {
        return Err(format!(
            "DOMAIN_PACK_FIXTURE_RUNTIME_CONTRACT_INVALID: {}",
            manifest.system_id
        ));
    }
    Ok(())
}

fn execute_fixture(
    manifest: &DomainManifest,
    schema: &Value,
    fixture: &FixtureCorpus,
) -> Result<BTreeSet<String>, String> {
    let mut diagnostics = BTreeSet::new();
    let properties = schema["properties"]
        .as_object()
        .ok_or_else(|| format!("DOMAIN_PACK_FIXTURE_SCHEMA_INVALID: {}", manifest.system_id))?;
    let required = schema["required"]
        .as_array()
        .ok_or_else(|| format!("DOMAIN_PACK_FIXTURE_SCHEMA_INVALID: {}", manifest.system_id))?;
    for record in &fixture.records {
        if record.keys().any(|field| !properties.contains_key(field))
            || required.iter().any(|field| {
                field
                    .as_str()
                    .is_none_or(|field| !record.contains_key(field))
            })
        {
            diagnostics.insert(format!("{}.schema", manifest.system_id));
        }
        for (field, rule) in properties {
            let Some(value) = record.get(field) else {
                continue;
            };
            if !value_matches_schema(value, rule)? {
                diagnostics.insert(format!("{}.schema", manifest.system_id));
            }
        }
    }

    execute_uniqueness(manifest, fixture, &mut diagnostics)?;
    execute_ranges(manifest, fixture, &mut diagnostics)?;
    execute_references(manifest, fixture, &mut diagnostics)?;
    execute_runtime_assertions(manifest, fixture, &mut diagnostics)?;
    Ok(diagnostics)
}

fn execute_uniqueness(
    manifest: &DomainManifest,
    fixture: &FixtureCorpus,
    diagnostics: &mut BTreeSet<String>,
) -> Result<(), String> {
    let fields = validator(manifest, "uniqueness")?
        .fields
        .as_array()
        .ok_or_else(|| {
            format!(
                "DOMAIN_PACK_FIXTURE_UNIQUE_CONTRACT_INVALID: {}",
                manifest.system_id
            )
        })?;
    let mut keys = BTreeSet::new();
    for record in &fixture.records {
        let key = fields
            .iter()
            .map(|field| {
                field
                    .as_str()
                    .and_then(|field| record.get(field))
                    .cloned()
                    .unwrap_or(Value::Null)
            })
            .collect::<Vec<_>>();
        let encoded = serde_json::to_string(&key)
            .map_err(|error| format!("DOMAIN_PACK_FIXTURE_UNIQUE_ENCODE_FAILED: {error}"))?;
        if !keys.insert(encoded) {
            diagnostics.insert(format!("{}.unique", manifest.system_id));
        }
    }
    Ok(())
}

fn execute_ranges(
    manifest: &DomainManifest,
    fixture: &FixtureCorpus,
    diagnostics: &mut BTreeSet<String>,
) -> Result<(), String> {
    let fields = validator(manifest, "range")?
        .fields
        .as_array()
        .ok_or_else(|| {
            format!(
                "DOMAIN_PACK_FIXTURE_RANGE_CONTRACT_INVALID: {}",
                manifest.system_id
            )
        })?;
    for rule in fields {
        let field = rule.get("field").and_then(Value::as_str).ok_or_else(|| {
            format!(
                "DOMAIN_PACK_FIXTURE_RANGE_CONTRACT_INVALID: {}",
                manifest.system_id
            )
        })?;
        let minimum = rule.get("minimum").and_then(Value::as_f64);
        let maximum = rule.get("maximum").and_then(Value::as_f64);
        if minimum.is_none() && maximum.is_none() {
            return Err(format!(
                "DOMAIN_PACK_FIXTURE_RANGE_CONTRACT_INVALID: {}:{field}",
                manifest.system_id
            ));
        }
        for record in &fixture.records {
            let Some(value) = record.get(field).and_then(Value::as_f64) else {
                continue;
            };
            if minimum.is_some_and(|minimum| value < minimum)
                || maximum.is_some_and(|maximum| value > maximum)
            {
                diagnostics.insert(format!("{}.range", manifest.system_id));
            }
        }
    }
    Ok(())
}

fn execute_references(
    manifest: &DomainManifest,
    fixture: &FixtureCorpus,
    diagnostics: &mut BTreeSet<String>,
) -> Result<(), String> {
    let references = &validator(manifest, "reference-integrity")?.references;
    if references.is_empty() {
        return Err(format!(
            "DOMAIN_PACK_FIXTURE_REFERENCE_CONTRACT_INVALID: {}",
            manifest.system_id
        ));
    }
    for reference in references {
        for record in &fixture.records {
            let Some(value) = record.get(&reference.field) else {
                continue;
            };
            let resolved = fixture
                .reference_catalog
                .get(&reference.system_id)
                .is_some_and(|catalog| catalog.contains(value));
            if !resolved {
                diagnostics.insert(format!("{}.reference", manifest.system_id));
            }
        }
    }
    Ok(())
}

fn execute_runtime_assertions(
    manifest: &DomainManifest,
    fixture: &FixtureCorpus,
    diagnostics: &mut BTreeSet<String>,
) -> Result<(), String> {
    let runtime = validator(manifest, "runtime-diagnostics")?;
    for assertion in &fixture.runtime_assertions {
        if assertion.rule != runtime.rule {
            return Err(format!(
                "DOMAIN_PACK_FIXTURE_RUNTIME_RULE_MISMATCH: {}: expected {}, got {}",
                manifest.system_id, runtime.rule, assertion.rule
            ));
        }
        if !assertion.expected {
            diagnostics.insert(format!("{}.runtime", manifest.system_id));
        }
    }
    if fixture.fixture == "invalid"
        && !fixture
            .runtime_assertions
            .iter()
            .any(|assertion| assertion.rule == runtime.rule && !assertion.expected)
    {
        return Err(format!(
            "DOMAIN_PACK_FIXTURE_RUNTIME_FAILURE_MISSING: {}:{}",
            manifest.system_id, runtime.rule
        ));
    }
    Ok(())
}

fn dry_run_operations(pack_root: &Path, manifest: &DomainManifest) -> Result<usize, String> {
    for operation in &manifest.operations {
        for step in &operation.steps {
            if !step.schema.is_empty() {
                let _: Value = read_contract_json(pack_root, &step.schema)?;
            }
            if step.operation == operation.id {
                let trace = serde_json::json!({
                    "operation": operation.id,
                    "primitive": step.primitive,
                    "action": step.action,
                    "writeSystems": operation.write_systems,
                });
                serde_json::to_vec(&trace)
                    .map_err(|error| format!("DOMAIN_PACK_OPERATION_DRY_RUN_FAILED: {error}"))?;
            }
        }
    }
    Ok(manifest.operations.len())
}

fn validator<'a>(
    manifest: &'a DomainManifest,
    kind: &str,
) -> Result<&'a DomainValidatorContract, String> {
    manifest
        .validators
        .iter()
        .find(|validator| validator.kind == kind)
        .ok_or_else(|| {
            format!(
                "DOMAIN_PACK_FIXTURE_VALIDATOR_MISSING: {}:{kind}",
                manifest.system_id
            )
        })
}

fn value_matches_schema(value: &Value, rule: &Value) -> Result<bool, String> {
    let value_type = rule.get("type").and_then(Value::as_str).unwrap_or_default();
    let valid_type = match value_type {
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.as_f64().is_some(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        _ => false,
    };
    if !valid_type {
        return Ok(false);
    }
    if let Some(text) = value.as_str() {
        if rule
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|minimum| text.chars().count() < minimum as usize)
            || rule
                .get("maxLength")
                .and_then(Value::as_u64)
                .is_some_and(|maximum| text.chars().count() > maximum as usize)
        {
            return Ok(false);
        }
        if let Some(pattern) = rule.get("pattern").and_then(Value::as_str) {
            let regex = Regex::new(pattern).map_err(|error| {
                format!("DOMAIN_PACK_FIXTURE_SCHEMA_PATTERN_INVALID: {pattern}: {error}")
            })?;
            if !regex.is_match(text) {
                return Ok(false);
            }
        }
    }
    if rule
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|allowed| !allowed.contains(value))
    {
        return Ok(false);
    }
    Ok(true)
}

fn read_contract_json<T: DeserializeOwned>(pack_root: &Path, relative: &str) -> Result<T, String> {
    let path = resolve_contract_path(pack_root, relative)?;
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("DOMAIN_PACK_FIXTURE_METADATA_FAILED: {error}"))?;
    if metadata.len() > MAX_CONTRACT_FILE_BYTES {
        return Err(format!(
            "DOMAIN_PACK_FIXTURE_TOO_LARGE: {} exceeds {} bytes",
            path.display(),
            MAX_CONTRACT_FILE_BYTES
        ));
    }
    let content = fs::read_to_string(&path)
        .map_err(|error| format!("DOMAIN_PACK_FIXTURE_READ_FAILED: {error}"))?;
    serde_json::from_str(&content).map_err(|error| {
        format!(
            "DOMAIN_PACK_FIXTURE_JSON_INVALID: {}: {error}",
            path.display()
        )
    })
}

fn resolve_contract_path(pack_root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative.is_empty()
        || relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("DOMAIN_PACK_FIXTURE_PATH_INVALID: {relative}"));
    }
    let root = fs::canonicalize(pack_root)
        .map_err(|error| format!("DOMAIN_PACK_FIXTURE_ROOT_INVALID: {error}"))?;
    let target = root.join(relative_path);
    let link_metadata = fs::symlink_metadata(&target)
        .map_err(|error| format!("DOMAIN_PACK_FIXTURE_METADATA_FAILED: {error}"))?;
    if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
        return Err(format!(
            "DOMAIN_PACK_FIXTURE_FILE_INVALID: {}",
            target.display()
        ));
    }
    let canonical = fs::canonicalize(&target)
        .map_err(|error| format!("DOMAIN_PACK_FIXTURE_PATH_INVALID: {error}"))?;
    if !canonical.starts_with(&root) {
        return Err(format!(
            "DOMAIN_PACK_FIXTURE_PATH_ESCAPE: {}",
            target.display()
        ));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bundled_domain_pack_fixtures_execute_with_exact_diagnostics() {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/mir3-domain-packs");
        let registry = crate::bundled_domain_registry().unwrap();
        let mut operations = 0_usize;
        for manifest in &registry.packs {
            let report = execute_domain_pack_fixture_canary(
                &root.join(&manifest.system_id),
                &manifest.system_id,
                &manifest.version,
            )
            .unwrap();
            assert_eq!(report.expected_diagnostics.len(), 4);
            assert!(report.valid_records >= 2);
            assert!(report.invalid_records >= 2);
            operations += report.operations_dry_run;
        }
        assert_eq!(operations, 194);
    }

    #[test]
    fn public_sdk_corpus_matches_runtime_manifest_and_fixture_contract() {
        let sdk_root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/mir3-domain-sdk");
        let corpus: Value = serde_json::from_str(
            &fs::read_to_string(sdk_root.join("fixtures/contract-corpus.json")).unwrap(),
        )
        .unwrap();
        for range in corpus["acceptedEngineRanges"].as_array().unwrap() {
            assert!(
                semver::VersionReq::parse(range.as_str().unwrap()).is_ok(),
                "Rust rejected SDK accepted engine range: {range}"
            );
        }
        for range in corpus["rejectedEngineRanges"].as_array().unwrap() {
            assert!(
                semver::VersionReq::parse(range.as_str().unwrap()).is_err(),
                "Rust accepted SDK rejected engine range: {range}"
            );
        }
        for accepted in corpus["accepted"].as_array().unwrap() {
            let pack_root = sdk_root
                .join("fixtures")
                .join(accepted["packRoot"].as_str().unwrap());
            let system_id = accepted["systemId"].as_str().unwrap();
            let version = accepted["version"].as_str().unwrap();
            let report =
                execute_domain_pack_fixture_canary(&pack_root, system_id, version).unwrap();
            assert_eq!(report.system_id, system_id);
            assert_eq!(report.version, version);
            assert!(report.valid_records >= 2);
            assert!(report.invalid_records >= 2);

            let source: Value =
                serde_json::from_str(&fs::read_to_string(pack_root.join("domain.json")).unwrap())
                    .unwrap();
            for rejected in corpus["rejected"].as_array().unwrap() {
                let mut mutated = source.clone();
                let pointer = rejected["pointer"].as_str().unwrap();
                *mutated.pointer_mut(pointer).unwrap() = rejected["value"].clone();
                let temporary = std::env::temp_dir().join(format!(
                    "mir3-domain-sdk-rejected-{}-{}",
                    std::process::id(),
                    crate::now_millis()
                ));
                fs::create_dir_all(&temporary).unwrap();
                let manifest_path = temporary.join("domain.json");
                fs::write(&manifest_path, serde_json::to_vec_pretty(&mutated).unwrap()).unwrap();
                assert!(
                    validate_domain_pack_manifest(&manifest_path, system_id, version).is_err(),
                    "SDK rejected corpus was accepted by Rust: {}",
                    rejected["name"].as_str().unwrap()
                );
                fs::remove_dir_all(temporary).ok();
            }
        }
    }
}
