use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use serde_json::json;

use offf_core::{
    provenance::ProvenanceWriter,
    storage::ContainerRef,
    types::{AnnotationEvent, AnnotationTarget},
};

const TOOL_NAME: &str = "offf-annotate";
const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(
    name = "offf-annotate",
    about = "Append-only annotation tooling for OFFF analysis layer",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Add a human annotation event
    AddHuman {
        /// OFFF container path or URI (local path or s3://bucket/prefix)
        #[arg(long)]
        case: String,
        /// Analyst identifier
        #[arg(long)]
        actor: String,
        /// Annotation type (default: relevance_label)
        #[arg(long, default_value = "relevance_label")]
        annotation_type: String,
        /// Label value (e.g. relevant, irrelevant, needs_review)
        #[arg(long)]
        label: String,
        /// Optional human comment
        #[arg(long)]
        comment: Option<String>,
        #[arg(long)]
        file_id: Option<String>,
        #[arg(long)]
        chunk_id: Option<String>,
        #[arg(long)]
        artifact_id: Option<String>,
    },
    /// Add an AI annotation event
    AddAi {
        /// OFFF container path or URI (local path or s3://bucket/prefix)
        #[arg(long)]
        case: String,
        #[arg(long)]
        model_name: String,
        #[arg(long)]
        model_version: String,
        #[arg(long)]
        model_hash: String,
        #[arg(long)]
        classification: String,
        #[arg(long)]
        confidence: f64,
        #[arg(long)]
        input_scope: String,
        #[arg(long)]
        comment: Option<String>,
        #[arg(long)]
        file_id: Option<String>,
        #[arg(long)]
        chunk_id: Option<String>,
        #[arg(long)]
        artifact_id: Option<String>,
    },
    /// Add a correction event for a previous annotation
    Correct {
        /// OFFF container path or URI (local path or s3://bucket/prefix)
        #[arg(long)]
        case: String,
        /// Analyst identifier
        #[arg(long)]
        actor: String,
        /// Previous annotation id being corrected
        #[arg(long)]
        correction_of: String,
        /// Correction explanation
        #[arg(long)]
        comment: String,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        classification: Option<String>,
        #[arg(long)]
        file_id: Option<String>,
        #[arg(long)]
        chunk_id: Option<String>,
        #[arg(long)]
        artifact_id: Option<String>,
    },
    /// List latest annotation events
    List {
        /// OFFF container path or URI (local path or s3://bucket/prefix)
        #[arg(long)]
        case: String,
        /// Maximum number of events to print
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::AddHuman {
            case,
            actor,
            annotation_type,
            label,
            comment,
            file_id,
            chunk_id,
            artifact_id,
        } => {
            let container = ContainerRef::parse(&case)?;
            let target = AnnotationTarget {
                file_id,
                chunk_id,
                artifact_id,
            };
            require_target(&target)?;

            let event = AnnotationEvent {
                annotation_id: format!("ann-{}", uuid::Uuid::new_v4()),
                timestamp: Utc::now().to_rfc3339(),
                actor: actor.clone(),
                origin: "human".to_string(),
                annotation_type,
                target,
                label: Some(label),
                comment,
                classification: None,
                confidence: None,
                input_scope: None,
                model_name: None,
                model_version: None,
                model_hash: None,
                correction_of: None,
            };

            append_annotation(&container, &event)?;
            append_provenance(
                &container,
                "annotation_added",
                &actor,
                json!({
                    "annotation_id": event.annotation_id,
                    "origin": event.origin,
                    "annotation_type": event.annotation_type,
                }),
            )?;

            println!("Added annotation: {}", event.annotation_id);
        }
        Command::AddAi {
            case,
            model_name,
            model_version,
            model_hash,
            classification,
            confidence,
            input_scope,
            comment,
            file_id,
            chunk_id,
            artifact_id,
        } => {
            let container = ContainerRef::parse(&case)?;
            let target = AnnotationTarget {
                file_id,
                chunk_id,
                artifact_id,
            };
            require_target(&target)?;

            let actor = format!("model:{model_name}");
            let event = AnnotationEvent {
                annotation_id: format!("ai-{}", uuid::Uuid::new_v4()),
                timestamp: Utc::now().to_rfc3339(),
                actor: actor.clone(),
                origin: "ai".to_string(),
                annotation_type: "classification".to_string(),
                target,
                label: None,
                comment,
                classification: Some(classification),
                confidence: Some(confidence),
                input_scope: Some(input_scope),
                model_name: Some(model_name),
                model_version: Some(model_version),
                model_hash: Some(model_hash),
                correction_of: None,
            };

            append_annotation(&container, &event)?;
            append_provenance(
                &container,
                "annotation_added",
                &actor,
                json!({
                    "annotation_id": event.annotation_id,
                    "origin": event.origin,
                    "annotation_type": event.annotation_type,
                    "model_name": event.model_name,
                    "model_version": event.model_version,
                }),
            )?;

            println!("Added annotation: {}", event.annotation_id);
        }
        Command::Correct {
            case,
            actor,
            correction_of,
            comment,
            label,
            classification,
            file_id,
            chunk_id,
            artifact_id,
        } => {
            let container = ContainerRef::parse(&case)?;
            let target = AnnotationTarget {
                file_id,
                chunk_id,
                artifact_id,
            };
            require_target(&target)?;

            let event = AnnotationEvent {
                annotation_id: format!("ann-{}", uuid::Uuid::new_v4()),
                timestamp: Utc::now().to_rfc3339(),
                actor: actor.clone(),
                origin: "human".to_string(),
                annotation_type: "correction".to_string(),
                target,
                label,
                comment: Some(comment),
                classification,
                confidence: None,
                input_scope: None,
                model_name: None,
                model_version: None,
                model_hash: None,
                correction_of: Some(correction_of.clone()),
            };

            append_annotation(&container, &event)?;
            append_provenance(
                &container,
                "annotation_corrected",
                &actor,
                json!({
                    "annotation_id": event.annotation_id,
                    "correction_of": correction_of,
                }),
            )?;

            println!("Added correction: {}", event.annotation_id);
        }
        Command::List { case, limit } => {
            let container = ContainerRef::parse(&case)?;
            let content = container
                .read_text("analysis/annotations.jsonl")
                .context("analysis/annotations.jsonl not found")?;

            let mut rows: Vec<AnnotationEvent> = Vec::new();
            for line in content.lines().filter(|l| !l.trim().is_empty()) {
                if let Ok(evt) = serde_json::from_str::<AnnotationEvent>(line) {
                    rows.push(evt);
                }
            }

            println!(
                "Annotations: {} total, showing last {}",
                rows.len(),
                limit.min(rows.len())
            );
            for evt in rows.iter().rev().take(limit).rev() {
                println!(
                    "{} | {} | {} | {}",
                    evt.annotation_id, evt.timestamp, evt.origin, evt.annotation_type
                );
            }
        }
    }

    Ok(())
}

