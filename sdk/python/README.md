# OFFF Python SDK

## Purpose
The Python SDK provides a practical API for common OFFF operations:
- open containers
- read manifests
- verify containers and chunks
- map offsets to chunks
- write analysis outputs
- append provenance events

## Installation
1. Activate a Python environment.
2. Install dependencies via `pyproject.toml`.
3. Install the package in editable mode:

```bash
pip install -e sdk/python
```

## Quick Start
```python
from offf_sdk.api import open_container, verify_container

c = open_container("tests/samples/4orensics.case2.offf")
result = verify_container(c)
print(result)
```

## More Example Commands
```bash
cd sdk/python
python -m unittest tests/test_api_contract.py -v
python -m unittest tests/test_container_chunk_reader.py -v
```

## Tests
- Contract test: `sdk/python/tests/test_api_contract.py`
- Run locally:

```bash
cd sdk/python
python -m unittest tests/test_api_contract.py -v
```

## Key Modules
- `offf_sdk/api.py`: public API functions
- `offf_sdk/container.py`: core logic for read/verify/write operations
- `offf_sdk/schema_validation.py`: schema validation helpers
