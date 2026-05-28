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


# ── Object graph read/query helpers (Sprint 18) ───────────────────────────────
#
# These helpers work with JSONL-based object indexes generated by Python workers.
# For Parquet-based indexes (written by Rust tools), use OfffContainer methods.
#
# Standard paths:
#   indexes/objects/object_index.jsonl
#   indexes/objects/object_edges.jsonl
#   indexes/objects/derivations.jsonl

def load_object_edges(case_path: str | Path) -> list[dict[str, Any]]:
    """Load all object edges from indexes/objects/object_edges.jsonl."""
    return read_jsonl_file(Path(case_path) / "indexes" / "objects" / "object_edges.jsonl")


def get_object(case_path: str | Path, object_id: str) -> dict[str, Any] | None:
    """Return the object record for *object_id*, or None if not found."""
    return next(
        (o for o in load_object_index(case_path) if o.get("object_id") == object_id),
        None,
    )


def list_objects(
    case_path: str | Path,
    *,
    object_type: str | None = None,
    parent_id: str | None = None,
    limit: int | None = None,
    offset: int = 0,
) -> list[dict[str, Any]]:
    """Return objects with optional filtering and pagination.

    Args:
        case_path:   Path to the OFFF case directory.
        object_type: Filter to objects of this type (e.g. "file", "partition").
        parent_id:   Filter to objects with this parent_id.
        limit:       Maximum number of records to return.
        offset:      Skip the first *offset* matching records.
    """
    rows = load_object_index(case_path)

    if object_type is not None:
        rows = [r for r in rows if r.get("object_type") == object_type]
    if parent_id is not None:
        rows = [r for r in rows if r.get("parent_id") == parent_id or r.get("source_object_id") == parent_id]

    rows = rows[offset:]
    if limit is not None:
        rows = rows[:limit]
    return rows


def get_object_children(
    case_path: str | Path,
    object_id: str,
    *,
    relationship: str | None = None,
    limit: int | None = None,
    offset: int = 0,
) -> list[dict[str, Any]]:
    """Return objects that are direct children of *object_id*.

    Consults both the edge index (``object_edges.jsonl``) and ``parent_id``
    fields in the object index to support both edge-based and parent-pointer
    graph layouts.

    Args:
        object_id:    The source/parent object ID to query.
        relationship: If given, only edges with this relationship type.
        limit:        Maximum number of results.
        offset:       Skip this many matching results.
    """
    objects = {o["object_id"]: o for o in load_object_index(case_path) if "object_id" in o}

    # Collect child IDs from edge index
    child_ids: list[str] = []
    for edge in load_object_edges(case_path):
        if edge.get("source_object_id") == object_id:
            if relationship is None or edge.get("relationship") == relationship:
                target = edge.get("target_object_id")
                if target:
                    child_ids.append(target)

    # Also check parent_id / source_object_id fields in object index
    for obj in objects.values():
        if obj.get("parent_id") == object_id or obj.get("source_object_id") == object_id:
            oid = obj.get("object_id")
            if oid and oid not in child_ids:
                child_ids.append(oid)

    result = [objects[cid] for cid in child_ids if cid in objects]
    result = result[offset:]
    if limit is not None:
        result = result[:limit]
    return result


def get_object_parents(
    case_path: str | Path,
    object_id: str,
) -> list[dict[str, Any]]:
    """Return the parent objects of *object_id*.

    Consults the edge index (``object_edges.jsonl``) for incoming edges and
    checks ``parent_id`` / ``source_object_id`` fields on the object itself.
    """
    objects = {o["object_id"]: o for o in load_object_index(case_path) if "object_id" in o}
    parent_ids: list[str] = []

    # Parent pointer on the object itself
    obj = objects.get(object_id)
    if obj:
        for field in ("parent_id", "source_object_id"):
            pid = obj.get(field)
            if pid and pid not in parent_ids:
                parent_ids.append(pid)

    # Reverse edges: edges where *this* object is the target
    for edge in load_object_edges(case_path):
        if edge.get("target_object_id") == object_id:
            src = edge.get("source_object_id")
            if src and src not in parent_ids:
                parent_ids.append(src)

    return [objects[pid] for pid in parent_ids if pid in objects]


