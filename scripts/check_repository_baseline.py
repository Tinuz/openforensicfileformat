#!/usr/bin/env python3
"""
check_repository_baseline.py

Validate Hardening Sprint 0 repository baseline expectations:
- README.md exists and is a canonical entrypoint with quickstart + architecture sections.
- README.md links to status matrix, formal spec, and schema catalog.
- docs/status.md exists and references README.md.
"""

from __future__ import annotations

import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).parent.parent
README = ROOT / "README.md"
STATUS = ROOT / "docs" / "status.md"


def require(condition: bool, message: str, failures: list[str]) -> None:
    if not condition:
        failures.append(message)


def main() -> int:
    failures: list[str] = []

    require(README.exists(), "README.md does not exist", failures)
    require(STATUS.exists(), "docs/status.md does not exist", failures)

    if README.exists():
        readme = README.read_text(encoding="utf-8")
        require(
            re.search(r"^##\s+Quickstart\s*$", readme, flags=re.MULTILINE) is not None,
            "README.md is missing the '## Quickstart' section",
            failures,
        )
        require(
            re.search(r"^##\s+Architecture Overview\s*$", readme, flags=re.MULTILINE)
            is not None,
            "README.md is missing the '## Architecture Overview' section",
            failures,
        )
        require("docs/status.md" in readme, "README.md does not link docs/status.md", failures)
        require(
            "SPEC_OFFF_Formal_Spec_v0.1.0.md" in readme,
            "README.md does not reference SPEC_OFFF_Formal_Spec_v0.1.0.md",
            failures,
        )
        require(
            "docs/schema/offf-schema-catalog-0.2.0.json" in readme,
            "README.md does not reference docs/schema/offf-schema-catalog-0.2.0.json",
            failures,
        )

    if STATUS.exists():
        status = STATUS.read_text(encoding="utf-8")
        require(
            re.search(r"^#\s+OFFF Component Status Matrix\s*$", status, flags=re.MULTILINE)
            is not None,
            "docs/status.md is missing the canonical title",
            failures,
        )
        require(
            "README.md" in status,
            "docs/status.md does not reference README.md in related documentation",
            failures,
        )

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        print(f"\n{len(failures)} repository baseline check(s) failed.", file=sys.stderr)
        return 1

    print("OK: repository baseline checks passed (README/status existence, links, and key sections).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
