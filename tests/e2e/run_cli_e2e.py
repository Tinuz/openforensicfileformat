from __future__ import annotations

import json
import subprocess
import tempfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REPORT = ROOT / "tests" / "e2e" / "cli-e2e-report.json"


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    print("+", " ".join(cmd))
    return subprocess.run(
        cmd,
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=True,
    )


def write_json(path: Path, payload: dict) -> None:
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def check_path(path: Path, checks: list[dict[str, str]], name: str) -> None:
    checks.append(
        {
            "name": name,
            "status": "PASS" if path.exists() else "FAIL",
            "detail": str(path),
        }
    )


def run_e2e() -> int:
    checks: list[dict[str, str]] = []

    with tempfile.TemporaryDirectory(prefix="offf-e2e-") as tmp:
        tmp_dir = Path(tmp)
        raw_path = tmp_dir / "e2e.raw"
        case_path = tmp_dir / "e2e.offf"
        raw_path.write_bytes((bytes(range(256)) * 8192)[:1_048_576])

        run(
            [
                "cargo",
                "run",
                "-p",
                "offf-convert",
                "--",
                "--input",
                str(raw_path),
                "--output",
                str(case_path),
                "--chunk-size",
                "256K",
                "--compression",
                "none",
                "--deterministic",
            ]
        )

        run([
            "cargo",
            "run",
            "-p",
            "offf-verify",
            "--",
            str(case_path),
            "--profile",
            "conformance",
        ])

        # Core-repo E2E scope intentionally excludes worker execution.
        check_path(case_path / "manifest.json", checks, "manifest_present")
        check_path(case_path / "acquisition.json", checks, "acquisition_present")
        check_path(case_path / "hashes" / "merkle_tree.bin", checks, "merkle_tree_present")
        check_path(
            case_path / "maps" / "physical_to_chunk.parquet",
            checks,
            "physical_to_chunk_map_present",
        )

    status = "PASS" if all(c["status"] == "PASS" for c in checks) else "FAIL"
    report = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "status": status,
        "checks": checks,
    }
    REPORT.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"Wrote E2E report to {REPORT}")

    return 0 if status == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(run_e2e())
