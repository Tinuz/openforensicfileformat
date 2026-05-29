use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::types::{DerivationRow, DiscoveredObjectRow, ObjectEdgeRow};

#[derive(Debug, Clone, Default)]
pub struct ObjectLineageValidationReport {
    pub missing_edge_parents: Vec<String>,
    pub missing_edge_children: Vec<String>,
    pub missing_derivation_parents: Vec<String>,
    pub missing_derivation_children: Vec<String>,
    pub invalid_derivation_links: Vec<String>,
    pub cycles: Vec<Vec<String>>,
}

impl ObjectLineageValidationReport {
    pub fn is_valid(&self) -> bool {
        self.missing_edge_parents.is_empty()
            && self.missing_edge_children.is_empty()
            && self.missing_derivation_parents.is_empty()
            && self.missing_derivation_children.is_empty()
            && self.invalid_derivation_links.is_empty()
            && self.cycles.is_empty()
    }
}

pub struct ObjectLineageValidator;

impl ObjectLineageValidator {
    pub fn validate(
        objects: &[DiscoveredObjectRow],
        edges: &[ObjectEdgeRow],
        derivations: &[DerivationRow],
    ) -> ObjectLineageValidationReport {
        let object_ids: HashSet<&str> = objects.iter().map(|o| o.object_id.as_str()).collect();
        let edge_pairs: HashSet<(&str, &str)> = edges
            .iter()
            .map(|e| (e.parent_object_id.as_str(), e.child_object_id.as_str()))
            .collect();

        let mut report = ObjectLineageValidationReport::default();

        for edge in edges {
            if !object_ids.contains(edge.parent_object_id.as_str()) {
                report.missing_edge_parents.push(edge.edge_id.clone());
            }
            if !object_ids.contains(edge.child_object_id.as_str()) {
                report.missing_edge_children.push(edge.edge_id.clone());
            }
        }

        for derivation in derivations {
            if !object_ids.contains(derivation.parent_object_id.as_str()) {
                report
                    .missing_derivation_parents
                    .push(derivation.derivation_id.clone());
            }
            if !object_ids.contains(derivation.child_object_id.as_str()) {
                report
                    .missing_derivation_children
                    .push(derivation.derivation_id.clone());
            }
            if !edge_pairs.contains(&(
                derivation.parent_object_id.as_str(),
                derivation.child_object_id.as_str(),
            )) {
                report
                    .invalid_derivation_links
                    .push(derivation.derivation_id.clone());
            }
        }

        report.cycles = detect_cycles(edges);
        report
    }
}

fn detect_cycles(edges: &[ObjectEdgeRow]) -> Vec<Vec<String>> {
    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges {
        graph
            .entry(edge.parent_object_id.as_str())
            .or_default()
            .push(edge.child_object_id.as_str());
        graph.entry(edge.child_object_id.as_str()).or_default();
    }

    let mut visited = HashSet::new();
    let mut stack = HashSet::new();
    let mut path = Vec::new();
    let mut cycles = Vec::new();

    for node in graph.keys().copied().collect::<Vec<_>>() {
        if !visited.contains(node) {
            dfs(
                node,
                &graph,
                &mut visited,
                &mut stack,
                &mut path,
                &mut cycles,
            );
        }
    }

    cycles
}

fn dfs<'a>(
    node: &'a str,
    graph: &HashMap<&'a str, Vec<&'a str>>,
    visited: &mut HashSet<&'a str>,
    stack: &mut HashSet<&'a str>,
    path: &mut Vec<&'a str>,
    cycles: &mut Vec<Vec<String>>,
) {
    visited.insert(node);
    stack.insert(node);
    path.push(node);

    if let Some(children) = graph.get(node) {
        for &child in children {
            if !visited.contains(child) {
                dfs(child, graph, visited, stack, path, cycles);
            } else if stack.contains(child) {
                if let Some(pos) = path.iter().position(|&n| n == child) {
                    cycles.push(path[pos..].iter().map(|s| (*s).to_string()).collect());
                }
            }
        }
    }

    stack.remove(node);
    let _ = path.pop();
}

// ── Lineage statistics and export ────────────────────────────────────────────

