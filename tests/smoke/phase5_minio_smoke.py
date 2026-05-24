from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BUCKET = "offf-smoke"
PREFIX = "cases/phase5-smoke.offf"
CASE_URI = f"s3://{BUCKET}/{PREFIX}"
MINIO_ENDPOINT = "http://localhost:9000"
MINIO_DOCKER_ENDPOINT = "http://host.docker.internal:9000"
AWS_REGION = "us-east-1"
AWS_ACCESS_KEY = "offfadmin"
AWS_SECRET_KEY = "offfadmin123"


def run(cmd: list[str], *, env: dict[str, str] | None = None, capture: bool = False, check: bool = True) -> subprocess.CompletedProcess[str]:
    print("+", " ".join(cmd))
    return subprocess.run(
        cmd,
        cwd=ROOT,
        env=env,
        check=check,
        text=True,
        capture_output=capture,
    )


def aws_docker_cmd(*args: str) -> list[str]:
    return [
        "docker",
        "run",
        "--rm",
        "-e",
        f"AWS_ACCESS_KEY_ID={AWS_ACCESS_KEY}",
        "-e",
        f"AWS_SECRET_ACCESS_KEY={AWS_SECRET_KEY}",
        "-e",
        f"AWS_DEFAULT_REGION={AWS_REGION}",
        "amazon/aws-cli",
        "--endpoint-url",
        MINIO_DOCKER_ENDPOINT,
        *args,
    ]


def check_minio_health() -> None:
    url = f"{MINIO_ENDPOINT}/minio/health/live"
    with urllib.request.urlopen(url, timeout=5) as response:
        if response.status != 200:
            raise RuntimeError(f"MinIO health endpoint not ready: {response.status}")


def upload_sample_case(local_container: Path) -> None:
    run(aws_docker_cmd("s3", "mb", f"s3://{BUCKET}"), check=False)
    run(
        [
            "docker",
            "run",
            "--rm",
            "-e",
            f"AWS_ACCESS_KEY_ID={AWS_ACCESS_KEY}",
            "-e",
            f"AWS_SECRET_ACCESS_KEY={AWS_SECRET_KEY}",
            "-e",
            f"AWS_DEFAULT_REGION={AWS_REGION}",
            "-v",
            f"{local_container}:/src:ro",
            "amazon/aws-cli",
            "--endpoint-url",
            MINIO_DOCKER_ENDPOINT,
            "s3",
            "sync",
            "/src",
            f"s3://{BUCKET}/{PREFIX}",
            "--delete",
        ]
    )


