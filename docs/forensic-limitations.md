# OFFF Forensic Limitations

## Purpose

This document enumerates the known limitations of OFFF as a forensic format and ecosystem.
Understanding these limitations is essential for any organisation that considers using OFFF
in a formal forensic context.

Limitations listed here are not defects. They are inherent properties of the acquisition
mode, the technical model, or the current maturity level of specific components. Some
limitations will be addressed in future versions of the specification; others are fundamental
to the nature of digital forensics and cannot be resolved by any format.

---

## 1. Limitations Common to All Acquisition Modes

### 1.1 Hash-based integrity does not prove origin

SHA-256 hashes prove that data has not changed since the hash was computed. They do not prove:
- When the data was originally created.
- Who created the data.
- Whether the data was present on the device before the acquisition.
- Whether the acquisition environment was free from contamination.

### 1.2 Timestamp trustworthiness

Timestamps recorded in OFFF (in `acquisition.json`, `provenance_events.jsonl`, etc.) are
taken from the system clock of the acquisition machine. If that clock was inaccurate or was
tampered with, the timestamps may be unreliable. OFFF does not independently verify the
source of timestamps.

### 1.3 Tool identity is not tool certification

OFFF records which tool performed an operation (via `tool.name`, `tool.version`, and in
JWT mode, a cryptographic token). Recording the tool name does not:
- Certify that the tool was unmodified.
- Certify that the tool was validated or accredited.
- Prevent a malicious actor from reporting a false tool name in `dev_headers` auth mode.

In `jwt` auth mode, tool identity is cryptographically bound to a signed token, which
significantly raises the bar for identity falsification. In `dev_headers` mode, tool
identity is self-asserted.

### 1.4 Pre-acquisition contamination is invisible to OFFF

OFFF records what was present in the evidence source at the time of acquisition. If data was
added, modified, or deleted on the source before acquisition began, OFFF cannot detect or
record this. OFFF integrity guarantees begin at the moment of acquisition.

### 1.5 No protection against full container replacement

OFFF cryptographic integrity protects against modification of the container contents after
finalization. It does not protect against an adversary who replaces the entire container
(and its verification report) with a crafted substitute. Container-level authentication
(signing the container root hash with an external key) is outside the current OFFF specification.

---

## 2. Limitations of block_image Acquisitions

### 2.1 Logical interpretation requires additional tooling

A block image acquisition records raw sector data. Extracting files, metadata, and user data
requires file system parsing tools (e.g., `offf-index` with the NTFS parser). If file system
parsing fails or is incomplete:
- Some files may not appear in the object index.
- File names, timestamps, and attributes may be absent or incorrect.
- OFFF cannot guarantee the completeness of the file index from a block image.

### 2.2 NTFS parser is experimental

The NTFS parser in `offf-core/src/ntfs.rs` and `offf-index` is classified `experimental`.
It handles common NTFS structures but may fail or produce incomplete results on:
- Damaged or corrupted NTFS volumes.
- NTFS volumes with unusual configurations.
- NTFS extended attributes and ADS (alternate data streams) are partially supported.

**Guidance:** For production forensic analysis of NTFS images, supplement OFFF indexing
with a validated NTFS forensic tool.

### 2.3 Encrypted volumes are opaque

OFFF records encrypted blocks as raw bytes. Without decryption keys or a decryption
capability, the content of encrypted volumes is not accessible. OFFF does not record
whether a volume was encrypted at the time of acquisition.

---

## 3. Limitations of file_collection Acquisitions

### 3.1 Completeness depends entirely on collection tool

A `file_collection` acquisition contains only the files that the collection tool chose to
include. OFFF cannot verify:
- That all relevant files were included.
- That no relevant files were excluded by filter, error, or design.
- That the collection scope matches the intended investigative scope.

The only record of what was excluded is the skipped events and exclusion sets recorded by
the collection tool. If the collection tool does not record these, the omission is invisible.

### 3.2 No sector-level integrity

`file_collection` acquisitions do not contain raw disk sectors. Deleted files, unallocated
space, and file slack are not captured. The integrity chain covers only the collected files.