/// Summary statistics derived from the object graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageStats {
    /// Total number of objects in the index.
    pub object_count: usize,
    /// Total number of directed edges between objects.
    pub edge_count: usize,
    /// Total number of derivation records.
    pub derivation_count: usize,
    /// IDs of objects that have no incoming edges (source objects).
    pub root_object_ids: Vec<String>,
    /// IDs of objects that have no outgoing edges (terminal objects).
    pub leaf_object_ids: Vec<String>,
    /// Length of the longest directed path from any root, measured in edges.
    pub max_depth: usize,
}

/// Compute [`LineageStats`] from the three index tables.
///
/// `max_depth` is calculated with a BFS from all root nodes simultaneously.
/// If the graph contains cycles the BFS terminates anyway because each node
/// is visited at most once.
pub fn compute_lineage_stats(
    objects: &[DiscoveredObjectRow],
    edges: &[ObjectEdgeRow],
    derivations: &[DerivationRow],
) -> LineageStats {
    let all_ids: HashSet<&str> = objects.iter().map(|o| o.object_id.as_str()).collect();

    // Build adjacency list: parent → children
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut has_incoming: HashSet<&str> = HashSet::new();
    for edge in edges {
        children
            .entry(edge.parent_object_id.as_str())
            .or_default()
            .push(edge.child_object_id.as_str());
        has_incoming.insert(edge.child_object_id.as_str());
    }

    // Root objects: known objects with no incoming edge
    let root_object_ids: Vec<String> = all_ids
        .iter()
        .filter(|id| !has_incoming.contains(*id))
        .map(|id| id.to_string())
        .collect();

    // Leaf objects: known objects with no outgoing edge
    let leaf_object_ids: Vec<String> = all_ids
        .iter()
        .filter(|id| !children.contains_key(*id))
        .map(|id| id.to_string())
        .collect();

    // BFS from all roots to compute max depth (cycle-safe: visited set)
    let mut max_depth = 0usize;
    let mut visited: HashSet<&str> = HashSet::new();
    // queue holds (node, depth)
    let mut queue: std::collections::VecDeque<(&str, usize)> = root_object_ids
        .iter()
        .map(|id| (id.as_str(), 0usize))
        .collect();
    while let Some((node, depth)) = queue.pop_front() {
        if visited.contains(node) {
            continue;
        }
        visited.insert(node);
        if depth > max_depth {
            max_depth = depth;
        }
        if let Some(kids) = children.get(node) {
            for &kid in kids {
                if !visited.contains(kid) {
                    queue.push_back((kid, depth + 1));
                }
            }
        }
    }

    LineageStats {
        object_count: objects.len(),
        edge_count: edges.len(),
        derivation_count: derivations.len(),
        root_object_ids,
        leaf_object_ids,
        max_depth,
    }
}

/// Write the object graph to `writer` in [Graphviz DOT format].
///
/// Each object becomes a node labelled with its ID, name, and object type.
/// Each edge becomes a directed arc labelled with the relation type.
///
/// [Graphviz DOT format]: https://graphviz.org/doc/info/lang.html
pub fn export_dot(
    objects: &[DiscoveredObjectRow],
    edges: &[ObjectEdgeRow],
    writer: &mut impl std::io::Write,
) -> std::io::Result<()> {
    writeln!(writer, "digraph object_lineage {{")?;
    writeln!(writer, "    graph [rankdir=LR];")?;
    writeln!(writer, "    node  [shape=box, fontname=\"monospace\"];")?;
    for obj in objects {
        let label = format!(
            "{}\\n{}\\n{}",
            escape_dot(&obj.object_id),
            escape_dot(obj.name.as_deref().unwrap_or("")),
            escape_dot(&obj.object_type),
        );
        writeln!(
            writer,
            "    \"{}\" [label=\"{}\"];",
            escape_dot(&obj.object_id),
            label
        )?;
    }
    for edge in edges {
        writeln!(
            writer,
            "    \"{}\" -> \"{}\" [label=\"{}\"];",
            escape_dot(&edge.parent_object_id),
            escape_dot(&edge.child_object_id),
            escape_dot(&edge.relation_type),
        )?;
    }
    writeln!(writer, "}}")?;
    Ok(())
}

fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Serialise the full lineage dataset to a self-contained JSON value.
///
/// The returned object includes a UTC timestamp, basic statistics, validation
/// results, and the raw row arrays so that consumers can reconstruct the graph
/// without reading Parquet files.
pub fn export_lineage_json(
    container_id: &str,
    objects: &[DiscoveredObjectRow],
    edges: &[ObjectEdgeRow],
    derivations: &[DerivationRow],
) -> serde_json::Value {
    let stats = compute_lineage_stats(objects, edges, derivations);
    let validation = ObjectLineageValidator::validate(objects, edges, derivations);
    serde_json::json!({
        "generated_at": Utc::now().to_rfc3339(),
        "container_id": container_id,
        "stats": stats,
        "valid": validation.is_valid(),
        "validation": {
            "missing_edge_parents":       validation.missing_edge_parents,
            "missing_edge_children":      validation.missing_edge_children,
            "missing_derivation_parents": validation.missing_derivation_parents,
            "missing_derivation_children":validation.missing_derivation_children,
            "invalid_derivation_links":   validation.invalid_derivation_links,
            "cycles":                     validation.cycles,
        },
        "objects":     serde_json::to_value(objects).unwrap_or(serde_json::Value::Null),
        "edges":       serde_json::to_value(edges).unwrap_or(serde_json::Value::Null),
        "derivations": serde_json::to_value(derivations).unwrap_or(serde_json::Value::Null),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(id: &str) -> DiscoveredObjectRow {
        DiscoveredObjectRow {
            object_id: id.to_string(),
            object_type: "object".to_string(),
            name: None,
            logical_path: None,
            media_type: None,
            size_bytes: None,
            sha256: None,
            source_layer: "analysis".to_string(),
            storage_ref: None,
            content_ref: None,
            content_hash_status: None,
            root_source_ref: None,
            root_id: None,
            collection_relative_path: None,
            original_created_at: None,
            original_modified_at: None,
            original_accessed_at: None,
            created_by_job_id: None,
            parser_status: "success".to_string(),
            provenance_ref: None,
            schema_version: "0.2.0".to_string(),
        }
    }

    #[test]
    fn reports_missing_references() {
        let objects = vec![obj("obj-a")];
        let edges = vec![ObjectEdgeRow {
            edge_id: "edge-1".to_string(),
            parent_object_id: "obj-a".to_string(),
            child_object_id: "obj-b".to_string(),
            relation_type: "contains".to_string(),
            method: None,
            logical_path: None,
            sequence: None,
            created_by_job_id: None,
            provenance_ref: None,
            schema_version: "0.2.0".to_string(),
        }];
        let derivations = vec![DerivationRow {
            derivation_id: "drv-1".to_string(),
            parent_object_id: "obj-a".to_string(),
            child_object_id: "obj-b".to_string(),
            job_id: "job-1".to_string(),
            method: "content_extraction".to_string(),
            tool_id: "tool-a".to_string(),
            tool_name: "Tool A".to_string(),
            tool_version: "1.0.0".to_string(),
            parameters_hash: None,
            input_sha256: None,
            output_sha256: None,
            storage_mode: "referenced_only".to_string(),
            provenance_ref: None,
            created_at: "2026-05-25T00:00:00Z".to_string(),
            schema_version: "0.2.0".to_string(),
        }];

        let report = ObjectLineageValidator::validate(&objects, &edges, &derivations);
        assert_eq!(report.missing_edge_children, vec!["edge-1".to_string()]);
        assert_eq!(
            report.missing_derivation_children,
            vec!["drv-1".to_string()]
        );
    }

    #[test]
    fn detects_cycle() {
        let objects = vec![obj("a"), obj("b")];
        let edges = vec![
            ObjectEdgeRow {
                edge_id: "e1".to_string(),
                parent_object_id: "a".to_string(),
                child_object_id: "b".to_string(),
                relation_type: "contains".to_string(),
                method: None,
                logical_path: None,
                sequence: None,
                created_by_job_id: None,
                provenance_ref: None,
                schema_version: "0.2.0".to_string(),
            },
            ObjectEdgeRow {
                edge_id: "e2".to_string(),
                parent_object_id: "b".to_string(),
                child_object_id: "a".to_string(),
                relation_type: "contains".to_string(),
                method: None,
                logical_path: None,
                sequence: None,
                created_by_job_id: None,
                provenance_ref: None,
                schema_version: "0.2.0".to_string(),
            },
        ];

        let report = ObjectLineageValidator::validate(&objects, &edges, &[]);
        assert!(!report.cycles.is_empty());
    }
}
