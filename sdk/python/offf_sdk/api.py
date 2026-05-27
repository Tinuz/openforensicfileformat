from __future__ import annotations

import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import TYPE_CHECKING, Any, Iterable

if TYPE_CHECKING:
    from .container import OfffContainer
    from .types import ChunkRecord, ProvenanceEvent


def _container_cls():
    from .container import OfffContainer

    return OfffContainer


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def open_container(container_path: str | Path) -> OfffContainer:
    return _container_cls()(container_path)


def read_manifest(container: OfffContainer) -> dict[str, Any]:
    return container.read_manifest()


def verify_container(container: OfffContainer) -> dict[str, bool]:
    return container.verify_container()


def read_chunk(container: OfffContainer, chunk_ref: int | str | ChunkRecord, verify: bool = True) -> bytes:
    return container.read_chunk(chunk_ref, verify=verify)


def verify_chunk(container: OfffContainer, chunk_ref: int | str | ChunkRecord) -> bool:
    return container.verify_chunk(chunk_ref)


def map_offset_to_chunk(container: OfffContainer, source_offset: int) -> tuple[ChunkRecord, int]:
    return container.map_offset_to_chunk(source_offset)


def read_file_index(container: OfffContainer, partition_id: str | None = None) -> list[dict[str, Any]]:
    return container.read_file_index(partition_id=partition_id)


def write_analysis_result(container: OfffContainer, relative_path: str, rows: list[dict[str, Any]]) -> Path:
    return container.write_analysis_result(relative_path, rows)


def append_provenance_event(
    container: OfffContainer,
    action: str,
    actor: str,
    details: dict[str, Any],
    tool_name: str = "offf-sdk-python",
    tool_version: str = "0.1.0",
) -> ProvenanceEvent:
    return container.append_provenance_event(
        action=action,
        actor=actor,
        details=details,
        tool_name=tool_name,
        tool_version=tool_version,
    )


# ── Object-producing worker convenience functions (Sprint 11) ─────────────────

def write_object_delta(
    container: OfffContainer, job_id: str, rows: list[dict[str, Any]]
) -> Path:
    return container.write_object_delta(job_id, rows)


def write_edge_delta(
    container: OfffContainer, job_id: str, rows: list[dict[str, Any]]
) -> Path:
    return container.write_edge_delta(job_id, rows)


def write_derivation_delta(
    container: OfffContainer, job_id: str, rows: list[dict[str, Any]]
) -> Path:
    return container.write_derivation_delta(job_id, rows)


def materialize_derived_object(container: OfffContainer, data: bytes) -> str:
    return container.materialize_derived_object(data)


def read_objects(container: OfffContainer) -> list[dict[str, Any]]:
    return container.read_objects()


def read_object_edges(container: OfffContainer) -> list[dict[str, Any]]:
    return container.read_object_edges()


def read_derivations(container: OfffContainer) -> list[dict[str, Any]]:
    return container.read_derivations()


def read_derived_object(container: OfffContainer, sha256: str) -> bytes:
    return container.read_derived_object(sha256)


# ── Path-based worker contract helpers (SDK-first demo and workers) ──────────

def read_json_file(path: str | Path) -> dict[str, Any]:
    p = Path(path)
    with p.open("r", encoding="utf-8") as f:
        return json.load(f)


def write_json_file(path: str | Path, data: dict[str, Any]) -> None:
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    with p.open("w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


def read_jsonl_file(path: str | Path) -> list[dict[str, Any]]:
    p = Path(path)
    if not p.exists():
        return []
    rows: list[dict[str, Any]] = []
    with p.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            rows.append(json.loads(line))
    return rows


def append_jsonl_file(path: str | Path, row: dict[str, Any]) -> None:
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    with p.open("a", encoding="utf-8") as f:
        f.write(json.dumps(row, ensure_ascii=False) + "\n")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: str | Path) -> str:
    p = Path(path)
    h = hashlib.sha256()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def ensure_job_output_dir(case_path: str | Path, job_id: str) -> Path:
    out = Path(case_path) / "analysis" / "jobs" / job_id
    out.mkdir(parents=True, exist_ok=True)
    return out


def write_result_manifest_file(
    case_path: str | Path,
    job_id: str,
    manifest: dict[str, Any],
    force: bool = False,
) -> Path:
    out_dir = ensure_job_output_dir(case_path, job_id)
    manifest_path = out_dir / "result_manifest.json"
    if manifest_path.exists() and not force:
        raise FileExistsError(
            f"result_manifest.json already exists for job {job_id}; use force=True to overwrite"
        )
    write_json_file(manifest_path, manifest)
    return manifest_path


