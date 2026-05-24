OFFF Conformance Suite (Initial Scaffold)
========================================

This folder contains the OFFF conformance scaffold used to validate profile-level expectations.

Current scope
-------------

- Reader profile baseline checks on sample containers.
- Analysis profile baseline checks for append-only result/provenance artifacts.
- Indexer profile baseline checks for filesystem index presence.
- Acquisition profile baseline checks for acquisition/manifest required keys.
- Negative dataset scenarios (expected-fail verification).
- Machine-readable JSON report output.

Profiles explained
------------------

- Reader profile
  - Verifies container readability and required reader artefacts.
- Analysis profile
  - Verifies expected analysis outputs and provenance baseline.
- Indexer profile
  - Verifies filesystem index structure and file index presence.
- Acquisition profile
  - Verifies required acquisition and manifest keys.

Negative scenarios
------------------

The suite includes expected-fail scenarios, for example:
- missing manifest
- empty provenance log

Negative cases are defined in:

tests/conformance/negative_cases.json

Each negative scenario records expected vs observed status and marks PASS only if failure is correctly detected.

Run
---

From repository root:

python tests/conformance/run_conformance.py

Optional: fail-fast in CI by checking exit code only.

Output
------

A JSON report is written to:

tests/conformance/conformance-report.json

Interpreting results
--------------------

- A profile status is PASS only when all checks in that profile pass.
- The script returns non-zero when one or more profiles fail or a negative scenario does not fail as expected.

Report shape
------------

{
  "offf_version": "0.1.0",
  "generated_at": "ISO-8601",
  "profiles": {
    "reader": {"status": "PASS|FAIL", "checks": [...]},
    "analysis": {"status": "PASS|FAIL", "checks": [...]},
    "indexer": {"status": "PASS|FAIL", "checks": [...]},
    "acquisition": {"status": "PASS|FAIL", "checks": [...]}
  },
  "negative_tests": [
    {
      "name": "missing_manifest",
      "expected_profile": "reader",
      "expected_status": "FAIL",
      "observed_status": "FAIL",
      "status": "PASS"
    }
  ]
}

Maintenance notes
-----------------

- Keep report schema stable for CI consumers.
- Add new checks with clear names and deterministic details.
- When adding a new OFFF profile, extend both runner and report structure in one change.
- When adding negative scenarios, update tests/conformance/negative_cases.json instead of hardcoding cases in the runner.