def get_object_lineage_path(
    case_path: str | Path,
    object_id: str,
    *,
    max_depth: int = 256,
) -> list[dict[str, Any]]:
    """Return the lineage path from root(s) to *object_id* (inclusive).

    The list is ordered root-first.  If multiple parents exist at any level
    (DAG), the first parent encountered is followed.

    Args:
        object_id: The leaf object to trace back to the root.
        max_depth: Guard against cycles — raises ValueError if exceeded.

    Returns:
        List of object records from root to the given object.
    """
    objects = {o["object_id"]: o for o in load_object_index(case_path) if "object_id" in o}
    path: list[dict[str, Any]] = []
    visited: set[str] = set()
    current_id: str | None = object_id

    while current_id is not None:
        if current_id in visited:
            raise ValueError(f"cycle detected in object graph at object_id={current_id!r}")
        if len(path) >= max_depth:
            raise ValueError(f"lineage path exceeded max_depth={max_depth}")

        obj = objects.get(current_id)
        if obj is None:
            break

        path.append(obj)
        visited.add(current_id)

        # Follow parent pointer
        parent_id = obj.get("parent_id") or obj.get("source_object_id")
        current_id = parent_id if parent_id and parent_id != current_id else None

    path.reverse()
    return path


def export_lineage_report(
    case_path: str | Path,
    output_path: str | Path | None = None,
) -> dict[str, Any]:
    """Build a machine-readable lineage report for all objects in the case.

    The report includes:
    - Full object listing (object_id, object_type, parent_id, name)
    - Edge listing
    - Per-root lineage tree summary

    If *output_path* is provided the report is written as JSON to that path.
    Always returns the report dict.
    """
    objects = load_object_index(case_path)
    edges = load_object_edges(case_path)

    # Build parent map
    obj_by_id = {o["object_id"]: o for o in objects if "object_id" in o}
    children_map: dict[str, list[str]] = {}
    for obj in obj_by_id.values():
        oid = obj["object_id"]
        parent = obj.get("parent_id") or obj.get("source_object_id")
        if parent:
            children_map.setdefault(parent, []).append(oid)
    for edge in edges:
        src = edge.get("source_object_id")
        tgt = edge.get("target_object_id")
        if src and tgt:
            children_map.setdefault(src, []).append(tgt)

    # Find roots (objects with no parent)
    roots = [
        oid
        for oid, obj in obj_by_id.items()
        if not (obj.get("parent_id") or obj.get("source_object_id"))
        and not any(tgt == oid for e in edges for tgt in [e.get("target_object_id")])
    ]

    def _build_tree(oid: str, depth: int = 0) -> dict[str, Any]:
        obj = obj_by_id.get(oid, {"object_id": oid})
        return {
            "object_id": oid,
            "object_type": obj.get("object_type"),
            "name": obj.get("name"),
            "depth": depth,
            "children": [_build_tree(c, depth + 1) for c in children_map.get(oid, [])],
        }

    report: dict[str, Any] = {
        "generated_at": utc_now_iso(),
        "case_path": str(case_path),
        "summary": {
            "total_objects": len(objects),
            "total_edges": len(edges),
            "root_count": len(roots),
        },
        "trees": [_build_tree(r) for r in roots],
        "objects": [
            {
                "object_id": o.get("object_id"),
                "object_type": o.get("object_type"),
                "parent_id": o.get("parent_id") or o.get("source_object_id"),
                "name": o.get("name"),
            }
            for o in objects
        ],
        "edges": edges,
    }

    if output_path is not None:
        write_json_file(output_path, report)

    return report