def append_provenance_event_row(case_path: str | Path, event: dict[str, Any]) -> None:
    append_jsonl_file(Path(case_path) / "provenance" / "chain_of_custody.jsonl", event)


def load_object_index(case_path: str | Path) -> list[dict[str, Any]]:
    return read_jsonl_file(Path(case_path) / "indexes" / "objects" / "object_index.jsonl")


def _matches_extension(name: str | None, allowed: Iterable[str]) -> bool:
    if not name:
        return False
    suffix = Path(name).suffix.lower().lstrip(".")
    return suffix in {x.lower() for x in allowed}


def resolve_scope(
    object_index: list[dict[str, Any]],
    scope: dict[str, Any] | None,
) -> list[dict[str, Any]]:
    if not scope:
        return object_index

    selectors = scope.get("selectors", {})
    excludes = scope.get("exclude", {})
    limits = scope.get("limits", {})

    allow_types = {x.lower() for x in selectors.get("object_types", [])}
    allow_exts = {x.lower() for x in selectors.get("extensions", [])}
    deny_labels = {x.lower() for x in excludes.get("labels", [])}
    max_size = limits.get("max_object_size_bytes")

    selected: list[dict[str, Any]] = []
    for row in object_index:
        obj_type = str(row.get("object_type", "")).lower()
        if allow_types and obj_type not in allow_types:
            continue

        name = row.get("name")
        if allow_exts and not _matches_extension(name, allow_exts):
            continue

        labels = {str(x).lower() for x in row.get("labels", [])}
        if deny_labels.intersection(labels):
            continue

        size_bytes = row.get("size_bytes")
        if max_size is not None and isinstance(size_bytes, int) and size_bytes > int(max_size):
            continue

        selected.append(row)

    return selected


def build_result_manifest(
    *,
    job_id: str,
    task: str,
    worker: dict[str, Any],
    output_dir: Path,
    output_artifacts: Iterable[str],
    statistics: dict[str, Any],
    status: str,
    input_ref: dict[str, Any] | None = None,
    started_at: str | None = None,
) -> dict[str, Any]:
    artifacts: list[dict[str, Any]] = []
    for rel in output_artifacts:
        p = output_dir / rel
        if not p.exists():
            continue
        artifacts.append(
            {
                "path": rel,
                "sha256": f"sha256:{sha256_file(p)}",
                "size_bytes": p.stat().st_size,
            }
        )

    return {
        "job_id": job_id,
        "task": task,
        "worker": worker,
        "input": input_ref or {},
        "output_artifacts": artifacts,
        "statistics": statistics,
        "created_at": started_at or utc_now_iso(),
        "completed_at": utc_now_iso(),
        "status": status,
    }


def build_job_completed_event(
    *,
    job_id: str,
    tool_id: str,
    tool_name: str,
    tool_version: str,
    status: str,
    result_manifest_path: str,
) -> dict[str, Any]:
    return {
        "event_id": f"evt-{job_id}",
        "timestamp": utc_now_iso(),
        "actor": f"worker:{tool_id}",
        "action": "analysis_job_completed",
        "tool": {
            "tool_id": tool_id,
            "name": tool_name,
            "version": tool_version,
        },
        "details": {
            "job_id": job_id,
            "result_manifest": result_manifest_path,
            "status": status,
        },
    }


# ── Generic extension point helpers (Sprint 15) ───────────────────────────────
#
# Standard paths:
#   extensions/labels/labels.jsonl
#   extensions/scopes/scopes.jsonl
#   extensions/sets/working_sets.jsonl
#   extensions/sets/release_sets.jsonl
#   extensions/sets/exclusion_sets.jsonl
#   extensions/decisions/decisions.jsonl
#   extensions/policies/policy_refs.jsonl
#   extensions/access/access_events.jsonl
#   extensions/access/denied_access_events.jsonl
#   extensions/audit/audit_events.jsonl

_EXT_PATHS: dict[str, str] = {
    "labels": "extensions/labels/labels.jsonl",
    "scopes": "extensions/scopes/scopes.jsonl",
    "working_sets": "extensions/sets/working_sets.jsonl",
    "release_sets": "extensions/sets/release_sets.jsonl",
    "exclusion_sets": "extensions/sets/exclusion_sets.jsonl",
    "decisions": "extensions/decisions/decisions.jsonl",
    "policy_refs": "extensions/policies/policy_refs.jsonl",
    "access_events": "extensions/access/access_events.jsonl",
    "denied_access_events": "extensions/access/denied_access_events.jsonl",
    "audit_events": "extensions/audit/audit_events.jsonl",
}