### 3.3 File system metadata fidelity

File system metadata (creation time, modification time, access time, owner, permissions)
in a `file_collection` depends on what the collection tool preserved. OFFF records what
was provided; it cannot independently verify that the metadata matches the original device.

---

## 4. Limitations of logical_extraction and api_export Acquisitions

### 4.1 Source device integrity is unverifiable

For `logical_extraction` (e.g., mobile device extraction) and `api_export` (e.g., cloud
service export), OFFF records the data as received from the extraction or export tool.
OFFF cannot verify:
- That the extraction tool received all data from the device or service.
- That the device or service had not modified data before extraction.
- That the extraction was complete at the time of export.

### 4.2 No physical layer binding

These modes lack a sector-level hash and Merkle root. The integrity chain begins at the
object level (per-file or per-item hashes), not at the physical storage level.

---

## 5. Limitations of Analysis Results

### 5.1 Analysis results are not evidence; they are derived findings

An analysis result (keyword hit, YARA match, classification label, OCR text) is the output
of an analysis tool applied to evidence. The result is only as reliable as:
- The quality and configuration of the analysis tool.
- The completeness of the input scope.
- The accuracy of the underlying algorithm or rule set.

OFFF records and integrity-protects the analysis output. It does not validate the correctness
or completeness of the analysis.

### 5.2 YARA and keyword workers are experimental

`offf-keyword-worker` and `offf-yara-worker` are classified `experimental`. Known gaps:
- Cross-chunk boundary matching is not conformance-tested.
- Job output isolation hardening is pending.
- False positive/negative rates depend on rule or keyword quality.

### 5.3 Demo workers are not production tools

The Docker demo workers (Tika, Elasticsearch, unsupervised classifier) are classified
`demo-only`. They:
- Use simplified or synthetic containers.
- Are not covered by OFFF conformance tests.
- Must not be cited as production forensic analysis.

---

## 6. Limitations of the Access Service

### 6.1 JWT/mTLS security has not been independently reviewed

The JWT authentication mode in `offf-access-service` implements HMAC-SHA256 signed tokens.
This implementation has received internal review (as of sprint threat model 0.1.1) but has
not been externally or independently security-reviewed.

**Guidance:** Before using the access service in a production forensic environment, an
independent security review is strongly recommended.

### 6.2 S3 backend is smoke-tested only

MinIO/S3 backend support is `experimental`. It has been smoke-tested but not subjected to
the full conformance test suite.

---

## 7. Limitations of the Packed Container Format

The `.offfpack` format is a transport representation, not a canonical OFFF format. Its
limitations:
- Verification of a packed container requires unpacking first.
- Packing and unpacking add steps that may introduce errors.
- Not all OFFF verifier profiles support packed containers directly.

---

## 8. Maturity Limitations

The following components are not yet `forensic-grade` and carry additional limitations:

| Component | Maturity | Key limitation |
|---|---|---|
| `offf-core` NTFS parser | experimental | Incomplete NTFS coverage |
| `offf-index` object index rebuild | experimental | `--from-events` mode untested at scale |
| `offf-collect` | experimental | No dedicated file_collection integration tests |
| `offf-access-service` | experimental | No independent security review |
| `offf-keyword-worker` | experimental | Cross-chunk boundary matching not tested |
| `offf-yara-worker` | experimental | Job output isolation pending |
| Python SDK | experimental | v0.2 API parity incomplete |
| Go SDK | experimental | No lineage/extension API |
| Worker runtime state | experimental | Not covered by OFFF conformance |

See `docs/status.md` for the full status matrix.

---

## Related Documents

- `docs/chain-of-evidence.md` — what the technical chain does and does not prove
- `docs/chain-of-custody.md` — what the custody record does and does not prove
- `docs/legal-neutrality.md` — scope of OFFF legal neutrality
- `docs/evidence-root-model.md` — per-mode evidence root properties
- `docs/forensic-baseline-profile.md` — required elements for baseline conformance
- `docs/status.md` — current maturity status per component

*Last updated: 2026-05-29*
