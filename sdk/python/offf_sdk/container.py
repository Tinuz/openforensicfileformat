from __future__ import annotations

import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterator

import pyarrow as pa
import pyarrow.parquet as pq
import zstandard as zstd

from .errors import OfffError, UnsupportedVersionError, ValidationError
from .schema_validation import SchemaError, validate_acquisition, validate_manifest, validate_provenance_events
from .types import ChunkRecord, ProvenanceEvent


class OfffContainer:
    """SDK entry point for reading and validating OFFF containers.

    Basic usage:
        c = OfffContainer("case.offf")
        ok = c.verify_source_hash()
        root_ok = c.verify_merkle_root()
        first_1k = c.read_bytes(0, 1024)
    """

    SUPPORTED_VERSION = "0.1.0"

    def __init__(self, container_path: str | Path) -> None:
        self.base_path = Path(container_path)
        if not self.base_path.exists():
            raise OfffError(f"container path does not exist: {self.base_path}")

        self.manifest = self._read_json("manifest.json")
        self.acquisition = self._read_json("acquisition.json")

        version = self.manifest.get("offf_version")
        if version != self.SUPPORTED_VERSION:
            raise UnsupportedVersionError(
                f"unsupported OFFF version: {version} (expected {self.SUPPORTED_VERSION})"
            )

        self._chunk_map: list[ChunkRecord] | None = None
        self._chunk_cache: dict[int, bytes] = {}

    def _read_json(self, rel_path: str) -> dict[str, Any]:
        path = self.base_path / rel_path
        if not path.exists():
            raise OfffError(f"required file missing: {rel_path}")
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            raise OfffError(f"invalid JSON in {rel_path}: {exc}") from exc

    @property
    def container_id(self) -> str:
        return str(self.manifest["container_id"])

    @property
    def chunk_map(self) -> list[ChunkRecord]:
        if self._chunk_map is None:
            self._chunk_map = self._load_chunk_map()
        return self._chunk_map

    def _load_chunk_map(self) -> list[ChunkRecord]:
        rel = self.manifest["indexes"]["physical_to_chunk"]
        map_path = self.base_path / rel
        if not map_path.exists():
            raise OfffError(f"mapping parquet missing: {rel}")

        table = pq.read_table(map_path)
        required_columns = {
            "sequence",
            "chunk_id",
            "source_offset",
            "source_length",
            "stored_length",
            "compression",
            "plaintext_sha256",
            "stored_sha256",
        }
        missing = required_columns.difference(table.column_names)
        if missing:
            raise OfffError(f"mapping parquet missing columns: {sorted(missing)}")

        rows: list[ChunkRecord] = []
        cols = {name: table[name] for name in required_columns}
        for i in range(table.num_rows):
            rows.append(
                ChunkRecord(
                    sequence=int(cols["sequence"][i].as_py()),
                    chunk_id=str(cols["chunk_id"][i].as_py()),
                    source_offset=int(cols["source_offset"][i].as_py()),
                    source_length=int(cols["source_length"][i].as_py()),
                    stored_length=int(cols["stored_length"][i].as_py()),
                    compression=str(cols["compression"][i].as_py()),
                    plaintext_sha256=str(cols["plaintext_sha256"][i].as_py()),
                    stored_sha256=str(cols["stored_sha256"][i].as_py()),
                )
            )

        rows.sort(key=lambda r: r.sequence)
        return rows

    def iter_chunks(self) -> Iterator[ChunkRecord]:
        yield from self.chunk_map

    def chunk_file_path(self, chunk: ChunkRecord | str) -> Path:
        if isinstance(chunk, ChunkRecord):
            hex_hash = chunk.plaintext_sha256
        else:
            hex_hash = chunk.removeprefix("sha256:")
        return (
            self.base_path
            / "chunks"
            / "sha256"
            / hex_hash[:2]
            / hex_hash[2:4]
            / f"{hex_hash}.chunk"
        )

    def read_chunk_plaintext(self, chunk: ChunkRecord, verify: bool = True) -> bytes:
        cached = self._chunk_cache.get(chunk.sequence)
        if cached is not None:
            return cached

        path = self.chunk_file_path(chunk)
        if not path.exists():
            raise OfffError(f"chunk file missing: {path}")

        stored = path.read_bytes()
        if verify:
            actual_stored = hashlib.sha256(stored).hexdigest()
            if actual_stored != chunk.stored_sha256:
                raise ValidationError(
                    f"stored hash mismatch for chunk {chunk.sequence}: "
                    f"expected={chunk.stored_sha256} actual={actual_stored}"
                )

        if chunk.compression == "none":
            plain = stored
        elif chunk.compression == "zstd":
            plain = zstd.ZstdDecompressor().decompress(stored)
        else:
            raise OfffError(f"unsupported compression in chunk map: {chunk.compression}")

        if verify:
            actual_plain = hashlib.sha256(plain).hexdigest()
            if actual_plain != chunk.plaintext_sha256:
                raise ValidationError(
                    f"plaintext hash mismatch for chunk {chunk.sequence}: "
                    f"expected={chunk.plaintext_sha256} actual={actual_plain}"
                )

        self._chunk_cache[chunk.sequence] = plain
        return plain

    def read_bytes(self, source_offset: int, length: int, verify_chunks: bool = True) -> bytes:
        if source_offset < 0 or length < 0:
            raise ValueError("source_offset and length must be >= 0")
        if length == 0:
            return b""

        start = source_offset
        end = source_offset + length
        out = bytearray()

        for chunk in self.chunk_map:
            c_start = chunk.source_offset
            c_end = chunk.source_offset + chunk.source_length
            if c_end <= start:
                continue
            if c_start >= end:
                break

            plain = self.read_chunk_plaintext(chunk, verify=verify_chunks)
            take_start = max(start, c_start)
            take_end = min(end, c_end)
            rel_start = take_start - c_start
            rel_end = take_end - c_start
            out.extend(plain[rel_start:rel_end])

            if len(out) >= length:
                break

        if len(out) != length:
            raise ValidationError(
                f"could not read requested range fully: requested={length} got={len(out)}"
            )
        return bytes(out)

    def compute_source_sha256(self) -> str:
        h = hashlib.sha256()
        for chunk in self.chunk_map:
            h.update(self.read_chunk_plaintext(chunk, verify=True))
        return h.hexdigest()

    def verify_source_hash(self) -> bool:
        expected = str(self.manifest["hashes"]["source_sha256"])
        actual = self.compute_source_sha256()
        return expected == actual

    def compute_merkle_root(self) -> str:
        leaves = [r.plaintext_sha256 for r in self.chunk_map]
        if not leaves:
            raise ValidationError("cannot compute Merkle root from empty leaf set")
        return _merkle_root_from_hex_leaves(leaves)

    def read_merkle_root_from_bin(self) -> str:
        path = self.base_path / "hashes" / "merkle_tree.bin"
        data = path.read_bytes()
        if len(data) < 4 + 1 + 4 + 32:
            raise ValidationError("merkle_tree.bin too short")
        if data[:4] != b"OFFF":
            raise ValidationError("merkle_tree.bin bad magic")
        if data[4] != 0x01:
            raise ValidationError(f"unsupported merkle_tree.bin version: {data[4]}")
        return data[-32:].hex()

    def verify_merkle_root(self) -> bool:
        expected = str(self.manifest["hashes"]["merkle_root_sha256"])
        from_bin = self.read_merkle_root_from_bin()
        from_map = self.compute_merkle_root()
        return expected == from_bin == from_map

    def iter_provenance_events(self) -> Iterator[ProvenanceEvent]:
        path = self.base_path / "provenance" / "chain_of_custody.jsonl"
        if not path.exists():
            return
        for raw in path.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if not line:
                continue
            obj = json.loads(line)
            tool = obj.get("tool") or {}
            yield ProvenanceEvent(
                event_id=str(obj.get("event_id", "")),
                timestamp=str(obj.get("timestamp", "")),
                actor=str(obj.get("actor", "")),
                action=str(obj.get("action", "")),
                tool_name=str(tool.get("name", "")),
                tool_version=str(tool.get("version", "")),
                details=dict(obj.get("details") or {}),
            )

    def _iter_provenance_raw(self) -> Iterator[dict[str, Any]]:
        path = self.base_path / "provenance" / "chain_of_custody.jsonl"
        if not path.exists():
            return
        for raw in path.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if not line:
                continue
            yield json.loads(line)

    def validate_manifest_schema(self) -> list[SchemaError]:
        return validate_manifest(self.manifest)

    def validate_acquisition_schema(self) -> list[SchemaError]:
        return validate_acquisition(self.acquisition)

    def validate_provenance_schema(self) -> list[SchemaError]:
        return validate_provenance_events(self._iter_provenance_raw())

    def validate_all_schemas(self) -> dict[str, list[SchemaError]]:
        return {
            "manifest": self.validate_manifest_schema(),
            "acquisition": self.validate_acquisition_schema(),
            "provenance": self.validate_provenance_schema(),
        }

    # Required minimal SDK surface

    def read_manifest(self) -> dict[str, Any]:
        return dict(self.manifest)

    def verify_container(self) -> dict[str, bool]:
        schema = self.validate_all_schemas()
        schema_ok = all(len(v) == 0 for v in schema.values())
        source_ok = self.verify_source_hash()
        merkle_ok = self.verify_merkle_root()
        return {
            "schema": schema_ok,
            "source_hash": source_ok,
            "merkle_root": merkle_ok,
            "valid": schema_ok and source_ok and merkle_ok,
        }

    def _resolve_chunk(self, chunk_ref: int | str | ChunkRecord) -> ChunkRecord:
        if isinstance(chunk_ref, ChunkRecord):
            return chunk_ref
        if isinstance(chunk_ref, int):
            for chunk in self.chunk_map:
                if chunk.sequence == chunk_ref:
                    return chunk
            raise OfffError(f"chunk sequence not found: {chunk_ref}")
        if isinstance(chunk_ref, str):
            wanted = chunk_ref
            for chunk in self.chunk_map:
                if chunk.chunk_id == wanted:
                    return chunk
            raise OfffError(f"chunk id not found: {chunk_ref}")
        raise TypeError("chunk_ref must be sequence(int), chunk_id(str), or ChunkRecord")

    def read_chunk(self, chunk_ref: int | str | ChunkRecord, verify: bool = True) -> bytes:
        chunk = self._resolve_chunk(chunk_ref)
        return self.read_chunk_plaintext(chunk, verify=verify)

    def verify_chunk(self, chunk_ref: int | str | ChunkRecord) -> bool:
        chunk = self._resolve_chunk(chunk_ref)
        self.read_chunk_plaintext(chunk, verify=True)
        return True

    def map_offset_to_chunk(self, source_offset: int) -> tuple[ChunkRecord, int]:
        if source_offset < 0:
            raise ValueError("source_offset must be >= 0")
        for chunk in self.chunk_map:
            start = chunk.source_offset
            end = chunk.source_offset + chunk.source_length
            if start <= source_offset < end:
                return chunk, source_offset - start
        raise OfffError(f"offset outside mapped range: {source_offset}")

    def read_file_index(self, partition_id: str | None = None) -> list[dict[str, Any]]:
        if partition_id is None:
            indexes_root = self.base_path / "indexes" / "filesystems"
            if not indexes_root.exists():
                return []
            rows: list[dict[str, Any]] = []
            for sub in sorted(indexes_root.iterdir()):
                if not sub.is_dir():
                    continue
                p = sub / "file_index.parquet"
                if p.exists():
                    rows.extend(self._read_parquet_rows(p))
            return rows

        path = self.base_path / "indexes" / "filesystems" / partition_id / "file_index.parquet"
        if not path.exists():
            raise OfffError(f"file index not found for partition: {partition_id}")
        return self._read_parquet_rows(path)

    def _read_parquet_rows(self, path: Path) -> list[dict[str, Any]]:
        table = pq.read_table(path)
        cols = table.column_names
        out: list[dict[str, Any]] = []
        for i in range(table.num_rows):
            out.append({c: table[c][i].as_py() for c in cols})
        return out

    def write_analysis_result(self, relative_path: str, rows: list[dict[str, Any]]) -> Path:
        rel = relative_path.replace("\\", "/").lstrip("/")
        if not rel.startswith("analysis/"):
            raise OfffError("analysis output path must start with 'analysis/'")

        target = self.base_path / rel
        target.parent.mkdir(parents=True, exist_ok=True)

        if rel.endswith(".jsonl"):
            with target.open("w", encoding="utf-8") as f:
                for row in rows:
                    f.write(json.dumps(row, ensure_ascii=False))
                    f.write("\n")
            return target

        if rel.endswith(".parquet"):
            table = pa.Table.from_pylist(rows)
            pq.write_table(table, target)
            return target

        raise OfffError("analysis output must end with .jsonl or .parquet")

    def append_provenance_event(
        self,
        action: str,
        actor: str,
        details: dict[str, Any],
        tool_name: str = "offf-sdk-python",
        tool_version: str = "0.1.0",
    ) -> ProvenanceEvent:
        path = self.base_path / "provenance" / "chain_of_custody.jsonl"
        path.parent.mkdir(parents=True, exist_ok=True)

        count = 0
        if path.exists():
            for raw in path.read_text(encoding="utf-8").splitlines():
                if raw.strip():
                    count += 1

        event_obj = {
            "event_id": f"evt-{count:06}",
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "actor": actor,
            "action": action,
            "tool": {
                "name": tool_name,
                "version": tool_version,
            },
            "details": details,
        }

        with path.open("a", encoding="utf-8") as f:
            f.write(json.dumps(event_obj, ensure_ascii=False))
            f.write("\n")

        return ProvenanceEvent(
            event_id=event_obj["event_id"],
            timestamp=event_obj["timestamp"],
            actor=event_obj["actor"],
            action=event_obj["action"],
            tool_name=tool_name,
            tool_version=tool_version,
            details=details,
        )


def _merkle_root_from_hex_leaves(leaves: list[str]) -> str:
    level = [_hex_to_bytes32(h) for h in leaves]
    if len(level) == 1:
        return level[0].hex()

    while len(level) > 1:
        next_level: list[bytes] = []
        i = 0
        while i < len(level):
            left = level[i]
            right = level[i + 1] if i + 1 < len(level) else level[i]
            next_level.append(hashlib.sha256(left + right).digest())
            i += 2
        level = next_level
    return level[0].hex()


def _hex_to_bytes32(h: str) -> bytes:
    if len(h) != 64:
        raise ValidationError(f"expected 64-char hex hash, got {len(h)}")
    try:
        b = bytes.fromhex(h)
    except ValueError as exc:
        raise ValidationError(f"invalid hex hash: {h}") from exc
    if len(b) != 32:
        raise ValidationError("hash must decode to 32 bytes")
    return b
