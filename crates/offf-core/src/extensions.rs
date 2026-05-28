//! Path helpers and append-only I/O for OFFF generic extension points.
//!
//! Standard directory layout inside a case container:
//! ```text
//! extensions/
//!   labels/labels.jsonl
//!   scopes/scopes.jsonl
//!   sets/working_sets.jsonl
//!   sets/release_sets.jsonl
//!   sets/exclusion_sets.jsonl
//!   decisions/decisions.jsonl
//!   policies/policy_refs.jsonl
//!   access/access_events.jsonl
//!   access/denied_access_events.jsonl
//!   audit/audit_events.jsonl
//! ```

use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use serde::{de::DeserializeOwned, Serialize};

use crate::{
    error::OfffError,
    types::{
        AccessEvent, AuditEvent, DecisionRecord, DeniedAccessEvent, DiscoveredObjectRow,
        LabelEvent, ObjectEdgeEvent, ObjectEvent, PolicyRef, ScopeRecord, SetRecord,
    },
};

// ── Standard extension paths ──────────────────────────────────────────────────

pub fn labels_path(case: &Path) -> PathBuf {
    case.join("extensions/labels/labels.jsonl")
}

pub fn scopes_path(case: &Path) -> PathBuf {
    case.join("extensions/scopes/scopes.jsonl")
}

pub fn working_sets_path(case: &Path) -> PathBuf {
    case.join("extensions/sets/working_sets.jsonl")
}

pub fn release_sets_path(case: &Path) -> PathBuf {
    case.join("extensions/sets/release_sets.jsonl")
}

pub fn exclusion_sets_path(case: &Path) -> PathBuf {
    case.join("extensions/sets/exclusion_sets.jsonl")
}

pub fn decisions_path(case: &Path) -> PathBuf {
    case.join("extensions/decisions/decisions.jsonl")
}

pub fn policy_refs_path(case: &Path) -> PathBuf {
    case.join("extensions/policies/policy_refs.jsonl")
}

pub fn access_events_path(case: &Path) -> PathBuf {
    case.join("extensions/access/access_events.jsonl")
}

pub fn denied_access_events_path(case: &Path) -> PathBuf {
    case.join("extensions/access/denied_access_events.jsonl")
}

pub fn audit_events_path(case: &Path) -> PathBuf {
    case.join("extensions/audit/audit_events.jsonl")
}

// ── Generic JSONL I/O ─────────────────────────────────────────────────────────

/// Append one serialisable record to a JSONL file.
/// Creates parent directories and the file if they do not exist.
pub fn append_jsonl<T: Serialize>(path: &Path, record: &T) -> Result<(), OfffError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(record)?;
    writeln!(file, "{line}")?;
    Ok(())
}

/// Read all records from a JSONL file.
/// Returns an empty `Vec` if the file does not exist.
pub fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, OfffError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let reader = BufReader::new(fs::File::open(path)?);
    let mut records = Vec::new();
    for line in reader.lines() {
        let raw = line?;
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            records.push(serde_json::from_str(trimmed)?);
        }
    }
    Ok(records)
}

// ── Typed append helpers ──────────────────────────────────────────────────────

pub fn append_label_event(case: &Path, ev: &LabelEvent) -> Result<(), OfffError> {
    append_jsonl(&labels_path(case), ev)
}

pub fn read_label_events(case: &Path) -> Result<Vec<LabelEvent>, OfffError> {
    read_jsonl(&labels_path(case))
}

pub fn append_scope(case: &Path, scope: &ScopeRecord) -> Result<(), OfffError> {
    append_jsonl(&scopes_path(case), scope)
}

pub fn read_scopes(case: &Path) -> Result<Vec<ScopeRecord>, OfffError> {
    read_jsonl(&scopes_path(case))
}

/// Append a set record to the appropriate JSONL file based on `set.set_type`.
pub fn append_set(case: &Path, set: &SetRecord) -> Result<(), OfffError> {
    let path = match set.set_type.as_str() {
        "release_set" => release_sets_path(case),
        "exclusion_set" => exclusion_sets_path(case),
        _ => working_sets_path(case),
    };
    append_jsonl(&path, set)
}

/// Read all set records from the file for the given `set_type`.
pub fn read_sets(case: &Path, set_type: &str) -> Result<Vec<SetRecord>, OfffError> {
    let path = match set_type {
        "release_set" => release_sets_path(case),
        "exclusion_set" => exclusion_sets_path(case),
        _ => working_sets_path(case),
    };
    read_jsonl(&path)
}

pub fn append_decision(case: &Path, decision: &DecisionRecord) -> Result<(), OfffError> {
    append_jsonl(&decisions_path(case), decision)
}

pub fn read_decisions(case: &Path) -> Result<Vec<DecisionRecord>, OfffError> {
    read_jsonl(&decisions_path(case))
}

