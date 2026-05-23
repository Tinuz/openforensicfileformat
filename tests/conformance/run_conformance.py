from __future__ import annotations

import json
import shutil
import tempfile
from dataclasses import dataclass, asdict
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SAMPLE = ROOT / "tests" / "samples" / "4orensics.case2.offf"
REPORT = ROOT / "tests" / "conformance" / "conformance-report.json"


@dataclass
class CheckResult:
    name: str
    status: str
    detail: str


def check_exists(path: Path, name: str) -> CheckResult:
    if path.exists():
        return CheckResult(name=name, status="PASS", detail=str(path))
    return CheckResult(name=name, status="FAIL", detail=f"missing: {path}")


def check_non_empty(path: Path, name: str) -> CheckResult:
    if not path.exists():
        return CheckResult(name=name, status="FAIL", detail=f"missing: {path}")
    if path.stat().st_size == 0:
        return CheckResult(name=name, status="FAIL", detail=f"empty: {path}")
    return CheckResult(name=name, status="PASS", detail=str(path))


def profile_status(checks: list[CheckResult]) -> str:
    return "PASS" if all(c.status == "PASS" for c in checks) else "FAIL"


def check_json_has_keys(path: Path, keys: list[str], name: str) -> CheckResult:
    if not path.exists():
        return CheckResult(name=name, status="FAIL", detail=f"missing: {path}")
    try:
        obj = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return CheckResult(name=name, status="FAIL", detail=f"invalid json: {exc}")

    missing = [k for k in keys if k not in obj]
    if missing:
        return CheckResult(name=name, status="FAIL", detail=f"missing keys: {missing}")
    return CheckResult(name=name, status="PASS", detail=str(path))


def evaluate_reader_profile(container: Path) -> list[CheckResult]:
    return [
        check_exists(container / "manifest.json", "manifest_present"),
        check_exists(container / "maps" / "physical_to_chunk.parquet", "mapping_present"),
        check_exists(container / "hashes" / "merkle_tree.bin", "merkle_tree_present"),
        check_non_empty(container / "provenance" / "chain_of_custody.jsonl", "provenance_non_empty"),
        check_exists(container / "chunks" / "sha256", "chunk_store_present"),
    ]


def evaluate_analysis_profile(container: Path) -> list[CheckResult]:
    return [
        check_exists(container / "analysis" / "keyword_hits.parquet", "keyword_hits_present"),
        check_exists(container / "analysis" / "yara_hits.parquet", "yara_hits_present"),
        check_exists(container / "analysis" / "annotations.jsonl", "annotations_present"),
        check_non_empty(container / "provenance" / "chain_of_custody.jsonl", "analysis_provenance_non_empty"),
    ]


def evaluate_indexer_profile(container: Path) -> list[CheckResult]:
    fs_root = container / "indexes" / "filesystems"
    has_partition = any(p.is_dir() for p in fs_root.iterdir()) if fs_root.exists() else False
    partition_detail = str(fs_root) if has_partition else f"no partition dirs in {fs_root}"
    partition_status = "PASS" if has_partition else "FAIL"

    return [
        check_exists(container / "indexes" / "filesystems", "filesystems_index_root_present"),
        CheckResult(
            name="filesystem_partition_dirs_present",
            status=partition_status,
            detail=partition_detail,
        ),
        check_exists(
            container / "indexes" / "filesystems" / "volume-1" / "file_index.parquet",
            "file_index_present",
        ),
    ]


def evaluate_acquisition_profile(container: Path) -> list[CheckResult]:
    acq_path = container / "acquisition.json"
    return [
        check_exists(acq_path, "acquisition_present"),
        check_json_has_keys(
            acq_path,
            ["container_id", "acquired_at", "tool", "source", "parameters"],
            "acquisition_required_keys",
        ),
        check_json_has_keys(
            container / "manifest.json",
            ["offf_version", "container_id", "hashes", "chunking", "indexes"],
            "manifest_required_keys",
        ),
    ]


def run_negative_scenarios() -> list[dict[str, object]]:
    results: list[dict[str, object]] = []

    with tempfile.TemporaryDirectory(prefix="offf-negative-") as tmp:
        base = Path(tmp) / "case.offf"
        shutil.copytree(SAMPLE, base)

        # Negative 1: missing manifest must fail reader profile.
        (base / "manifest.json").unlink(missing_ok=True)
        reader_checks = evaluate_reader_profile(base)
        reader_failed = profile_status(reader_checks) == "FAIL"
        results.append(
            {
                "name": "missing_manifest",
                "expected_profile": "reader",
                "expected_status": "FAIL",
                "observed_status": profile_status(reader_checks),
                "status": "PASS" if reader_failed else "FAIL",
            }
        )

    with tempfile.TemporaryDirectory(prefix="offf-negative-") as tmp:
        base = Path(tmp) / "case.offf"
        shutil.copytree(SAMPLE, base)

        # Negative 2: empty provenance must fail analysis profile.
        (base / "provenance" / "chain_of_custody.jsonl").write_text("", encoding="utf-8")
        analysis_checks = evaluate_analysis_profile(base)
        analysis_failed = profile_status(analysis_checks) == "FAIL"
        results.append(
            {
                "name": "empty_provenance",
                "expected_profile": "analysis",
                "expected_status": "FAIL",
                "observed_status": profile_status(analysis_checks),
                "status": "PASS" if analysis_failed else "FAIL",
            }
        )

    return results


def run() -> int:
    reader_checks = evaluate_reader_profile(SAMPLE)
    analysis_checks = evaluate_analysis_profile(SAMPLE)
    indexer_checks = evaluate_indexer_profile(SAMPLE)
    acquisition_checks = evaluate_acquisition_profile(SAMPLE)
    negative_tests = run_negative_scenarios()

    report = {
        "offf_version": "0.1.0",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "profiles": {
            "reader": {
                "status": profile_status(reader_checks),
                "checks": [asdict(c) for c in reader_checks],
            },
            "analysis": {
                "status": profile_status(analysis_checks),
                "checks": [asdict(c) for c in analysis_checks],
            },
            "indexer": {
                "status": profile_status(indexer_checks),
                "checks": [asdict(c) for c in indexer_checks],
            },
            "acquisition": {
                "status": profile_status(acquisition_checks),
                "checks": [asdict(c) for c in acquisition_checks],
            },
        },
        "negative_tests": negative_tests,
    }

    REPORT.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"Wrote conformance report to {REPORT}")

    failed_profiles = [
        name
        for name, data in report["profiles"].items()
        if data["status"] == "FAIL"
    ]
    failed_negative = [t["name"] for t in negative_tests if t["status"] == "FAIL"]
    if failed_profiles or failed_negative:
        print(f"FAILED profiles: {', '.join(failed_profiles)}")
        if failed_negative:
            print(f"FAILED negative tests: {', '.join(failed_negative)}")
        return 1

    print("Conformance scaffold checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(run())
