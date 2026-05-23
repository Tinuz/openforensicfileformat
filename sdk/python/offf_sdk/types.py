from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class ChunkRecord:
    sequence: int
    chunk_id: str
    source_offset: int
    source_length: int
    stored_length: int
    compression: str
    plaintext_sha256: str
    stored_sha256: str


@dataclass(frozen=True)
class ProvenanceEvent:
    event_id: str
    timestamp: str
    actor: str
    action: str
    tool_name: str
    tool_version: str
    details: dict[str, Any]