pub fn append_policy_ref(case: &Path, policy: &PolicyRef) -> Result<(), OfffError> {
    append_jsonl(&policy_refs_path(case), policy)
}

pub fn read_policy_refs(case: &Path) -> Result<Vec<PolicyRef>, OfffError> {
    read_jsonl(&policy_refs_path(case))
}

pub fn append_access_event(case: &Path, ev: &AccessEvent) -> Result<(), OfffError> {
    append_jsonl(&access_events_path(case), ev)
}

pub fn read_access_events(case: &Path) -> Result<Vec<AccessEvent>, OfffError> {
    read_jsonl(&access_events_path(case))
}

pub fn append_denied_access_event(case: &Path, ev: &DeniedAccessEvent) -> Result<(), OfffError> {
    append_jsonl(&denied_access_events_path(case), ev)
}

pub fn read_denied_access_events(case: &Path) -> Result<Vec<DeniedAccessEvent>, OfffError> {
    read_jsonl(&denied_access_events_path(case))
}

pub fn append_audit_event(case: &Path, ev: &AuditEvent) -> Result<(), OfffError> {
    append_jsonl(&audit_events_path(case), ev)
}

pub fn read_audit_events(case: &Path) -> Result<Vec<AuditEvent>, OfffError> {
    read_jsonl(&audit_events_path(case))
}

// ── Object event log helpers (Sprint 19) ─────────────────────────────────────

pub fn object_events_path(case: &Path) -> PathBuf {
    case.join("indexes/objects/object_events.jsonl")
}

pub fn object_edge_events_path(case: &Path) -> PathBuf {
    case.join("indexes/objects/object_edge_events.jsonl")
}

/// Append an immutable object discovery/update/removal event.
pub fn append_object_event(case: &Path, ev: &ObjectEvent) -> Result<(), OfffError> {
    append_jsonl(&object_events_path(case), ev)
}

pub fn read_object_events(case: &Path) -> Result<Vec<ObjectEvent>, OfffError> {
    read_jsonl(&object_events_path(case))
}

/// Append an immutable object edge discovery/removal event.
pub fn append_object_edge_event(case: &Path, ev: &ObjectEdgeEvent) -> Result<(), OfffError> {
    append_jsonl(&object_edge_events_path(case), ev)
}

pub fn read_object_edge_events(case: &Path) -> Result<Vec<ObjectEdgeEvent>, OfffError> {
    read_jsonl(&object_edge_events_path(case))
}

/// Replay the object event log into a sorted `Vec<DiscoveredObjectRow>`.
///
/// Replay rules (events applied in log order):
/// - `"discovered"` / `"updated"` → insert or replace by `object_id`
/// - `"removed"` → delete by `object_id`
///
/// Unknown event types are silently ignored so the log is forward-compatible.
pub fn rebuild_object_index_from_events(
    case: &Path,
) -> Result<Vec<DiscoveredObjectRow>, OfffError> {
    use std::collections::HashMap;
    let events = read_object_events(case)?;
    let mut index: HashMap<String, DiscoveredObjectRow> = HashMap::new();
    for ev in events {
        match ev.event_type.as_str() {
            "discovered" | "updated" => {
                if let Some(payload) = ev.payload {
                    let row: DiscoveredObjectRow = serde_json::from_value(payload)
                        .map_err(|e| OfffError::InvalidContainer(e.to_string()))?;
                    index.insert(ev.object_id, row);
                }
            }
            "removed" => {
                index.remove(&ev.object_id);
            }
            _ => {}
        }
    }
    let mut rows: Vec<DiscoveredObjectRow> = index.into_values().collect();
    rows.sort_by(|a, b| a.object_id.cmp(&b.object_id));
    Ok(rows)
}

// ── Validation helpers (used by offf-verify core+extensions profile) ──────────

/// Well-known extension keys and the required top-level fields in each record.
const KNOWN_EXTENSION_VALIDATORS: &[(&str, &[&str])] = &[
    (
        "extensions/labels/labels.jsonl",
        &["label_event_id", "timestamp", "actor", "target", "label"],
    ),
    (
        "extensions/scopes/scopes.jsonl",
        &["scope_id", "created_at", "created_by"],
    ),
    (
        "extensions/sets/working_sets.jsonl",
        &["set_id", "set_type", "created_at", "created_by"],
    ),
    (
        "extensions/sets/release_sets.jsonl",
        &["set_id", "set_type", "created_at", "created_by"],
    ),
    (
        "extensions/sets/exclusion_sets.jsonl",
        &["set_id", "set_type", "created_at", "created_by"],
    ),
    (
        "extensions/decisions/decisions.jsonl",
        &[
            "decision_id",
            "timestamp",
            "actor",
            "decision_type",
            "target",
            "outcome",
        ],
    ),
    (
        "extensions/policies/policy_refs.jsonl",
        &["policy_ref", "policy_type"],
    ),
    (
        "extensions/access/access_events.jsonl",
        &[
            "access_event_id",
            "timestamp",
            "actor",
            "action",
            "target",
            "result",
        ],
    ),
    (
        "extensions/access/denied_access_events.jsonl",
        &[
            "denied_event_id",
            "timestamp",
            "actor",
            "action",
            "target",
            "result",
        ],
    ),
    (
        "extensions/audit/audit_events.jsonl",
        &["audit_event_id", "timestamp", "actor", "event_type"],
    ),
];

