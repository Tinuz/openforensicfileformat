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
        keyword_job = tmp_dir / "keyword_job.json"
        yara_job = tmp_dir / "yara_job.json"

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

        write_json(
            keyword_job,
            {
                "job_id": "job-e2e-keyword",
                "created_at": "2026-05-24T00:00:00Z",
                "case_id": "urn:offf:case:e2e",
                "task": "keyword_scan",
                "scope": {"chunks": ["*"]},
                "tool": {"name": "offf-e2e", "version": "0.1.0"},
                "parameters": {
                    "keywords": ["ABC", "XYZ"],
                    "encoding": ["utf-8"],
                },
            },
        )
        write_json(
            yara_job,
            {
                "job_id": "job-e2e-yara",
                "created_at": "2026-05-24T00:00:00Z",
                "case_id": "urn:offf:case:e2e",
                "task": "yara_scan",
                "scope": {"chunks": ["*"]},
                "tool": {"name": "offf-e2e", "version": "0.1.0"},
                "parameters": {
                    "rules_hash": "sha256:e2e",
                    "rules_inline": "rule always_true { condition: true }",
                },
            },
        )

        run(
            [
                "cargo",
                "run",
                "-p",
                "offf-keyword-worker",
                "--",
                "--case",
                str(case_path),
                "--job",
                str(keyword_job),
                "--worker-id",
                "e2e-keyword",
            ]
        )
        run(
            [
                "cargo",
                "run",
                "-p",
                "offf-yara-worker",
                "--",
                "--case",
                str(case_path),
                "--job",
                str(yara_job),
                "--worker-id",
                "e2e-yara",
            ]
        )

        check_path(
            case_path / "analysis" / "jobs" / "job-e2e-keyword" / "keyword_hits.parquet",
            checks,
            "keyword_hits_job_scoped_present",
        )
        check_path(
            case_path / "analysis" / "jobs" / "job-e2e-keyword" / "result_manifest.json",
            checks,
            "keyword_result_manifest_present",
        )
        check_path(
            case_path / "analysis" / "jobs" / "job-e2e-yara" / "yara_hits.parquet",
            checks,
            "yara_hits_job_scoped_present",
        )
        check_path(
            case_path / "analysis" / "jobs" / "job-e2e-yara" / "result_manifest.json",
            checks,
            "yara_result_manifest_present",
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
