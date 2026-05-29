#!/usr/bin/env python3
"""
normalize_backlog.py
Add missing metadata fields (Classification, Maturity, Implemented in, Tests,
Conformance, Known limitations) to all done backlog items that lack them.
Idempotent: already-present fields are never duplicated.
"""
import re
import pathlib

ROOT = pathlib.Path(__file__).parent.parent
BACKLOG = ROOT / "BACKLOG.txt"

# Per-item metadata: keyed on a normalised title fragment (first ~40 chars lowercase).
# Fields: classification, maturity, implemented_in (list), tests (str), conformance (str), limitations (str)
ITEM_META = {
    "hardening sprint 0": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": ["README.md", "docs/status.md"],
        "tests": "manual verification",
        "conformance": "OFFF Core Conformant",
        "limitations": "none",
    },
    "hardening sprint 2": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": ["crates/offf-core/src/chunk.rs"],
        "tests": "test_write_chunk_reuse_valid_existing, test_write_chunk_corrupt_existing_fails",
        "conformance": "OFFF Core Conformant",
        "limitations": "none",
    },
    "hardening sprint 3": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": ["crates/offf-convert/src/main.rs", "crates/offf-core/src/types.rs"],
        "tests": "test_deterministic_mode_produces_stable_manifest, test_sector_size_persisted",
        "conformance": "OFFF Core Conformant",
        "limitations": "none",
    },
    "hardening sprint 4": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": ["crates/offf-core/src/hash.rs", "crates/offf-verify/src/main.rs"],
        "tests": "test_merkle_round_trip, test_merkle_proof_valid, test_merkle_proof_invalid",
        "conformance": "OFFF Core Conformant",
        "limitations": "none",
    },
    "hardening sprint 5": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": ["crates/offf-verify/src/main.rs"],
        "tests": "test_verify_profiles, test_leaves_consistency",
        "conformance": "OFFF Core Conformant, OFFF Conformance Profile Conformant",
        "limitations": "none",
    },
    "hardening sprint 6": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": [
            "crates/offf-keyword-worker/src/main.rs",
            "crates/offf-yara-worker/src/main.rs",
            "crates/offf-access-service/src/main.rs",
        ],
        "tests": "test_keyword_worker_writes_result_manifest, test_yara_worker_writes_result_manifest",
        "conformance": "OFFF Analysis Worker Conformant",
        "limitations": "none",
    },
    "hardening sprint 7": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": ["crates/offf-access-service/src/main.rs"],
        "tests": "grpc_smoke (denied overwrite logging verified)",
        "conformance": "OFFF Access Service Conformant",
        "limitations": "JWT not independently security-reviewed; mTLS path not integration-tested",
    },
    "hardening sprint 8": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": [".github/workflows/offf-ci.yml", "tests/e2e/run_cli_e2e.py"],
        "tests": "CI: rust-quality-gates, cli-e2e, schema-validation, conformance-scaffold",
        "conformance": "OFFF Conformance Profile Conformant",
        "limitations": "none",
    },
    "lineage sprint 9": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": [
            "crates/offf-core/src/types.rs",
            "crates/offf-core/src/parquet_io.rs",
            "crates/offf-core/src/lineage.rs",
            "docs/schema/",
        ],
        "tests": "test_object_index_round_trip, test_lineage_validator_referential_integrity",
        "conformance": "OFFF Lineage Conformant",
        "limitations": "none",
    },
    "lineage sprint 10": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": [
            "crates/offf-core/src/storage.rs",
            "crates/offf-keyword-worker/src/main.rs",
            "crates/offf-yara-worker/src/main.rs",
            "crates/offf-jobs/src/main.rs",
        ],
        "tests": "test_derived_object_store_write_verify, test_result_manifest_v2_complete",
        "conformance": "OFFF Analysis Worker Conformant",
        "limitations": "none",
    },
    "lineage sprint 11": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": [
            "crates/offf-access-service/src/main.rs",
            "crates/offf-access-service/proto/offf_access.proto",
            "sdk/python/offf_sdk/",
        ],
        "tests": "grpc_smoke (object-producing job mock)",
        "conformance": "OFFF Access Service Conformant, OFFF Lineage Conformant",
        "limitations": "none",
    },
    "lineage sprint 12": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": [
            "crates/offf-index/src/main.rs",
            "crates/offf-verify/src/main.rs",
        ],
        "tests": "test_object_index_rebuild_deterministic, test_lineage_verify_valid, test_lineage_verify_invalid",
        "conformance": "OFFF Lineage Conformant, OFFF Conformance Profile Conformant",
        "limitations": "none",
    },
    "offf v0.2 manifest extensions foundation": {
        "classification": "core",
        "maturity": "forensic-grade-candidate",
        "implemented_in": ["crates/offf-core/src/types.rs", "docs/schema/"],
        "tests": "manifest_v010_json_loadable_by_v020_reader, manifest_v020_round_trip_with_extensions",
        "conformance": "OFFF Core Conformant",
        "limitations": "none",
    },
    "demo sprint 14": {
        "classification": "demo",
        "maturity": "demo-only",
        "implemented_in": ["crates/offf-convert/src/main.rs (demo commands via offf-verify/offf-jobs)"],
        "tests": "smoke scripts; demo case integration",
        "conformance": "none (demo-only)",
        "limitations": "demo-only; not suitable for production use",
    },
    "object graph read/query apis": {
        "classification": "reference",
        "maturity": "experimental",
        "implemented_in": ["sdk/python/offf_sdk/api.py"],
        "tests": "Python SDK contract tests",
        "conformance": "OFFF SDK Conformant",
        "limitations": "Python SDK only; no Rust API parity; pagination experimental",
    },
    "legacy compatibility profile": {
        "classification": "reference",
        "maturity": "reference",
        "implemented_in": ["crates/offf-verify/src/main.rs (VerifyProfile::Legacy)"],
        "tests": "test_legacy_profile_warns_non_forensic_grade",
        "conformance": "OFFF Legacy Profile Conformant",
        "limitations": "legacy mode emits warnings only; does not block non-forensic-grade outputs",
    },
    "generic extension types + append-only apis": {
        "classification": "core",
        "maturity": "experimental",
        "implemented_in": [
            "crates/offf-core/src/extensions.rs",
            "crates/offf-verify/src/main.rs",
            "sdk/python/offf_sdk/",
        ],
        "tests": "test_extension_append_jsonl, test_extension_verify_content",
        "conformance": "OFFF Extension Conformant",
        "limitations": "Access Service REST/gRPC extension endpoints partially implemented",
    },
    "scope-aware jobs and workers": {
        "classification": "core",
        "maturity": "experimental",
        "implemented_in": [
            "crates/offf-core/src/types.rs",
            "crates/offf-keyword-worker/src/main.rs",
            "crates/offf-yara-worker/src/main.rs",
            "crates/offf-jobs/src/main.rs",
        ],
        "tests": "test_scope_ref_propagated_to_result_manifest, test_scope_audit_event_emitted",
        "conformance": "OFFF Analysis Worker Conformant",
        "limitations": "exclude_sets and exclude_labels not yet enforced in workers",
    },
    "object-per-event append model": {
        "classification": "core",
        "maturity": "experimental",
        "implemented_in": [
            "crates/offf-core/src/types.rs",
            "crates/offf-index/src/main.rs",
            "sdk/python/offf_sdk/",
        ],
        "tests": "test_object_event_append_and_rebuild, python SDK rebuild tests",
        "conformance": "OFFF Lineage Conformant",
        "limitations": "none",
    },
    "indexing hardening gpt/mbr": {
        "classification": "core",
        "maturity": "experimental",
        "implemented_in": ["crates/offf-core/src/ntfs.rs"],
        "tests": "std_info_flags_decoded, ads_streams_detected (68 workspace tests pass)",
        "conformance": "OFFF Core Conformant",
        "limitations": "NTFS parser experimental; see docs/forensic-limitations.md",
    },
    "python sdk hardening": {
        "classification": "reference",
        "maturity": "experimental",
        "implemented_in": ["sdk/python/offf_sdk/"],
        "tests": "Python SDK contract tests, test_api_contract.py",
        "conformance": "OFFF SDK Conformant",
        "limitations": "LRU cache size limits experimental; not independently benchmarked",
    },
    "object lineage scale/performance": {
        "classification": "reference",
        "maturity": "experimental",
        "implemented_in": ["crates/offf-core/src/storage.rs", "crates/offf-index/src/main.rs", "sdk/python/offf_sdk/"],
        "tests": "test_streaming_read_large_derived_object",
        "conformance": "OFFF Lineage Conformant",
        "limitations": "batch size tuning and DOT export are experimental",
    },
    "threat model and security mapping": {
        "classification": "core",
        "maturity": "reference",
        "implemented_in": ["docs/threat-model.md"],
        "tests": "none (documentation artifact)",
        "conformance": "none",
        "limitations": "gap: no automated test; manual review required",
    },
    "versioning policy and migration path": {
        "classification": "core",
        "maturity": "reference",
        "implemented_in": ["docs/versioning.md"],
        "tests": "manifest_v010_json_loadable_by_v020_reader, manifest_v020_round_trip_with_extensions",
        "conformance": "OFFF Core Conformant",
        "limitations": "offf-migrate CLI not yet implemented",
    },
    "ntfs forensic depth": {
        "classification": "core",
        "maturity": "experimental",
        "implemented_in": ["crates/offf-core/src/ntfs.rs"],
        "tests": "std_info_flags_decoded, ads_streams_detected",
        "conformance": "OFFF Core Conformant",
        "limitations": "NTFS parser experimental; see docs/forensic-limitations.md",
    },
    "offf packed container": {
        "classification": "reference",
        "maturity": "experimental",
        "implemented_in": ["crates/offf-core/src/packed.rs", "crates/offf-export/src/main.rs"],
        "tests": "packed unit tests in cargo test -p offf-core",
        "conformance": "none (supplementary format)",
        "limitations": "single-file format not canonical; exploded directory model is canonical",
    },
    "phase 5: minio": {
        "classification": "reference",
        "maturity": "experimental",
        "implemented_in": ["tests/smoke/phase5_minio_smoke.py"],
        "tests": "tests/smoke/phase5_minio_smoke.py (requires running MinIO instance)",
        "conformance": "OFFF Core Conformant",
        "limitations": "requires external MinIO instance; not run in standard CI",
    },
    "phase 7: e01 smoke": {
        "classification": "reference",
        "maturity": "experimental",
        "implemented_in": ["tests/smoke/phase7_e01_smoke.py"],
        "tests": "tests/smoke/phase7_e01_smoke.py (requires libewf/ewfexport in PATH)",
        "conformance": "OFFF Core Conformant",
        "limitations": "requires libewf tooling; not run in standard CI",
    },
    "access service: storage backends parity": {
        "classification": "core",
        "maturity": "experimental",
        "implemented_in": ["crates/offf-access-service/src/main.rs", "crates/offf-access-service/tests/grpc_storage_parity.rs"],
        "tests": "grpc_storage_parity smoke test",
        "conformance": "OFFF Access Service Conformant",
        "limitations": "S3 path experimental; Ceph not tested",
    },
    "go sdk minimal": {
        "classification": "reference",
        "maturity": "experimental",
        "implemented_in": ["sdk/go/sdk.go", "sdk/go/sdk_test.go"],
        "tests": "sdk/go/sdk_test.go; CI job go-sdk-smoke",
        "conformance": "OFFF SDK Conformant",
        "limitations": "API surface may change before v1.0; write paths not verified against Rust in CI",
    },
}


