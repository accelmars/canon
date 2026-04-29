use std::collections::HashSet;
use std::path::Path;

use serde_json::Value as Json;

use super::drift::{DriftCategory, DriftEntry};

/// Parse YAML frontmatter delimited by `---` lines.
///
/// Returns `None` when the file does not begin with a `---` block.
pub fn parse_frontmatter(content: &str) -> Option<serde_yaml::Value> {
    // Strip UTF-8 BOM if present.
    let content = content.trim_start_matches('\u{feff}');
    let rest = content.strip_prefix("---")?;
    // Accept `---\n` or `--- \n` (trailing space) after the opening fence.
    let rest = if let Some(r) = rest.strip_prefix('\n') {
        r
    } else {
        rest.strip_prefix("\r\n")?
    };
    // Find closing fence — must appear at the start of a line.
    let end = rest.find("\n---").or_else(|| rest.find("\r\n---"))?;
    let yaml_str = &rest[..end];
    serde_yaml::from_str(yaml_str).ok()
}

/// Validate the frontmatter of a file against a loaded JSON Schema value.
///
/// `schema` is the parsed JSON Schema object (the full schema JSON, not just properties).
/// Emits `FrontmatterRequiredMissing`, `FrontmatterTypeWrong`, `FrontmatterValueInvalid`,
/// and `UnknownFieldInfo`.
pub fn check_frontmatter(path: &Path, content: &str, schema: Option<&Json>) -> Vec<DriftEntry> {
    let mut entries = Vec::new();

    let Some(schema) = schema else {
        return entries;
    };

    let fm_yaml = match parse_frontmatter(content) {
        Some(v) => v,
        None => {
            entries.push(DriftEntry {
                path: path.to_owned(),
                category: DriftCategory::FrontmatterRequiredMissing,
                message: "file has no frontmatter".to_string(),
            });
            return entries;
        }
    };

    let fm_json = yaml_to_json(&fm_yaml);
    let fm_obj = match &fm_json {
        Json::Object(m) => m,
        _ => return entries,
    };

    // Check base required fields.
    if let Some(Json::Array(required)) = schema.get("required") {
        for req in required {
            if let Json::String(field) = req {
                if !fm_obj.contains_key(field) {
                    entries.push(DriftEntry {
                        path: path.to_owned(),
                        category: DriftCategory::FrontmatterRequiredMissing,
                        message: format!("required field '{}' is absent", field),
                    });
                }
            }
        }
    }

    // Check properties: type and enum.
    let known_fields: HashSet<&str> = if let Some(Json::Object(props)) = schema.get("properties") {
        props.keys().map(|k| k.as_str()).collect()
    } else {
        HashSet::new()
    };

    if let Some(Json::Object(props)) = schema.get("properties") {
        for (field, value) in fm_obj {
            if let Some(prop_schema) = props.get(field) {
                // Type check.
                if let Some(Json::String(expected_type)) = prop_schema.get("type") {
                    if !json_value_matches_type(value, expected_type) {
                        entries.push(DriftEntry {
                            path: path.to_owned(),
                            category: DriftCategory::FrontmatterTypeWrong,
                            message: format!(
                                "field '{}' expected type '{}' but got '{}'",
                                field,
                                expected_type,
                                json_type_name(value)
                            ),
                        });
                        continue; // Skip enum check when type is wrong.
                    }
                }
                // Enum check.
                if let Some(Json::Array(enum_vals)) = prop_schema.get("enum") {
                    if !enum_vals.contains(value) {
                        let allowed: Vec<&str> =
                            enum_vals.iter().filter_map(|v| v.as_str()).collect();
                        entries.push(DriftEntry {
                            path: path.to_owned(),
                            category: DriftCategory::FrontmatterValueInvalid,
                            message: format!(
                                "field '{}' value '{}' is not in allowed enum [{}]",
                                field,
                                value_to_display(value),
                                allowed.join(", ")
                            ),
                        });
                    }
                }
            } else if !known_fields.contains(field.as_str()) {
                entries.push(DriftEntry {
                    path: path.to_owned(),
                    category: DriftCategory::UnknownFieldInfo,
                    message: format!("field '{}' is not defined in the frontmatter schema", field),
                });
            }
        }
    }

    // Check allOf conditions (type-specific required fields).
    if let Some(Json::Array(all_of)) = schema.get("allOf") {
        for condition in all_of {
            let (Some(if_clause), Some(then_clause)) = (condition.get("if"), condition.get("then"))
            else {
                continue;
            };
            if !matches_if_clause(fm_obj, if_clause) {
                continue;
            }
            if let Some(Json::Array(then_required)) = then_clause.get("required") {
                let type_name = get_const_type(if_clause).unwrap_or("unknown");
                for req in then_required {
                    if let Json::String(field) = req {
                        if !fm_obj.contains_key(field) {
                            entries.push(DriftEntry {
                                path: path.to_owned(),
                                category: DriftCategory::FrontmatterRequiredMissing,
                                message: format!(
                                    "field '{}' is required for type '{}' but is absent",
                                    field, type_name
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    entries
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn matches_if_clause(fm_obj: &serde_json::Map<String, Json>, if_clause: &Json) -> bool {
    let props_ok = match if_clause.get("properties") {
        Some(Json::Object(if_props)) => if_props.iter().all(|(field, constraint)| {
            let Some(fm_val) = fm_obj.get(field) else {
                return false;
            };
            match constraint.get("const") {
                Some(Json::String(const_val)) => {
                    matches!(fm_val, Json::String(s) if s == const_val)
                }
                _ => true,
            }
        }),
        _ => true,
    };

    let required_ok = match if_clause.get("required") {
        Some(Json::Array(required)) => required
            .iter()
            .all(|r| matches!(r, Json::String(f) if fm_obj.contains_key(f))),
        _ => true,
    };

    props_ok && required_ok
}

fn get_const_type(if_clause: &Json) -> Option<&str> {
    if_clause
        .get("properties")?
        .get("type")?
        .get("const")?
        .as_str()
}

pub(crate) fn yaml_to_json(yaml: &serde_yaml::Value) -> Json {
    match yaml {
        serde_yaml::Value::Null => Json::Null,
        serde_yaml::Value::Bool(b) => Json::Bool(*b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Json::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                Json::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(Json::Number)
                    .unwrap_or(Json::Null)
            } else {
                Json::Null
            }
        }
        serde_yaml::Value::String(s) => Json::String(s.clone()),
        serde_yaml::Value::Sequence(seq) => Json::Array(seq.iter().map(yaml_to_json).collect()),
        serde_yaml::Value::Mapping(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                if let serde_yaml::Value::String(key) = k {
                    obj.insert(key.clone(), yaml_to_json(v));
                }
            }
            Json::Object(obj)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_json(&tagged.value),
    }
}

fn json_value_matches_type(value: &Json, expected_type: &str) -> bool {
    match expected_type {
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn json_type_name(value: &Json) -> &'static str {
    match value {
        Json::Null => "null",
        Json::Bool(_) => "boolean",
        Json::Number(_) => "number",
        Json::String(_) => "string",
        Json::Array(_) => "array",
        Json::Object(_) => "object",
    }
}

fn value_to_display(value: &Json) -> String {
    match value {
        Json::String(s) => s.clone(),
        _ => value.to_string(),
    }
}
