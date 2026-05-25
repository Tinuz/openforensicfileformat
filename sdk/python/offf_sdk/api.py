from __future__ import annotations

from pathlib import Path
from typing import Any

from .container import OfffContainer
from .types import ChunkRecord, ProvenanceEvent


def open_container(container_path: str | Path) -> OfffContainer:
    return OfffContainer(container_path)


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
