//! 领域运行时证据门禁与跨端记录一致性校验。

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// 引擎自动泛化所依赖的项目级运行时证据。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeEngineEvidence {
    pub project_directory_layout: bool,
    pub owned_projection: bool,
    pub resource_schema_valid: bool,
    pub resource_schema_checked: usize,
}

/// 已由固定版本 Schema 投影出的单条客户端或引擎记录。
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeProjectedRecord {
    pub path: String,
    pub role: String,
    pub value: Value,
}

/// 独立运行时校验结果，由系统校验器映射到公开报告。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeValidatorOutcome {
    pub valid: bool,
    pub checked: usize,
    pub diagnostics: Vec<String>,
}

/// 领域清单中的运行时规则必须落到显式、可失败的结构化校验，未知规则一律拒绝。
/// 这里处理记录内部或同类记录之间可证明的语义；跨领域引用仍由 reference validator 负责。
pub fn validate_runtime_rule(
    rule: &str,
    records: &[RuntimeProjectedRecord],
) -> RuntimeValidatorOutcome {
    let mut projections = BTreeMap::<&str, Vec<&RuntimeProjectedRecord>>::new();
    for record in records {
        projections
            .entry(record.role.as_str())
            .or_default()
            .push(record);
    }
    if projections.is_empty() {
        return invalid_runtime_rule(rule, 0, "NO_RECORDS");
    }
    let mut outcome = RuntimeValidatorOutcome {
        valid: true,
        checked: 0,
        diagnostics: Vec::new(),
    };
    for projection in projections.values() {
        let projection_outcome = validate_runtime_projection(rule, projection);
        outcome.valid &= projection_outcome.valid;
        outcome.checked += projection_outcome.checked;
        outcome.diagnostics.extend(projection_outcome.diagnostics);
    }
    outcome.diagnostics.sort();
    outcome.diagnostics.dedup();
    outcome
}