fn require_target(target: &AnnotationTarget) -> Result<()> {
    if target.file_id.is_none() && target.chunk_id.is_none() && target.artifact_id.is_none() {
        anyhow::bail!("at least one target must be set: --file-id, --chunk-id or --artifact-id");
    }
    Ok(())
}

fn append_annotation(container: &ContainerRef, event: &AnnotationEvent) -> Result<()> {
    let line = serde_json::to_string(event)?;
    container.append_jsonl_line("analysis/annotations.jsonl", &line)?;
    Ok(())
}

fn append_provenance(
    container: &ContainerRef,
    action: &str,
    actor: &str,
    details: serde_json::Value,
) -> Result<()> {
    let rel = "provenance/chain_of_custody.jsonl";
    match container {
        ContainerRef::Local(base) => {
            let mut prov = ProvenanceWriter::new(&base.join(rel))?;
            prov.record(action, TOOL_NAME, TOOL_VERSION, actor, details)?;
            Ok(())
        }
        ContainerRef::S3 { .. } => {
            let counter = if container.exists(rel)? {
                container
                    .read_text(rel)?
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .count() as u64
            } else {
                0
            };
            let event = json!({
                "event_id": format!("evt-{counter:06}"),
                "timestamp": Utc::now().to_rfc3339(),
                "actor": actor,
                "action": action,
                "tool": {
                    "name": TOOL_NAME,
                    "version": TOOL_VERSION,
                },
                "details": details,
            });
            let line = serde_json::to_string(&event)?;
            container.append_jsonl_line(rel, &line)?;
            Ok(())
        }
    }
}
