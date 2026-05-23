from __future__ import annotations

import json
from dataclasses import dataclass
from importlib.resources import files
from pathlib import Path
from typing import Any, Iterable

from jsonschema import Draft202012Validator


@dataclass(frozen=True)
class SchemaError:
    path: str
    message: str


class SchemaRegistry:
    def __init__(self) -> None:
        self._schemas: dict[str, dict[str, Any]] = {}
        self._load()

    def _load(self) -> None:
        base = files("offf_sdk.schemas")
        catalog = json.loads((base / "offf-schema-catalog-0.1.0.json").read_text(encoding="utf-8"))
        mapping: dict[str, str] = catalog["schemas"]
        for key, filename in mapping.items():
            self._schemas[key] = json.loads((base / filename).read_text(encoding="utf-8"))

    def validator(self, key: str) -> Draft202012Validator:
        schema = self._schemas[key]
        return Draft202012Validator(schema)


def _fmt_path(parts: Iterable[Any]) -> str:
    p = [str(x) for x in parts]
    return ".".join(p) if p else "<root>"


def validate_manifest(manifest_obj: dict[str, Any]) -> list[SchemaError]:
    v = SchemaRegistry().validator("manifest")
    return [SchemaError(path=_fmt_path(e.path), message=e.message) for e in v.iter_errors(manifest_obj)]


def validate_acquisition(acquisition_obj: dict[str, Any]) -> list[SchemaError]:
    v = SchemaRegistry().validator("acquisition")
    return [SchemaError(path=_fmt_path(e.path), message=e.message) for e in v.iter_errors(acquisition_obj)]


def validate_provenance_events(events: Iterable[dict[str, Any]]) -> list[SchemaError]:
    v = SchemaRegistry().validator("provenance_event")
    out: list[SchemaError] = []
    for idx, evt in enumerate(events):
        errs = list(v.iter_errors(evt))
        for e in errs:
            out.append(SchemaError(path=f"[{idx}].{_fmt_path(e.path)}", message=e.message))
    return out