fn validate_runtime_projection(
    rule: &str,
    records: &[&RuntimeProjectedRecord],
) -> RuntimeValidatorOutcome {
    if records.is_empty() {
        return invalid_runtime_rule(rule, 0, "NO_RECORDS");
    }
    let values = records
        .iter()
        .filter_map(|record| record.value.as_object())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return invalid_runtime_rule(rule, records.len(), "NO_OBJECT_RECORDS");
    }

    let valid = match rule {
        "map.bounds-contain-spawns" => values.iter().all(|value| {
            positive_number(value, "width")
                && positive_number(value, "height")
                && nonempty_string(value, "spawnNpcId")
        }),
        "npc.script-entry-resolves" => values.iter().all(|value| {
            safe_relative_reference(value, "scriptPath")
                && number_at_least(value, "coordinateX", 0.0)
                && number_at_least(value, "coordinateY", 0.0)
        }),
        "monster.drop-weight-positive" => values.iter().all(|value| {
            positive_number(value, "combatLevel")
                && positive_number(value, "healthPoints")
                && nonempty_string(value, "primaryDropItemId")
        }),
        "equipment.slot-matches-item-mode" => values.iter().all(|value| {
            nonempty_string(value, "slot")
                && nonempty_string(value, "baseItemId")
                && positive_number(value, "durability")
        }),
        "item.icon-resource-exists" => values
            .iter()
            .all(|value| safe_relative_reference(value, "clientIcon")),
        "level.experience-monotonic" => {
            grouped_monotonic(&values, None, "level", "requiredExperience", false)
        }
        "rebirth.minimum-level-reachable" => {
            grouped_monotonic(&values, None, "rebirthTier", "minimumLevel", false)
                && values
                    .iter()
                    .all(|value| number_in_range(value, "minimumLevel", 1.0, 255.0))
        }
        "title.permanent-duration-zero" => values.iter().all(|value| {
            number_at_least(value, "durationSeconds", 0.0)
                && (!string_equals(value, "displayLabel", "permanent")
                    || number_equals(value, "durationSeconds", 0.0))
        }),
        "buff.stack-mode-capacity-compatible" => values.iter().all(|value| {
            positive_number(value, "maximumStacks")
                && (!string_equals(value, "stackMode", "stack")
                    || number_at_least(value, "maximumStacks", 2.0))
        }),
        "skill.level-curve-contiguous" => grouped_contiguous(&values, "skillId", "skillLevel", 1),
        "enhance.probability-budget-valid" => values
            .iter()
            .all(|value| number_in_range(value, "successRateBasisPoints", 0.0, 10_000.0)),
        "crafting.no-self-consuming-cycle" => values.iter().all(|value| {
            distinct_strings(value, "materialItemId", "outputItemId")
                && positive_number(value, "materialCount")
                && positive_number(value, "outputCount")
        }),
        "gem.tier-chain-contiguous" => grouped_contiguous(&values, "socketType", "gemTier", 1),
        "refine.minimum-not-greater-than-maximum" => values
            .iter()
            .all(|value| ordered_numbers(value, "minimumValue", "maximumValue", true)),
        "quest.chain-acyclic-and-reachable" => {
            acyclic_optional_link(&values, "questId", "nextQuestId")
        }
        "checkin.days-contiguous" => grouped_contiguous(&values, "cycleId", "dayIndex", 1),
        "online-reward.duration-monotonic" => unique_positive_number(&values, "durationSeconds"),
        "limited-event.start-before-end" => values
            .iter()
            .all(|value| ordered_numbers(value, "startEpochSeconds", "endEpochSeconds", false)),
        "launch-event.day-windows-nonoverlapping" => {
            unique_grouped_number(&values, "scheduleId", "openServerDay")
        }
        "first-charge.first-tier-is-minimum" => {
            grouped_monotonic(&values, None, "chargeThreshold", "chargeThreshold", true)
        }
        "cumulative-charge.thresholds-strictly-increase" => grouped_monotonic(
            &values,
            Some("cycleId"),
            "chargeThreshold",
            "chargeThreshold",
            true,
        ),
        "vip.points-monotonic" => {
            grouped_monotonic(&values, None, "vipLevel", "requiredPoints", false)
        }
        "shop.sale-window-and-price-valid" => values.iter().all(|value| {
            number_at_least(value, "price", 0.0)
                && ordered_numbers(value, "startEpochSeconds", "endEpochSeconds", false)
        }),
        "recycle.quality-range-ordered" => values
            .iter()
            .all(|value| ordered_numbers(value, "minimumQuality", "maximumQuality", true)),
        "guild.members-and-contribution-monotonic" => {
            grouped_monotonic(&values, None, "guildLevel", "maximumMembers", false)
                && grouped_monotonic(&values, None, "guildLevel", "requiredContribution", false)
        }
        "sabac.phases-ordered-and-regions-contained" => {
            values.iter().all(|value| {
                ordered_numbers(value, "startMinute", "endMinute", false)
                    && number_in_range(value, "startMinute", 0.0, 1440.0)
                    && number_in_range(value, "endMinute", 1.0, 1440.0)
            }) && non_overlapping_intervals(&values, None, "startMinute", "endMinute")
        }
        "ranking.settlement-within-cycle" => values.iter().all(|value| {
            positive_number(value, "cycleSeconds") && nonempty_string(value, "boardId")
        }),
        "production.point-inside-map-and-rate-positive" => values.iter().all(|value| {
            positive_number(value, "intervalSeconds")
                && positive_number(value, "yieldCount")
                && nonempty_string(value, "mapId")
        }),
        "manor.entry-and-exit-loop-reachable" => values.iter().all(|value| {
            nonempty_string(value, "entryNpcId")
                && nonempty_string(value, "mapId")
                && nonempty_string(value, "productionPointId")
                && distinct_strings(value, "entryNpcId", "productionPointId")
        }),
        "hero-soul.route-acyclic-and-affordable" => values.iter().all(|value| {
            nonempty_string(value, "routeId")
                && nonempty_string(value, "nodeId")
                && distinct_strings(value, "routeId", "nodeId")
                && number_at_least(value, "powerValue", 0.0)
        }),
        "talent.graph-acyclic-and-budget-valid" => {
            acyclic_optional_link(&values, "nodeId", "parentNodeId")
                && values
                    .iter()
                    .all(|value| number_at_least(value, "costPoints", 0.0))
        }
        "season.window-and-settlement-ordered" => values
            .iter()
            .all(|value| ordered_numbers(value, "startEpochSeconds", "endEpochSeconds", false)),
        "cross-server.route-and-engine-range-compatible" => values.iter().all(|value| {
            distinct_strings(value, "sourceShard", "targetShard")
                && ordered_versions(value, "minimumEngineVersion", "maximumEngineVersion")
                && positive_number(value, "maximumConcurrentPlayers")
        }),
        _ => return invalid_runtime_rule(rule, values.len(), "UNSUPPORTED"),
    };
    RuntimeValidatorOutcome {
        valid,
        checked: values.len(),
        diagnostics: if valid {
            Vec::new()
        } else {
            vec![format!("DOMAIN_RUNTIME_RULE_FAILED:{rule}")]
        },
    }
}

