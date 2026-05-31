#!/usr/bin/env python3
"""
generate_release_readiness.py
Read components.toml and emit a release readiness report.

Outputs:
  reports/release-readiness.json   — machine-readable summary
  reports/release-readiness.md     — human-readable summary (printed to stdout too)

Exit codes:
  0 — report generated successfully
  1 — could not read components.toml or reports/ could not be created
"""

import json
import sys
import datetime
import pathlib
import tomllib

ROOT = pathlib.Path(__file__).parent.parent
COMPONENTS_TOML = ROOT / "components.toml"
REPORTS_DIR = ROOT / "reports"
JSON_OUT = REPORTS_DIR / "release-readiness.json"
MD_OUT = REPORTS_DIR / "release-readiness.md"

MATURITY_RANK = {
    "demo-only": 0,
    "experimental": 1,
    "reference": 2,
    "forensic-grade-candidate": 3,
    "forensic-grade": 4,
}

FORENSIC_READY = {"forensic-grade-candidate", "forensic-grade"}
RELEASE_1_0_IN_SCOPE_CLASSIFICATIONS = {"core", "reference"}


def main() -> int:
    if not COMPONENTS_TOML.exists():
        print(f"ERROR: {COMPONENTS_TOML} not found", file=sys.stderr)
        return 1

    with COMPONENTS_TOML.open("rb") as fh:
        data = tomllib.load(fh)

    components = data.get("components", {})
    if not components:
        print("ERROR: no components found in components.toml", file=sys.stderr)
        return 1

    # Aggregate stats
    by_classification: dict[str, list[str]] = {}
    by_maturity: dict[str, list[str]] = {}
    gaps: list[dict] = []
    forensic_ready: list[str] = []
    not_forensic_ready: list[dict] = []
    release_1_0_ready: list[str] = []
    release_1_0_blockers: list[dict] = []
    release_1_0_out_of_scope: list[str] = []

    for name, meta in components.items():
        cls = meta.get("classification", "unknown")
        mat = meta.get("maturity", "unknown")

        by_classification.setdefault(cls, []).append(name)
        by_maturity.setdefault(mat, []).append(name)

        tests_val = meta.get("tests", "")
        docs_val = meta.get("docs", "")

        if not tests_val or tests_val in ("–", ""):
            gaps.append({"component": name, "type": "missing-tests"})
        if not docs_val or docs_val in ("–", ""):
            gaps.append({"component": name, "type": "missing-docs"})

        if mat in FORENSIC_READY:
            forensic_ready.append(name)
        else:
            not_forensic_ready.append({"component": name, "maturity": mat})

        in_scope_1_0 = cls in RELEASE_1_0_IN_SCOPE_CLASSIFICATIONS
        tests_missing = not tests_val or tests_val in ("–", "")
        docs_missing = not docs_val or docs_val in ("–", "")

        if in_scope_1_0 and mat in FORENSIC_READY and not tests_missing and not docs_missing:
            release_1_0_ready.append(name)
        elif in_scope_1_0:
            blockers: list[str] = []
            if mat not in FORENSIC_READY:
                blockers.append(f"maturity={mat}")
            if tests_missing:
                blockers.append("missing-tests")
            if docs_missing:
                blockers.append("missing-docs")
            release_1_0_blockers.append({"component": name, "blockers": blockers})
        else:
            release_1_0_out_of_scope.append(name)

    generated_at = datetime.datetime.now(datetime.timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%SZ"
    )

    report = {
        "generated_at": generated_at,
        "total_components": len(components),
        "by_classification": {k: sorted(v) for k, v in sorted(by_classification.items())},
        "by_maturity": {k: sorted(v) for k, v in sorted(by_maturity.items())},
        "forensic_ready": sorted(forensic_ready),
        "not_forensic_ready": sorted(not_forensic_ready, key=lambda x: x["component"]),
        "gaps": gaps,
        "gap_count": len(gaps),
        "release_1_0": {
            "scope_classifications": sorted(RELEASE_1_0_IN_SCOPE_CLASSIFICATIONS),
            "ready": sorted(release_1_0_ready),
            "blockers": sorted(release_1_0_blockers, key=lambda x: x["component"]),
            "out_of_scope": sorted(release_1_0_out_of_scope),
            "ready_count": len(release_1_0_ready),
            "blocker_count": len(release_1_0_blockers),
            "out_of_scope_count": len(release_1_0_out_of_scope),
        },
    }

    REPORTS_DIR.mkdir(exist_ok=True)

    JSON_OUT.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")

    # Build markdown report
    lines: list[str] = [
        "# OFFF Release Readiness Report",
        "",
        f"Generated: {generated_at}",
        "",
        f"**Total components:** {len(components)}",
        "",
        "## Classification breakdown",
        "",
        "| Classification | Count | Components |",
        "|---|---|---|",
    ]
    for cls, names in sorted(by_classification.items()):
        lines.append(f"| {cls} | {len(names)} | {', '.join(sorted(names))} |")

    lines += [
        "",
        "## Maturity breakdown",
        "",
        "| Maturity | Count | Components |",
        "|---|---|---|",
    ]
    for mat in ["forensic-grade", "forensic-grade-candidate", "reference", "experimental", "demo-only"]:
        names = by_maturity.get(mat, [])
        if names:
            lines.append(f"| {mat} | {len(names)} | {', '.join(sorted(names))} |")

    lines += [
        "",
        f"## Forensic-ready components ({len(forensic_ready)})",
        "",
    ]
    for name in sorted(forensic_ready):
        mat = components[name].get("maturity", "?")
        lines.append(f"- **{name}** (`{mat}`)")

    if not_forensic_ready:
        lines += [
            "",
            f"## Not yet forensic-ready ({len(not_forensic_ready)})",
            "",
            "| Component | Current maturity | Gap to forensic-grade-candidate |",
            "|---|---|---|",
        ]
        for item in sorted(not_forensic_ready, key=lambda x: x["component"]):
            mat = item["maturity"]
            rank = MATURITY_RANK.get(mat, -1)
            fgc_rank = MATURITY_RANK["forensic-grade-candidate"]
            levels_away = fgc_rank - rank
            gap_str = f"{levels_away} maturity level(s)" if levels_away > 0 else "already eligible"
            lines.append(f"| {item['component']} | {mat} | {gap_str} |")

    if gaps:
        lines += [
            "",
            f"## Metadata gaps ({len(gaps)})",
            "",
            "| Component | Gap type |",
            "|---|---|",
        ]
        for gap in gaps:
            lines.append(f"| {gap['component']} | {gap['type']} |")
    else:
        lines += ["", "## Metadata gaps", "", "None — all components have tests and docs listed."]

    lines += [
        "",
        "## 1.0 readiness",
        "",
        "Scope policy: core and reference components are in scope for the first 1.0 release; demo, experimental, and legacy components are out of scope unless promoted.",
        "",
        f"**Ready:** {len(release_1_0_ready)}",
        f"**Blockers:** {len(release_1_0_blockers)}",
        f"**Out of scope:** {len(release_1_0_out_of_scope)}",
        "",
    ]
    if release_1_0_ready:
        lines += ["### Ready components", ""]
        for name in sorted(release_1_0_ready):
            lines.append(f"- **{name}** (`{components[name].get('maturity', '?')}`)")
        lines.append("")
    if release_1_0_blockers:
        lines += [
            "### Blockers",
            "",
            "| Component | Blockers |",
            "|---|---|",
        ]
        for item in sorted(release_1_0_blockers, key=lambda x: x["component"]):
            lines.append(f"| {item['component']} | {', '.join(item['blockers'])} |")
        lines.append("")
    if release_1_0_out_of_scope:
        lines += ["### Out of scope", ""]
        for name in sorted(release_1_0_out_of_scope):
            lines.append(f"- {name}")

    lines += ["", "---", "", f"*Generated by `scripts/generate_release_readiness.py`*", ""]

    md_content = "\n".join(lines)
    MD_OUT.write_text(md_content, encoding="utf-8")

    print(md_content)
    print(f"\nWrote {JSON_OUT} and {MD_OUT}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
