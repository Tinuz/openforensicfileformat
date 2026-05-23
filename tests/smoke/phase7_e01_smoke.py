from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def run(cmd: list[str], *, env: dict[str, str] | None = None, check: bool = True, capture: bool = False) -> subprocess.CompletedProcess[str]:
    print("+", " ".join(cmd))
    return subprocess.run(
        cmd,
        cwd=ROOT,
        env=env,
        check=check,
        text=True,
        capture_output=capture,
    )


def build_env() -> dict[str, str]:
    env = os.environ.copy()
    user_profile = env.get("USERPROFILE")
    if user_profile:
        user_bin = str(Path(user_profile) / "bin")
        current = env.get("PATH", "")
        parts = current.split(os.pathsep) if current else []
        if user_bin not in parts:
            env["PATH"] = current + (os.pathsep if current else "") + user_bin
    return env


def make_raw_sample(path: Path, size: int = 2 * 1024 * 1024) -> None:
    pattern = bytes(range(256))
    repeats = size // len(pattern)
    remainder = size % len(pattern)
    data = pattern * repeats + pattern[:remainder]
    path.write_bytes(data)


def generate_e01(raw_path: Path, out_prefix: Path) -> Path:
    run(
        [
            "docker",
            "run",
            "--rm",
            "-v",
            f"{raw_path.parent}:/work",
            "--entrypoint",
            "ewfacquire",
            "offf/ewf-tools:latest",
            "-u",
            "-q",
            "-f",
            "encase6",
            "-t",
            f"/work/{out_prefix.name}",
            f"/work/{raw_path.name}",
        ]
    )

    candidates = sorted(list(raw_path.parent.glob(f"{out_prefix.name}.E*")) + list(raw_path.parent.glob(f"{out_prefix.name}.e*")))
    if not candidates:
        raise RuntimeError("E01 generation succeeded but no segment file was found")
    return candidates[0]


def main() -> int:
    env = build_env()

    if not shutil.which("docker"):
        raise RuntimeError("docker command not found")
    if shutil.which("ewfexport", path=env.get("PATH")) is None:
        raise RuntimeError("ewfexport command not found in PATH")

    with tempfile.TemporaryDirectory(prefix="offf-phase7-") as tmp:
        tmp_dir = Path(tmp)
        raw_path = tmp_dir / "sample.dd"
        e01_prefix = tmp_dir / "sample_evidence"
        out_container = tmp_dir / "phase7.offf"

        make_raw_sample(raw_path)
        e01_path = generate_e01(raw_path, e01_prefix)

        run(
            [
                "cargo",
                "run",
                "-p",
                "offf-convert",
                "--",
                "--input",
                str(e01_path),
                "--output",
                str(out_container),
                "--input-type",
                "e01",
                "--ewf-export-tool",
                "ewfexport",
            ],
            env=env,
        )

        acquisition = json.loads((out_container / "acquisition.json").read_text(encoding="utf-8"))
        source_container = acquisition.get("source_container") or {}
        evidence_stream = acquisition.get("evidence_stream") or {}

        if source_container.get("type") != "E01":
            raise RuntimeError("acquisition.json source_container.type is not E01")
        if not source_container.get("container_sha256"):
            raise RuntimeError("acquisition.json source_container.container_sha256 missing")
        if source_container.get("tool_used") != "ewfexport":
            raise RuntimeError("acquisition.json source_container.tool_used is not ewfexport")
        if not evidence_stream.get("stream_sha256"):
            raise RuntimeError("acquisition.json evidence_stream.stream_sha256 missing")

        run(["cargo", "run", "-p", "offf-verify", "--", str(out_container)], env=env)

    print("Phase 7 smoke PASSED")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:  # noqa: BLE001
        print(f"Phase 7 smoke FAILED: {exc}", file=sys.stderr)
        raise
