/// ScopeResolver — translates a `JobManifest` input_scope into a deterministic
/// list of `AnalysisInputObject`s from a container's object index.
///
/// The canonical sort order is `object_id` ascending. The same scope on the
/// same container always produces the same list in the same order.
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::{
    error::OfffError,
    parquet_io::for_each_object_batch,
    types::{
        AnalysisInputObject, InputObjectMetadata, InputSourceRefs, JobManifest,
    },
};

/// Resolve the analysis scope of `job` against the object index at
/// `container_path/indexes/objects/object_index.parquet`.
///
/// Returns a deterministically sorted `Vec<AnalysisInputObject>` (sorted by
/// `object_id` ascending).  Returns an empty vec when the index does not exist.
pub fn resolve_analysis_scope(
    container_path: &Path,
    job: &JobManifest,
) -> Result<Vec<AnalysisInputObject>, OfffError> {
    let index_path = container_path.join("indexes/objects/object_index.parquet");
    if !index_path.exists() {
        return Ok(Vec::new());
    }

    let input_scope = job.input_scope.as_ref();

    // Extract filter criteria once, before the closure borrows `input_scope`.
    let object_types: Vec<String> = input_scope
        .and_then(|s| s.include.as_ref())
        .map(|inc| inc.object_types.clone())
        .unwrap_or_default();

    let extensions: Vec<String> = input_scope
        .and_then(|s| s.include.as_ref())
        .map(|inc| {
            inc.extensions
                .iter()
                .map(|e| e.trim_start_matches('.').to_lowercase())
                .collect()
        })
        .unwrap_or_default();

    let media_types: Vec<String> = input_scope
        .and_then(|s| s.include.as_ref())
        .map(|inc| inc.media_types.clone())
        .unwrap_or_default();

    let parser_statuses: Vec<String> = input_scope
        .and_then(|s| s.include.as_ref())
        .map(|inc| inc.parser_statuses.clone())
        .unwrap_or_default();

    let max_size: Option<u64> = input_scope
        .and_then(|s| s.limits.as_ref())
        .and_then(|l| l.max_object_size_bytes);

    let min_size: Option<u64> = input_scope
        .and_then(|s| s.limits.as_ref())
        .and_then(|l| l.min_object_size_bytes);

    let exclude_labels: Vec<String> = input_scope
        .map(|s| s.exclude.labels.clone())
        .unwrap_or_default();

    // Explicit object_id allow-list (empty = no restriction).
    let allowed_ids: Vec<String> = input_scope
        .map(|s| s.selectors.object_ids.clone())
        .unwrap_or_default();

    let mut results: Vec<AnalysisInputObject> = Vec::new();
    let mut counter: u64 = 0;

    for_each_object_batch(&index_path, 1024, |batch| {
        for row in batch {
            // ── allow-list filter ────────────────────────────────────────────
            if !allowed_ids.is_empty() && !allowed_ids.contains(&row.object_id) {
                continue;
            }

            // ── object_type filter ───────────────────────────────────────────
            if !object_types.is_empty() && !object_types.contains(&row.object_type) {
                continue;
            }

            // ── extension filter ─────────────────────────────────────────────
            if !extensions.is_empty() {
                let ext = extract_extension(row.name.as_deref(), row.logical_path.as_deref());
                if !extensions.contains(&ext) {
                    continue;
                }
            }

            // ── media_type filter ────────────────────────────────────────────
            if !media_types.is_empty() {
                let mt = row.media_type.as_deref().unwrap_or("");
                if !media_types.iter().any(|m| m == mt) {
                    continue;
                }
            }

            // ── parser_status filter ─────────────────────────────────────────
            if !parser_statuses.is_empty() && !parser_statuses.contains(&row.parser_status) {
                continue;
            }

            // ── size filters ─────────────────────────────────────────────────
            if let Some(max) = max_size {
                if row.size_bytes.unwrap_or(0) > max {
                    continue;
                }
            }
            if let Some(min) = min_size {
                if row.size_bytes.unwrap_or(0) < min {
                    continue;
                }
            }

            // ── exclude label filter (placeholder — full label store TBD) ────
            // For now: skip objects whose object_type appears in exclude_labels
            // (a full implementation would read extensions/labels/labels.jsonl).
            if !exclude_labels.is_empty() {
                // We don't store labels on the row directly; skip this check
                // for now and leave a hook for future label-index integration.
                let _ = &exclude_labels;
            }

            counter += 1;
            let input_id = format!("input-{counter:06}");

            // Derive file extension for metadata.
            let ext_meta =
                extract_extension_opt(row.name.as_deref(), row.logical_path.as_deref());

            results.push(AnalysisInputObject {
                input_id,
                input_type: object_type_to_input_type(&row.object_type),
                object_id: row.object_id.clone(),
                source_refs: InputSourceRefs {
                    root_id: row.root_id.clone(),
                    sha256: row.sha256.clone(),
                    storage_ref: row.storage_ref.clone(),
                },
                metadata: InputObjectMetadata {
                    name: row.name.clone(),
                    extension: ext_meta,
                    size_bytes: row.size_bytes,
                    media_type: row.media_type.clone(),
                },
            });
        }
        Ok(())
    })?;

    // Canonical sort: object_id ascending.
    results.sort_by(|a, b| a.object_id.cmp(&b.object_id));

    // Re-assign input_ids after sort so they match position order.
    for (i, obj) in results.iter_mut().enumerate() {
        obj.input_id = format!("input-{:06}", i + 1);
    }

    Ok(results)
}

