use std::collections::{HashMap, HashSet};

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
            root_source_ref: None,
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
