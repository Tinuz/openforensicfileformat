# Test Samples

## Purpose
This directory contains sample data used by OFFF tooling and test suites.

## Contents
- sample OFFF container(s)
- job manifests for keyword and YARA workers
- YARA rule files
- compressed or unpacked reference cases

## Usage
- Use samples in smoke and conformance tests for reproducibility.
- Do not modify sample files without impact analysis on existing tests.

## Example Commands
```bash
python tests/conformance/run_conformance.py
cargo test -p offf-integration-tests -- --nocapture
```

## Guidelines
- Do not include sensitive data.
- Keep file names stable so CI scripts do not break.
- Add a short description here when introducing new sample files.
