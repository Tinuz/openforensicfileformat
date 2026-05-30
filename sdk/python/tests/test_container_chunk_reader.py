import hashlib
import json
import tempfile
import unittest
from pathlib import Path

import zstandard as zstd

from offf_sdk.container import OfffContainer
from offf_sdk.types import ChunkRecord


class TestChunkZstdReader(unittest.TestCase):
    def _make_container(self) -> OfffContainer:
        temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(temp_dir.cleanup)
        base = Path(temp_dir.name)

        manifest = {
            "offf_version": "0.1.0",
            "container_id": "test-container",
            "hashes": {
                "source_sha256": "0" * 64,
                "merkle_root_sha256": "0" * 64,
            },
            "indexes": {
                "physical_to_chunk": "indexes/chunk_map.parquet",
            },
        }
        acquisition = {"case_id": "test-case"}

        (base / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        (base / "acquisition.json").write_text(json.dumps(acquisition), encoding="utf-8")

        return OfffContainer(base)

    def _write_chunk_file(self, container: OfffContainer, plain: bytes, stored: bytes) -> ChunkRecord:
        plain_sha = hashlib.sha256(plain).hexdigest()
        stored_sha = hashlib.sha256(stored).hexdigest()
        chunk = ChunkRecord(
            sequence=0,
            chunk_id="chunk-0",
            source_offset=0,
            source_length=len(plain),
            stored_length=len(stored),
            compression="zstd",
            plaintext_sha256=plain_sha,
            stored_sha256=stored_sha,
        )

        path = container.chunk_file_path(chunk)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(stored)
        return chunk

    def test_read_chunk_plaintext_zstd_standard_frame(self) -> None:
        container = self._make_container()
        plain = b"normal-zstd-frame" * 32
        stored = zstd.ZstdCompressor(level=3).compress(plain)
        chunk = self._write_chunk_file(container, plain, stored)

        actual = container.read_chunk_plaintext(chunk, verify=True)
        self.assertEqual(actual, plain)

    def test_read_chunk_plaintext_zstd_frame_without_content_size(self) -> None:
        container = self._make_container()
        plain = b"zstd-frame-without-content-size" * 64
        stored = zstd.ZstdCompressor(level=3, write_content_size=False).compress(plain)
        chunk = self._write_chunk_file(container, plain, stored)

        actual = container.read_chunk_plaintext(chunk, verify=True)
        self.assertEqual(actual, plain)


if __name__ == "__main__":
    unittest.main()