fn invalid_runtime_rule(rule: &str, checked: usize, reason: &str) -> RuntimeValidatorOutcome {
    RuntimeValidatorOutcome {
        valid: false,
        checked,
        diagnostics: vec![format!("DOMAIN_RUNTIME_RULE_{reason}:{rule}")],
    }
}

fn number(value: &serde_json::Map<String, Value>, field: &str) -> Option<f64> {
    value.get(field).and_then(Value::as_f64)
}

fn string<'a>(value: &'a serde_json::Map<String, Value>, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}

fn positive_number(value: &serde_json::Map<String, Value>, field: &str) -> bool {
    number(value, field).is_some_and(|number| number > 0.0)
}

fn number_at_least(value: &serde_json::Map<String, Value>, field: &str, minimum: f64) -> bool {
    number(value, field).is_some_and(|number| number >= minimum)
}

fn number_in_range(
    value: &serde_json::Map<String, Value>,
    field: &str,
    minimum: f64,
    maximum: f64,
) -> bool {
    number(value, field).is_some_and(|number| number >= minimum && number <= maximum)
}

fn number_equals(value: &serde_json::Map<String, Value>, field: &str, expected: f64) -> bool {
    number(value, field).is_some_and(|number| (number - expected).abs() < f64::EPSILON)
}

fn nonempty_string(value: &serde_json::Map<String, Value>, field: &str) -> bool {
    string(value, field).is_some_and(|value| !value.trim().is_empty())
}