def normalise_key(title: str) -> str:
    return re.sub(r"[^a-z0-9 /]", "", title.lower()).strip()


def find_meta(title: str) -> dict | None:
    key = normalise_key(title)
    for pattern, meta in ITEM_META.items():
        if pattern in key:
            return meta
    return None


def add_meta_fields_after_done(block: str, meta: dict) -> str:
    """Insert missing metadata fields after the first 'Done:' line in a block."""
    # Don't add if all fields already present
    if "Classification:" in block and "Maturity:" in block:
        return block

    lines = block.splitlines(keepends=True)
    # Find the Done: line (or Status: done if no Done:)
    insert_after = -1
    for i, line in enumerate(lines):
        if re.match(r"\s+- Done:", line) or re.match(r"\s+- Status:\s+done", line, re.IGNORECASE):
            insert_after = i

    if insert_after == -1:
        return block  # can't find insertion point

    # Build fields to insert (skip if already present)
    new_lines = []
    indent = "  "
    if "Classification:" not in block and "classification:" not in block:
        new_lines.append(f"{indent}- Classification: {meta['classification']}\n")
    if "Maturity:" not in block and "maturity:" not in block:
        new_lines.append(f"{indent}- Maturity: {meta['maturity']}\n")
    if "Implemented in:" not in block and "implemented_in:" not in block and "implemented-in:" not in block:
        impl = meta["implemented_in"]
        if len(impl) == 1:
            new_lines.append(f"{indent}- Implemented in: {impl[0]}\n")
        else:
            new_lines.append(f"{indent}- Implemented in:\n")
            for p in impl:
                new_lines.append(f"{indent}    - {p}\n")
    if "Tests:" not in block and "tests:" not in block:
        new_lines.append(f"{indent}- Tests: {meta['tests']}\n")
    if "Conformance:" not in block and "conformance:" not in block:
        new_lines.append(f"{indent}- Conformance: {meta['conformance']}\n")
    if "Known limitations:" not in block and "known limitations:" not in block.lower():
        new_lines.append(f"{indent}- Known limitations: {meta['limitations']}\n")

    if not new_lines:
        return block

    result = lines[: insert_after + 1] + new_lines + lines[insert_after + 1 :]
    return "".join(result)


def normalise_backlog(text: str) -> str:
    # Split on item boundaries (lines starting with "- [")
    # but preserve non-item text as-is
    parts = re.split(r"(?=^- \[)", text, flags=re.MULTILINE)
    out = []
    for part in parts:
        if part.startswith("- [x]") and "Status: done" in part:
            title_match = re.match(r"- \[x\]\s+(.+?)(?:\n|$)", part)
            if title_match:
                meta = find_meta(title_match.group(1))
                if meta:
                    part = add_meta_fields_after_done(part, meta)
        out.append(part)
    return "".join(out)


if __name__ == "__main__":
    original = BACKLOG.read_text(encoding="utf-8")
    normalised = normalise_backlog(original)
    if normalised == original:
        print("BACKLOG.txt: already fully normalised, no changes.")
    else:
        BACKLOG.write_text(normalised, encoding="utf-8")
        added = normalised.count("\n") - original.count("\n")
        print(f"BACKLOG.txt: normalised — added ~{added} lines of metadata.")