/// Compute the `input_scope_hash` for a list of input objects.
///
/// Hash = SHA-256 of the newline-joined `object_id`s, in the order they appear
/// in `inputs` (the caller is responsible for deterministic ordering).
pub fn compute_input_scope_hash(inputs: &[AnalysisInputObject]) -> String {
    let mut hasher = Sha256::new();
    for (i, obj) in inputs.iter().enumerate() {
        if i > 0 {
            hasher.update(b"\n");
        }
        hasher.update(obj.object_id.as_bytes());
    }
    let hash = hasher.finalize();
    format!("sha256:{}", hex::encode(hash))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract lowercase extension from `name` or `logical_path`, stripping the dot.
/// Returns an empty string when no extension can be derived.
fn extract_extension(name: Option<&str>, path: Option<&str>) -> String {
    extract_extension_opt(name, path).unwrap_or_default()
}

fn extract_extension_opt(name: Option<&str>, path: Option<&str>) -> Option<String> {
    let s = name.or(path)?;
    let fname = std::path::Path::new(s).file_name()?.to_str()?;
    let dot_pos = fname.rfind('.')?;
    if dot_pos == 0 {
        return None; // dotfile with no extension
    }
    Some(fname[dot_pos + 1..].to_lowercase())
}

/// Map an OFFF `object_type` to the `input_type` vocabulary used in
/// `AnalysisInputObject`.
fn object_type_to_input_type(object_type: &str) -> String {
    match object_type {
        "evidence_file" => "evidence_file",
        "derived_object" => "derived_object",
        "artifact" => "artifact",
        "file" => "file",
        _ => "object",
    }
    .to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AnalysisInputObject;

    fn make_input(object_id: &str) -> AnalysisInputObject {
        AnalysisInputObject {
            input_id: String::new(),
            input_type: "object".into(),
            object_id: object_id.to_string(),
            source_refs: InputSourceRefs {
                root_id: None,
                sha256: None,
                storage_ref: None,
            },
            metadata: InputObjectMetadata {
                name: None,
                extension: None,
                size_bytes: None,
                media_type: None,
            },
        }
    }

    #[test]
    fn scope_hash_stability() {
        let inputs: Vec<AnalysisInputObject> = ["obj-c", "obj-a", "obj-b"]
            .iter()
            .map(|id| make_input(id))
            .collect();
        let h1 = compute_input_scope_hash(&inputs);
        let h2 = compute_input_scope_hash(&inputs);
        assert_eq!(h1, h2);
        assert!(h1.starts_with("sha256:"));
    }

    #[test]
    fn scope_hash_order_sensitive() {
        let a = vec![make_input("obj-a"), make_input("obj-b")];
        let b = vec![make_input("obj-b"), make_input("obj-a")];
        let ha = compute_input_scope_hash(&a);
        let hb = compute_input_scope_hash(&b);
        // Different order → different hash.
        assert_ne!(ha, hb);
    }

    #[test]
    fn extract_extension_cases() {
        assert_eq!(extract_extension(Some("contract.docx"), None), "docx");
        assert_eq!(extract_extension(Some("image.JPEG"), None), "jpeg");
        assert_eq!(extract_extension(None, Some("/path/to/file.pdf")), "pdf");
        assert_eq!(extract_extension(Some(".gitignore"), None), "");
        assert_eq!(extract_extension(Some("noext"), None), "");
    }
}