/// Validation result for a single extension JSONL file.
#[derive(Debug)]
pub struct ExtensionFileResult {
    pub rel_path: String,
    /// Number of records parsed successfully.
    pub record_count: usize,
    /// Validation issues found (empty = valid).
    pub issues: Vec<String>,
}

/// Validate all known extension JSONL files that are present under `case`.
/// Files that do not exist are skipped (extensions are always optional).
pub fn validate_extension_files(case: &Path) -> Vec<ExtensionFileResult> {
    let mut results = Vec::new();
    for (rel, required_fields) in KNOWN_EXTENSION_VALIDATORS {
        let path = case.join(rel);
        if !path.exists() {
            continue;
        }
        let mut issues = Vec::new();
        let mut record_count = 0usize;
        match fs::read_to_string(&path) {
            Err(e) => {
                issues.push(format!("cannot read file: {e}"));
            }
            Ok(content) => {
                for (line_no, raw) in content.lines().enumerate() {
                    let trimmed = raw.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<serde_json::Value>(trimmed) {
                        Err(e) => {
                            issues.push(format!("invalid JSON at line {}: {e}", line_no + 1));
                            break;
                        }
                        Ok(v) => {
                            for field in *required_fields {
                                if v.get(field)
                                    .and_then(|f| f.as_str())
                                    .map(|s| s.trim().is_empty())
                                    .unwrap_or(true)
                                    && v.get(field).is_none()
                                {
                                    issues.push(format!(
                                        "missing required field '{field}' at line {}",
                                        line_no + 1
                                    ));
                                    break;
                                }
                            }
                            record_count += 1;
                        }
                    }
                }
            }
        }
        results.push(ExtensionFileResult {
            rel_path: rel.to_string(),
            record_count,
            issues,
        });
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExtensionTarget, LabelEvent, ScopeRecord};
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        tempfile::TempDir::new().unwrap()
    }

    #[test]
    fn round_trip_label_event() {
        let dir = tmp();
        let ev = LabelEvent {
            label_event_id: "label-000001".into(),
            timestamp: "2026-05-27T12:00:00Z".into(),
            actor: "tool:test".into(),
            tool: None,
            target: ExtensionTarget {
                target_type: "file".into(),
                id: "file-001".into(),
            },
            label: "restricted".into(),
            reason: Some("matched_policy".into()),
            policy_ref: None,
            provenance_ref: None,
        };
        append_label_event(dir.path(), &ev).unwrap();
        let events = read_label_events(dir.path()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].label_event_id, "label-000001");
        assert_eq!(events[0].label, "restricted");
    }

    #[test]
    fn round_trip_scope_record() {
        let dir = tmp();
        let scope = ScopeRecord {
            scope_id: "scope-000001".into(),
            created_at: "2026-05-27T12:00:00Z".into(),
            created_by: "tool:scope-manager".into(),
            description: Some("Test scope".into()),
            include: None,
            exclude: None,
            policy_refs: vec!["policy:external:test".into()],
            provenance_ref: None,
        };
        append_scope(dir.path(), &scope).unwrap();
        let scopes = read_scopes(dir.path()).unwrap();
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].scope_id, "scope-000001");
    }

    #[test]
    fn validate_extension_files_empty_case() {
        let dir = tmp();
        let results = validate_extension_files(dir.path());
        // No extension files present → empty results
        assert!(results.is_empty());
    }

    #[test]
    fn validate_extension_files_valid_label() {
        let dir = tmp();
        let ev = LabelEvent {
            label_event_id: "label-000001".into(),
            timestamp: "2026-05-27T12:00:00Z".into(),
            actor: "tool:test".into(),
            tool: None,
            target: ExtensionTarget {
                target_type: "file".into(),
                id: "file-001".into(),
            },
            label: "restricted".into(),
            reason: None,
            policy_ref: None,
            provenance_ref: None,
        };
        append_label_event(dir.path(), &ev).unwrap();
        let results = validate_extension_files(dir.path());
        assert_eq!(results.len(), 1);
        assert!(results[0].issues.is_empty(), "{:?}", results[0].issues);
        assert_eq!(results[0].record_count, 1);
    }

    #[test]
    fn read_jsonl_empty_when_missing() {
        let dir = tmp();
        let labels: Vec<LabelEvent> = read_jsonl(&labels_path(dir.path())).unwrap();
        assert!(labels.is_empty());
    }
}
