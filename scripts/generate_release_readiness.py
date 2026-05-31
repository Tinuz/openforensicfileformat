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

RELEASE_1_0_BACKLOG = [
    {
        "priority": "P0",
        "title": "Freeze 1.0 scope and gate metadata",
        "components": [
            "conformance-suite",
            "extension-model",
            "offf-access-service",
            "offf-annotate",
            "offf-collect",
            "offf-convert",
            "offf-export",
            "offf-index",
            "offf-jobs",
            "offf-keyword-worker",
            "offf-yara-worker",
            "packed-container",
            "python-sdk",
            "go-sdk",
            "tool-registry",
            "worker-runtime-state",
        ],
        "files": [
            "components.toml",
            "docs/status.md",
            "docs/maturity-model.md",
            "docs/component-classification.md",
            "docs/test-traceability.md",
            "docs/evidence-of-done.md",
            "README.md",
            ".github/workflows/offf-ci.yml",
            "scripts/check_component_metadata.py",
            "scripts/check_test_traceability.py",
            "scripts/generate_release_readiness.py",
        ],
        "tests": [
            "python scripts/check_component_metadata.py",
            "python scripts/check_test_traceability.py",
            "python scripts/generate_release_readiness.py",
        ],
        "acceptance": "1.0 scope is explicit, the release gate is reproducible, and the readiness report is authoritative.",
    },
    {
        "priority": "P1",
        "title": "Promote stable reference path components",
        "components": [
            "conformance-suite",
            "extension-model",
            "offf-convert",
            "offf-export",
            "offf-index",
            "offf-jobs",
            "packed-container",
        ],
        "files": [
            "crates/offf-core/src/extensions.rs",
            "crates/offf-core/src/packed.rs",
            "crates/offf-convert/src/main.rs",
            "crates/offf-export/src/main.rs",
            "crates/offf-index/src/main.rs",
            "crates/offf-jobs/src/main.rs",
            "docs/conformance-profiles.md",
            "docs/test-traceability.md",
        ],
        "tests": [
            "cargo test -p offf-core",
            "cargo test -p offf-convert",
            "cargo test -p offf-export",
            "cargo test -p offf-index",
            "cargo test -p offf-jobs",
            "cargo test -p offf-integration-tests",
            "python tests/conformance/run_conformance.py",
        ],
        "acceptance": "Production/reference path components can be defended as release-stable or explicitly fenced with documented limits.",
    },
    {
        "priority": "P2",
        "title": "Fence experimental production surfaces",
        "components": [
            "offf-access-service",
            "offf-annotate",
            "offf-collect",
            "offf-keyword-worker",
            "offf-yara-worker",
        ],
        "files": [
            "crates/offf-access-service/src/main.rs",
            "crates/offf-annotate/src/main.rs",
            "crates/offf-collect/src/main.rs",
            "crates/offf-keyword-worker/src/main.rs",
            "crates/offf-yara-worker/src/main.rs",
            "docs/object-content-ref.md",
            "docs/filesystem-to-object-graph.md",
            "docs/conformance-profiles.md",
        ],
        "tests": [
            "cargo test -p offf-access-service",
            "cargo test -p offf-annotate",
            "cargo test -p offf-collect",
            "cargo test -p offf-keyword-worker",
            "cargo test -p offf-yara-worker",
            "python tests/e2e/run_cli_e2e.py",
        ],
        "acceptance": "Experimental surfaces are either promoted with evidence or clearly fenced from the 1.0 guarantee.",
    },
    {
        "priority": "P3",
        "title": "Stabilize SDK and governance surfaces",
        "components": [
            "python-sdk",
            "go-sdk",
            "tool-registry",
            "worker-runtime-state",
        ],
        "files": [
            "sdk/python/offf_sdk/container.py",
            "sdk/python/offf_sdk/api.py",
            "sdk/python/tests/test_api_contract.py",
            "sdk/python/tests/test_container_chunk_reader.py",
            "sdk/go/sdk.go",
            "sdk/go/sdk_test.go",
            "config/tool-registry.example.json",
            "docs/reference-worker-runtime.md",
        ],
        "tests": [
            "python -m unittest sdk/python/tests/test_api_contract.py sdk/python/tests/test_container_chunk_reader.py",
            "go test ./...",
        ],
        "acceptance": "SDKs and governance metadata have a stable, tested minimum contract for 1.0 consumers.",
    },
]


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

        in_scope_1_0 = bool(meta.get("release_1_0", False))
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
            "backlog": RELEASE_1_0_BACKLOG,
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
        "Scope policy: only components with `release_1_0 = true` in `components.toml` are part of the first 1.0 stability promise. Other components remain available but are explicitly out of scope until promoted.",
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

    lines += [
        "",
        "## 1.0 backlog",
        "",
        "The backlog below is ordered by delivery priority, not by component size.",
        "",
    ]
    for item in RELEASE_1_0_BACKLOG:
        lines += [
            f"### {item['priority']} — {item['title']}",
            "",
            f"**Components:** {', '.join(item['components'])}",
            "",
            "**Minimal files:**",
            "",
        ]
        for path in item["files"]:
            lines.append(f"- {path}")
        lines += ["", "**Minimal tests:**", ""]
        for test in item["tests"]:
            lines.append(f"- {test}")
        lines += ["", f"**Acceptance:** {item['acceptance']}", ""]

    lines += ["", "---", "", f"*Generated by `scripts/generate_release_readiness.py`*", ""]

    md_content = "\n".join(lines)
    MD_OUT.write_text(md_content, encoding="utf-8")

    print(md_content)
    print(f"\nWrote {JSON_OUT} and {MD_OUT}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