def _ext_path(case_path: str | Path, key: str) -> Path:
    return Path(case_path) / _EXT_PATHS[key]


# ── Read helpers ──────────────────────────────────────────────────────────────

def list_label_events(case_path: str | Path) -> list[dict[str, Any]]:
    return read_jsonl_file(_ext_path(case_path, "labels"))


def list_scopes(case_path: str | Path) -> list[dict[str, Any]]:
    return read_jsonl_file(_ext_path(case_path, "scopes"))


def get_scope(case_path: str | Path, scope_id: str) -> dict[str, Any] | None:
    return next(
        (s for s in list_scopes(case_path) if s.get("scope_id") == scope_id), None
    )


def list_sets(case_path: str | Path, set_type: str = "working_set") -> list[dict[str, Any]]:
    """Return sets matching *set_type* ('working_set', 'release_set', 'exclusion_set')."""
    key = {
        "release_set": "release_sets",
        "exclusion_set": "exclusion_sets",
    }.get(set_type, "working_sets")
    return read_jsonl_file(_ext_path(case_path, key))


def get_set(case_path: str | Path, set_id: str) -> dict[str, Any] | None:
    for stype in ("working_set", "release_set", "exclusion_set"):
        found = next((s for s in list_sets(case_path, stype) if s.get("set_id") == set_id), None)
        if found is not None:
            return found
    return None


def list_decisions(case_path: str | Path) -> list[dict[str, Any]]:
    return read_jsonl_file(_ext_path(case_path, "decisions"))


def get_decisions_for_target(
    case_path: str | Path, target_id: str
) -> list[dict[str, Any]]:
    return [
        d for d in list_decisions(case_path)
        if (d.get("target") or {}).get("id") == target_id
    ]


def list_policy_refs(case_path: str | Path) -> list[dict[str, Any]]:
    return read_jsonl_file(_ext_path(case_path, "policy_refs"))


def list_access_events(case_path: str | Path) -> list[dict[str, Any]]:
    return read_jsonl_file(_ext_path(case_path, "access_events"))


def list_denied_access_events(case_path: str | Path) -> list[dict[str, Any]]:
    return read_jsonl_file(_ext_path(case_path, "denied_access_events"))


def list_audit_events(case_path: str | Path) -> list[dict[str, Any]]:
    return read_jsonl_file(_ext_path(case_path, "audit_events"))


# ── Append helpers ────────────────────────────────────────────────────────────

def append_label_event(case_path: str | Path, event: dict[str, Any]) -> None:
    """Append a label event to extensions/labels/labels.jsonl."""
    append_jsonl_file(_ext_path(case_path, "labels"), event)


def append_scope(case_path: str | Path, scope: dict[str, Any]) -> None:
    """Append a scope record to extensions/scopes/scopes.jsonl."""
    append_jsonl_file(_ext_path(case_path, "scopes"), scope)


def append_set(case_path: str | Path, set_record: dict[str, Any]) -> None:
    """Append a set record to the appropriate JSONL file based on set_type."""
    set_type = set_record.get("set_type", "working_set")
    key = {
        "release_set": "release_sets",
        "exclusion_set": "exclusion_sets",
    }.get(set_type, "working_sets")
    append_jsonl_file(_ext_path(case_path, key), set_record)


def append_decision(case_path: str | Path, decision: dict[str, Any]) -> None:
    """Append a decision record to extensions/decisions/decisions.jsonl."""
    append_jsonl_file(_ext_path(case_path, "decisions"), decision)


def append_policy_ref(case_path: str | Path, policy_ref: dict[str, Any]) -> None:
    """Append a policy reference to extensions/policies/policy_refs.jsonl."""
    append_jsonl_file(_ext_path(case_path, "policy_refs"), policy_ref)


def append_access_event(case_path: str | Path, event: dict[str, Any]) -> None:
    """Append an access event to extensions/access/access_events.jsonl."""
    append_jsonl_file(_ext_path(case_path, "access_events"), event)


def append_denied_access_event(case_path: str | Path, event: dict[str, Any]) -> None:
    """Append a denied access event to extensions/access/denied_access_events.jsonl."""
    append_jsonl_file(_ext_path(case_path, "denied_access_events"), event)


def append_audit_event(case_path: str | Path, event: dict[str, Any]) -> None:
    """Append a generic audit event to extensions/audit/audit_events.jsonl."""
    append_jsonl_file(_ext_path(case_path, "audit_events"), event)