def make_tiny_offf_container(tmp_dir: Path) -> Path:
    raw = tmp_dir / "phase5-smoke.dd"
    container = tmp_dir / "phase5-smoke.offf"

    pattern = bytes(range(256))
    raw.write_bytes(pattern * (4 * 1024 * 1024 // len(pattern)))

    run(
        [
            "cargo",
            "run",
            "-p",
            "offf-convert",
            "--",
            "--input",
            str(raw),
            "--output",
            str(container),
            "--chunk-size",
            "1M",
            "--compression",
            "none",
            "--deterministic",
        ]
    )
    return container


def smoke_env() -> dict[str, str]:
    env = os.environ.copy()
    env["OFFF_S3_ENDPOINT"] = MINIO_ENDPOINT
    env["AWS_ACCESS_KEY_ID"] = AWS_ACCESS_KEY
    env["AWS_SECRET_ACCESS_KEY"] = AWS_SECRET_KEY
    env["AWS_REGION"] = AWS_REGION
    env["AWS_EC2_METADATA_DISABLED"] = "true"
    return env


def make_keyword_job(path: Path, job_id: str = "job-smoke-keyword") -> None:
    payload = {
        "job_id": job_id,
        "created_at": "2026-05-23T00:00:00Z",
        "case_id": "urn:offf:case:smoke",
        "task": "keyword_scan",
        "scope": {"chunks": ["*"]},
        "tool": {"name": "offf-smoke", "version": "0.1.0"},
        "parameters": {
            "keywords": ["MZ", "JPEG"],
            "encoding": ["utf-8"],
        },
    }
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def make_yara_job(path: Path, job_id: str = "job-smoke-yara") -> None:
    payload = {
        "job_id": job_id,
        "created_at": "2026-05-23T00:00:00Z",
        "case_id": "urn:offf:case:smoke",
        "task": "yara_scan",
        "scope": {"chunks": ["*"]},
        "tool": {"name": "offf-smoke", "version": "0.1.0"},
        "parameters": {
            "rules_hash": "sha256:smoke",
            "rules_inline": "rule always_true { condition: true }",
        },
    }
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def object_exists(key: str) -> None:
    run(aws_docker_cmd("s3", "ls", f"s3://{BUCKET}/{key}"))


def read_provenance_lines() -> int:
    cp = run(
        aws_docker_cmd("s3", "cp", f"s3://{BUCKET}/{PREFIX}/provenance/chain_of_custody.jsonl", "-"),
        capture=True,
    )
    return sum(1 for line in cp.stdout.splitlines() if line.strip())


def keyword_worker_exe() -> Path:
    exe = ROOT / "target" / "debug" / ("offf-keyword-worker.exe" if os.name == "nt" else "offf-keyword-worker")
    if not exe.exists():
        raise FileNotFoundError(f"keyword worker binary not found: {exe}")
    return exe


def run_concurrent_keyword_workers(env: dict[str, str], base_job_path: Path, workers: int = 4) -> None:
    exe = keyword_worker_exe()
    base_job = json.loads(base_job_path.read_text(encoding="utf-8"))
    procs: list[subprocess.Popen[str]] = []
    job_paths: list[Path] = []
    for idx in range(workers):
        job_path = base_job_path.parent / f"keyword_job_{idx}.json"
        payload = dict(base_job)
        payload["job_id"] = f"job-smoke-keyword-concurrent-{idx}"
        job_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        job_paths.append(job_path)
        cmd = [
            str(exe),
            "--case",
            CASE_URI,
            "--job",
            str(job_paths[-1]),
            "--worker-id",
            f"concurrent-{idx}",
        ]
        print("+", " ".join(cmd))
        procs.append(
            subprocess.Popen(
                cmd,
                cwd=ROOT,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
            )
        )

    failed = []
    for i, proc in enumerate(procs):
        out, _ = proc.communicate()
        if proc.returncode != 0:
            failed.append((i, out))

    if failed:
        details = "\n\n".join(f"worker {i} output:\n{out}" for i, out in failed)
        raise RuntimeError(f"concurrent keyword worker run failed\n{details}")


def main() -> int:
    if not shutil.which("docker"):
        raise RuntimeError("docker command not found")

    with tempfile.TemporaryDirectory(prefix="offf-phase5-") as tmp:
        tmp_dir = Path(tmp)
        check_minio_health()
        local_container = make_tiny_offf_container(tmp_dir)
        upload_sample_case(local_container)

        env = smoke_env()

        run(["cargo", "run", "-p", "offf-verify", "--", CASE_URI], env=env)

        keyword_job = tmp_dir / "keyword_job.json"
        yara_job = tmp_dir / "yara_job.json"
        make_keyword_job(keyword_job)
        make_yara_job(yara_job)

        run(
            [
                "cargo",
                "run",
                "-p",
                "offf-keyword-worker",
                "--",
                "--case",
                CASE_URI,
                "--job",
                str(keyword_job),
                "--worker-id",
                "smoke-keyword",
            ],
            env=env,
        )
        run(
            [
                "cargo",
                "run",
                "-p",
                "offf-yara-worker",
                "--",
                "--case",
                CASE_URI,
                "--job",
                str(yara_job),
                "--worker-id",
                "smoke-yara",
            ],
            env=env,
        )

        object_exists(f"{PREFIX}/analysis/jobs/job-smoke-keyword/keyword_hits.parquet")
        object_exists(f"{PREFIX}/analysis/jobs/job-smoke-keyword/result_manifest.json")
        object_exists(f"{PREFIX}/analysis/jobs/job-smoke-yara/yara_hits.parquet")
        object_exists(f"{PREFIX}/analysis/jobs/job-smoke-yara/result_manifest.json")

        run(["cargo", "build", "-p", "offf-keyword-worker"], env=env)
        before = read_provenance_lines()
        run_concurrent_keyword_workers(env, keyword_job, workers=4)
        after = read_provenance_lines()

        delta = after - before
        if delta != 4:
            raise RuntimeError(
                f"concurrent provenance append mismatch: expected +4 lines, observed +{delta}"
            )

    print("Phase 5 smoke PASSED")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:  # noqa: BLE001
        print(f"Phase 5 smoke FAILED: {exc}", file=sys.stderr)
        raise