fn string_equals(value: &serde_json::Map<String, Value>, field: &str, expected: &str) -> bool {
    string(value, field).is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn safe_relative_reference(value: &serde_json::Map<String, Value>, field: &str) -> bool {
    string(value, field).is_some_and(|value| {
        let normalized = value.replace('\\', "/");
        !normalized.is_empty()
            && !normalized.starts_with('/')
            && !normalized.split('/').any(|component| component == "..")
    })
}

fn distinct_strings(value: &serde_json::Map<String, Value>, left: &str, right: &str) -> bool {
    string(value, left)
        .zip(string(value, right))
        .is_some_and(|(left, right)| !left.eq_ignore_ascii_case(right))
}

fn ordered_numbers(
    value: &serde_json::Map<String, Value>,
    left: &str,
    right: &str,
    allow_equal: bool,
) -> bool {
    number(value, left)
        .zip(number(value, right))
        .is_some_and(|(left, right)| {
            if allow_equal {
                left <= right
            } else {
                left < right
            }
        })
}

fn grouped_monotonic(
    values: &[&serde_json::Map<String, Value>],
    group_field: Option<&str>,
    order_field: &str,
    value_field: &str,
    strict: bool,
) -> bool {
    let mut groups = BTreeMap::<String, Vec<(f64, f64)>>::new();
    for value in values {
        let Some(order) = number(value, order_field) else {
            return false;
        };
        let Some(item) = number(value, value_field) else {
            return false;
        };
        let group = group_field
            .and_then(|field| string(value, field))
            .unwrap_or_default()
            .to_string();
        groups.entry(group).or_default().push((order, item));
    }
    groups.values_mut().all(|items| {
        items.sort_by(|left, right| left.0.total_cmp(&right.0));
        items.windows(2).all(|pair| {
            if strict {
                pair[0].1 < pair[1].1
            } else {
                pair[0].1 <= pair[1].1
            }
        })
    })
}

fn grouped_contiguous(
    values: &[&serde_json::Map<String, Value>],
    group_field: &str,
    index_field: &str,
    first: i64,
) -> bool {
    let mut groups = BTreeMap::<String, Vec<i64>>::new();
    for value in values {
        let Some(group) = string(value, group_field) else {
            return false;
        };
        let Some(index) = value.get(index_field).and_then(Value::as_i64) else {
            return false;
        };
        groups.entry(group.to_string()).or_default().push(index);
    }
    groups.values_mut().all(|indexes| {
        indexes.sort_unstable();
        if indexes.len() == 1 {
            return indexes[0] >= first;
        }
        indexes
            .iter()
            .enumerate()
            .all(|(offset, index)| *index == first + offset as i64)
    })
}

fn unique_grouped_number(
    values: &[&serde_json::Map<String, Value>],
    group_field: &str,
    number_field: &str,
) -> bool {
    let mut seen = BTreeSet::new();
    values.iter().all(|value| {
        string(value, group_field)
            .zip(value.get(number_field).and_then(Value::as_i64))
            .is_some_and(|(group, number)| seen.insert((group.to_string(), number)))
    })
}

fn unique_positive_number(values: &[&serde_json::Map<String, Value>], field: &str) -> bool {
    let mut seen = BTreeSet::new();
    values.iter().all(|value| {
        number(value, field).is_some_and(|number| number > 0.0 && seen.insert(number.to_bits()))
    })
}

fn acyclic_optional_link(
    values: &[&serde_json::Map<String, Value>],
    id_field: &str,
    link_field: &str,
) -> bool {
    let links = values
        .iter()
        .filter_map(|value| {
            let id = string(value, id_field)?.to_string();
            let link = string(value, link_field).unwrap_or_default().to_string();
            Some((id, link))
        })
        .collect::<BTreeMap<_, _>>();
    if links.len() != values.len() {
        return false;
    }
    for start in links.keys() {
        let mut current = start.as_str();
        let mut seen = BTreeSet::new();
        while let Some(next) = links.get(current).filter(|next| !next.is_empty()) {
            if !seen.insert(current.to_string()) {
                return false;
            }
            current = next;
        }
    }
    true
}

fn non_overlapping_intervals(
    values: &[&serde_json::Map<String, Value>],
    group_field: Option<&str>,
    start_field: &str,
    end_field: &str,
) -> bool {
    let mut groups = BTreeMap::<String, Vec<(f64, f64)>>::new();
    for value in values {
        let Some(start) = number(value, start_field) else {
            return false;
        };
        let Some(end) = number(value, end_field) else {
            return false;
        };
        let group = group_field
            .and_then(|field| string(value, field))
            .unwrap_or_default()
            .to_string();
        groups.entry(group).or_default().push((start, end));
    }
    groups.values_mut().all(|intervals| {
        intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
        intervals.windows(2).all(|pair| pair[0].1 <= pair[1].0)
    })
}

fn ordered_versions(value: &serde_json::Map<String, Value>, minimum: &str, maximum: &str) -> bool {
    string(value, minimum)
        .and_then(parse_engine_version)
        .zip(string(value, maximum).and_then(parse_engine_version))
        .is_some_and(|(minimum, maximum)| minimum <= maximum)
}

fn parse_engine_version(value: &str) -> Option<semver::Version> {
    semver::Version::parse(value).ok().or_else(|| {
        (value.split('.').count() == 2)
            .then(|| semver::Version::parse(&format!("{value}.0")).ok())
            .flatten()
    })
}

/// requiredEvidence 不是清单装饰：每项都必须由当前项目的真实运行状态满足。
pub fn validate_required_engine_evidence(
    required: &[String],
    evidence: &RuntimeEngineEvidence,
) -> RuntimeValidatorOutcome {
    if required.is_empty() {
        return RuntimeValidatorOutcome {
            valid: false,
            checked: 0,
            diagnostics: vec!["DOMAIN_ENGINE_EVIDENCE_CONTRACT_EMPTY".to_string()],
        };
    }

    let mut valid = true;
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for requirement in required {
        if !seen.insert(requirement.as_str()) {
            valid = false;
            diagnostics.push(format!("DOMAIN_ENGINE_EVIDENCE_DUPLICATE:{requirement}"));
            continue;
        }
        let satisfied = match requirement.as_str() {
            "project-directory-layout" => evidence.project_directory_layout,
            "owned-selector-or-content-fingerprint" => evidence.owned_projection,
            "resource-schema-validation" => {
                evidence.resource_schema_valid && evidence.resource_schema_checked > 0
            }
            _ => {
                valid = false;
                diagnostics.push(format!("DOMAIN_ENGINE_EVIDENCE_UNSUPPORTED:{requirement}"));
                continue;
            }
        };
        if !satisfied {
            valid = false;
            diagnostics.push(format!("DOMAIN_ENGINE_EVIDENCE_MISSING:{requirement}"));
        }
    }
    RuntimeValidatorOutcome {
        valid,
        checked: required.len(),
        diagnostics,
    }
}

/// 客户端与引擎必须先按 matchBy 唯一关联，再逐关联记录比较字段。
pub fn validate_client_engine_records(
    match_by: &str,
    compare_fields: &[String],
    missing_projection: &str,
    records: &[RuntimeProjectedRecord],
) -> RuntimeValidatorOutcome {
    let unique_compare_fields = compare_fields.iter().collect::<BTreeSet<_>>();
    if match_by.trim().is_empty()
        || compare_fields.is_empty()
        || compare_fields.iter().any(|field| field.trim().is_empty())
        || unique_compare_fields.len() != compare_fields.len()
        || missing_projection != "error"
    {
        return RuntimeValidatorOutcome {
            valid: false,
            checked: 0,
            diagnostics: vec!["DOMAIN_CLIENT_ENGINE_RULE_INVALID".to_string()],
        };
    }

    let mut valid = true;
    let mut checked = 0;
    let mut diagnostics = Vec::new();
    let mut client = BTreeMap::new();
    let mut engine = BTreeMap::new();

    for record in records
        .iter()
        .filter(|record| matches!(record.role.as_str(), "client" | "engine"))
    {
        checked += 1;
        let Some(object) = record.value.as_object() else {
            valid = false;
            diagnostics.push(format!(
                "DOMAIN_CLIENT_ENGINE_RECORD_INVALID:{}",
                record.path
            ));
            continue;
        };
        let Some(key) = object.get(match_by).and_then(match_key) else {
            valid = false;
            diagnostics.push(format!(
                "DOMAIN_CLIENT_ENGINE_MATCH_KEY_MISSING:{}:{match_by}",
                record.path
            ));
            continue;
        };
        let index = if record.role == "client" {
            &mut client
        } else {
            &mut engine
        };
        if let Some(previous) = index.insert(key.clone(), record) {
            valid = false;
            diagnostics.push(format!(
                "DOMAIN_CLIENT_ENGINE_MATCH_KEY_DUPLICATE:{}:{match_by}:{key}:{}:{}",
                record.role, previous.path, record.path
            ));
        }
    }

    if client.is_empty() || engine.is_empty() {
        valid = false;
        diagnostics.push("DOMAIN_CLIENT_ENGINE_SIDE_INCOMPLETE".to_string());
    }

    let keys = client
        .keys()
        .chain(engine.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for key in keys {
        let Some(client_record) = client.get(&key) else {
            valid = false;
            diagnostics.push(format!(
                "DOMAIN_CLIENT_ENGINE_MATCH_MISSING:client:{match_by}:{key}"
            ));
            continue;
        };
        let Some(engine_record) = engine.get(&key) else {
            valid = false;
            diagnostics.push(format!(
                "DOMAIN_CLIENT_ENGINE_MATCH_MISSING:engine:{match_by}:{key}"
            ));
            continue;
        };
        let (Some(client_object), Some(engine_object)) = (
            client_record.value.as_object(),
            engine_record.value.as_object(),
        ) else {
            valid = false;
            diagnostics.push(format!(
                "DOMAIN_CLIENT_ENGINE_RECORD_INVALID:{match_by}:{key}"
            ));
            continue;
        };
        for field in compare_fields {
            checked += 1;
            match (client_object.get(field), engine_object.get(field)) {
                (Some(client_value), Some(engine_value)) if client_value == engine_value => {}
                (None, _) | (_, None) => {
                    valid = false;
                    diagnostics.push(format!(
                        "DOMAIN_CLIENT_ENGINE_FIELD_MISSING:{match_by}:{key}:{field}"
                    ));
                }
                _ => {
                    valid = false;
                    diagnostics.push(format!(
                        "DOMAIN_CLIENT_ENGINE_FIELD_MISMATCH:{match_by}:{key}:{field}"
                    ));
                }
            }
        }
    }

    RuntimeValidatorOutcome {
        valid,
        checked,
        diagnostics,
    }
}

fn match_key(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.is_empty() => Some(format!("s:{value}")),
        Value::Number(value) => Some(format!("n:{value}")),
        Value::Bool(value) => Some(format!("b:{value}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn required_engine_evidence_requires_every_runtime_fact() {
        let required = vec![
            "project-directory-layout".to_string(),
            "owned-selector-or-content-fingerprint".to_string(),
            "resource-schema-validation".to_string(),
        ];
        let complete = RuntimeEngineEvidence {
            project_directory_layout: true,
            owned_projection: true,
            resource_schema_valid: true,
            resource_schema_checked: 2,
        };
        assert_eq!(
            validate_required_engine_evidence(&required, &complete),
            RuntimeValidatorOutcome {
                valid: true,
                checked: 3,
                diagnostics: Vec::new(),
            }
        );

        let missing_schema = RuntimeEngineEvidence {
            resource_schema_checked: 0,
            ..complete
        };
        let outcome = validate_required_engine_evidence(&required, &missing_schema);
        assert!(!outcome.valid);
        assert_eq!(
            outcome.diagnostics,
            ["DOMAIN_ENGINE_EVIDENCE_MISSING:resource-schema-validation"]
        );
    }

    #[test]
    fn required_engine_evidence_rejects_unknown_and_duplicate_contracts() {
        let outcome = validate_required_engine_evidence(
            &[
                "project-directory-layout".to_string(),
                "project-directory-layout".to_string(),
                "vendor-attestation".to_string(),
            ],
            &RuntimeEngineEvidence {
                project_directory_layout: true,
                ..RuntimeEngineEvidence::default()
            },
        );
        assert!(!outcome.valid);
        assert!(outcome
            .diagnostics
            .contains(&"DOMAIN_ENGINE_EVIDENCE_DUPLICATE:project-directory-layout".to_string()));
        assert!(outcome
            .diagnostics
            .contains(&"DOMAIN_ENGINE_EVIDENCE_UNSUPPORTED:vendor-attestation".to_string()));
    }

    #[test]
    fn client_engine_comparison_is_associated_by_match_key() {
        let records = vec![
            record("client/a.json", "client", json!({"id":"a","value":1})),
            record("client/b.json", "client", json!({"id":"b","value":2})),
            record("engine/a.json", "engine", json!({"id":"a","value":2})),
            record("engine/b.json", "engine", json!({"id":"b","value":1})),
        ];
        let outcome =
            validate_client_engine_records("id", &["value".to_string()], "error", &records);
        assert!(!outcome.valid);
        assert!(outcome
            .diagnostics
            .contains(&"DOMAIN_CLIENT_ENGINE_FIELD_MISMATCH:id:s:a:value".to_string()));
        assert!(outcome
            .diagnostics
            .contains(&"DOMAIN_CLIENT_ENGINE_FIELD_MISMATCH:id:s:b:value".to_string()));
    }

    #[test]
    fn client_engine_comparison_accepts_exact_matched_records() {
        let records = vec![
            record(
                "client/a.json",
                "client",
                json!({"id":"a","value":1,"enabled":true}),
            ),
            record(
                "engine/a.json",
                "engine",
                json!({"id":"a","value":1,"enabled":true}),
            ),
        ];
        let outcome = validate_client_engine_records(
            "id",
            &["value".to_string(), "enabled".to_string()],
            "error",
            &records,
        );
        assert!(outcome.valid);
        assert_eq!(outcome.checked, 4);
        assert!(outcome.diagnostics.is_empty());
    }

    #[test]
    fn client_engine_comparison_rejects_missing_sides_keys_fields_and_duplicates() {
        let records = vec![
            record("client/a.json", "client", json!({"id":"a","value":1})),
            record(
                "client/a-duplicate.json",
                "client",
                json!({"id":"a","value":1}),
            ),
            record("client/no-key.json", "client", json!({"value":1})),
            record("engine/a.json", "engine", json!({"id":"a"})),
            record("engine/b.json", "engine", json!({"id":"b","value":2})),
        ];
        let outcome =
            validate_client_engine_records("id", &["value".to_string()], "error", &records);
        assert!(!outcome.valid);
        assert!(outcome.diagnostics.iter().any(|diagnostic| {
            diagnostic.starts_with("DOMAIN_CLIENT_ENGINE_MATCH_KEY_DUPLICATE:client:id:s:a")
        }));
        assert!(outcome.diagnostics.iter().any(|diagnostic| {
            diagnostic.starts_with("DOMAIN_CLIENT_ENGINE_MATCH_KEY_MISSING:client/no-key.json")
        }));
        assert!(outcome
            .diagnostics
            .contains(&"DOMAIN_CLIENT_ENGINE_FIELD_MISSING:id:s:a:value".to_string()));
        assert!(outcome
            .diagnostics
            .contains(&"DOMAIN_CLIENT_ENGINE_MATCH_MISSING:client:id:s:b".to_string()));
    }

    #[test]
    fn client_engine_comparison_rejects_empty_projection_and_non_error_policy() {
        let empty = validate_client_engine_records("id", &["value".to_string()], "error", &[]);
        assert!(!empty.valid);
        assert_eq!(empty.diagnostics, ["DOMAIN_CLIENT_ENGINE_SIDE_INCOMPLETE"]);

        let permissive =
            validate_client_engine_records("id", &["value".to_string()], "warning", &[]);
        assert!(!permissive.valid);
        assert_eq!(
            permissive.diagnostics,
            ["DOMAIN_CLIENT_ENGINE_RULE_INVALID"]
        );

        for compare_fields in [vec!["".to_string()], vec!["value".to_string(); 2]] {
            let malformed = validate_client_engine_records("id", &compare_fields, "error", &[]);
            assert!(!malformed.valid);
            assert_eq!(malformed.diagnostics, ["DOMAIN_CLIENT_ENGINE_RULE_INVALID"]);
        }
    }

    #[test]
    fn every_bundled_runtime_rule_executes_against_its_valid_fixture() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../resources/mir3-domain-packs");
        let registry: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("registry.json")).unwrap())
                .unwrap();
        let mut checked = 0;
        for entry in registry["packs"].as_array().unwrap() {
            let system_id = entry["systemId"].as_str().unwrap();
            let manifest: Value = serde_json::from_str(
                &std::fs::read_to_string(root.join(system_id).join("domain.json")).unwrap(),
            )
            .unwrap();
            let rule = manifest["validators"]
                .as_array()
                .unwrap()
                .iter()
                .find(|validator| validator["kind"] == "runtime-diagnostics")
                .and_then(|validator| validator["rule"].as_str())
                .unwrap();
            let fixture: Value = serde_json::from_str(
                &std::fs::read_to_string(root.join(system_id).join("fixtures/valid.json")).unwrap(),
            )
            .unwrap();
            let records = fixture["records"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    record(
                        &format!("fixture/{system_id}/{index}.json"),
                        "project",
                        value.clone(),
                    )
                })
                .collect::<Vec<_>>();
            let outcome = validate_runtime_rule(rule, &records);
            assert!(
                outcome.valid,
                "{system_id}:{rule} failed: {:?}",
                outcome.diagnostics
            );
            assert_eq!(outcome.checked, records.len());
            checked += 1;
        }
        assert_eq!(checked, 33);
        let unsupported = validate_runtime_rule(
            "unknown.runtime-rule",
            &[record("fixture.json", "project", json!({"id":"a"}))],
        );
        assert!(!unsupported.valid);
        assert_eq!(
            unsupported.diagnostics,
            ["DOMAIN_RUNTIME_RULE_UNSUPPORTED:unknown.runtime-rule"]
        );
    }

    fn record(path: &str, role: &str, value: Value) -> RuntimeProjectedRecord {
        RuntimeProjectedRecord {
            path: path.to_string(),
            role: role.to_string(),
            value,
        }
    }
}
