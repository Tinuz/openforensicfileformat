use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{error::OfffError, types::ToolInfo};

// ── Event types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceEvent {
    pub event_id: String,
    pub timestamp: String,
    pub actor: String,
    pub action: String,
    pub tool: ToolInfo,
    pub details: serde_json::Value,
}

// ── Writer ────────────────────────────────────────────────────────────────────

/// Append-only JSONL writer for provenance events.
///
/// Each `append` call immediately flushes and syncs to disk.
pub struct ProvenanceWriter {
    path: PathBuf,
    counter: u64,
}

impl ProvenanceWriter {
    /// Open (or create) the provenance log at `path`.
    pub fn new(path: &Path) -> Result<Self, OfffError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Ensure the file exists
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        // Count existing events so we generate unique IDs
        let counter = if path.exists() {
            let content = fs::read_to_string(path)?;
            content.lines().filter(|l| !l.trim().is_empty()).count() as u64
        } else {
            0
        };

        Ok(Self {
            path: path.to_path_buf(),
            counter,
        })
    }

    /// Append an event and immediately sync to disk.
    pub fn append(&mut self, event: ProvenanceEvent) -> Result<(), OfffError> {
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        let line = serde_json::to_string(&event)?;
        writeln!(file, "{line}")?;
        file.flush()?;
        file.sync_all()?;
        self.counter += 1;
        Ok(())
    }

    /// Build and append a structured event in one call.
    pub fn record(
        &mut self,
        action: &str,
        tool_name: &str,
        tool_version: &str,
        actor: &str,
        details: serde_json::Value,
    ) -> Result<(), OfffError> {
        self.record_at(
            action,
            tool_name,
            tool_version,
            actor,
            details,
            Utc::now().to_rfc3339(),
        )
    }

    /// Build and append a structured event with an explicit timestamp.
    pub fn record_at(
        &mut self,
        action: &str,
        tool_name: &str,
        tool_version: &str,
        actor: &str,
        details: serde_json::Value,
        timestamp: String,
    ) -> Result<(), OfffError> {
        let event = ProvenanceEvent {
            event_id: format!("evt-{:06}", self.counter),
            timestamp,
            actor: actor.to_string(),
            action: action.to_string(),
            tool: ToolInfo {
                name: tool_name.to_string(),
                version: tool_version.to_string(),
            },
            details,
        };
        self.append(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn append_creates_and_reads_back() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("provenance").join("events.jsonl");

        let mut w = ProvenanceWriter::new(&path).unwrap();
        w.record(
            "test_action",
            "offf-test",
            "0.1.0",
            "system",
            serde_json::json!({"key": "value"}),
        )
        .unwrap();
        w.record(
            "second_action",
            "offf-test",
            "0.1.0",
            "analyst",
            serde_json::json!({}),
        )
        .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let evt: ProvenanceEvent = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(evt.action, "test_action");
        assert_eq!(evt.event_id, "evt-000000");

        let evt2: ProvenanceEvent = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(evt2.event_id, "evt-000001");
    }

    #[test]
    fn is_append_only() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log.jsonl");

        {
            let mut w = ProvenanceWriter::new(&path).unwrap();
            w.record("first", "t", "0.0.0", "s", serde_json::json!({}))
                .unwrap();
        }

        // Re-open and add second event; first must still be present
        {
            let mut w = ProvenanceWriter::new(&path).unwrap();
            w.record("second", "t", "0.0.0", "s", serde_json::json!({}))
                .unwrap();
        }

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content.lines().count(), 2);
    }
}
