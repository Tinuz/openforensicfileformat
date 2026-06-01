#!/usr/bin/env python3
"""
check_threat_model.py

Validate that docs/threat-model.md keeps a minimum, machine-checkable structure:
- Required top-level sections exist.
- Threat entries T-01 through T-09 all exist.
- Every threat entry includes mitigation and residual-risk fields.
- Threat model keeps explicit test-traceability markers.
"""

from __future__ import annotations

import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).parent.parent
THREAT_MODEL = ROOT / "docs" / "threat-model.md"

REQUIRED_HEADINGS = [
    "## Scope",
    "## Assets",
    "## Threat vectors and mitigations",
]


def main() -> int:
    if not THREAT_MODEL.exists():
        print(f"FAIL: missing {THREAT_MODEL}", file=sys.stderr)
        return 1

    text = THREAT_MODEL.read_text(encoding="utf-8")
    failures: list[str] = []

    for heading in REQUIRED_HEADINGS:
        if heading not in text:
            failures.append(f"missing required heading: {heading}")

    threat_pattern = re.compile(r"^###\s+(T-\d{2}):", flags=re.MULTILINE)
    found_ids = [m.group(1) for m in threat_pattern.finditer(text)]
    expected_ids = [f"T-{i:02d}" for i in range(1, 10)]

    missing_ids = [tid for tid in expected_ids if tid not in found_ids]
    if missing_ids:
        failures.append(f"missing threat section(s): {', '.join(missing_ids)}")

    sections = re.split(r"(?m)^###\s+T-\d{2}:", text)
    # First split chunk is preface; each remaining chunk corresponds to a threat section.
    threat_sections = sections[1:]
    if len(threat_sections) < 9:
        failures.append("could not parse all threat sections")
    else:
        for idx, section in enumerate(threat_sections, start=1):
            tid = f"T-{idx:02d}"
            if "**Mitigation:**" not in section:
                failures.append(f"{tid} missing '**Mitigation:**' field")
            if "**Residual risk" not in section:
                failures.append(f"{tid} missing '**Residual risk' field")

    if "**Test evidence:**" not in text:
        failures.append("missing '**Test evidence:**' markers for threat-to-test traceability")

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        print(f"\n{len(failures)} threat model check(s) failed.", file=sys.stderr)
        return 1

    print("OK: threat model structure checks passed (sections, T-01..T-09, mitigation/residual, test evidence markers).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
