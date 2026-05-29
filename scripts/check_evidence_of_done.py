#!/usr/bin/env python3
"""
check_evidence_of_done.py
Verify that every completed backlog item (- [x] + Status: done) in BACKLOG.txt
has a matching section in docs/evidence-of-done.md.

Exit codes:
  0 — all done items have evidence-of-done sections
  1 — one or more done items lack evidence-of-done sections
"""

import re
import sys
import pathlib

ROOT = pathlib.Path(__file__).parent.parent
BACKLOG = ROOT / "BACKLOG.txt"
EVIDENCE = ROOT / "docs" / "evidence-of-done.md"

# Patterns for section identifiers that link backlog items to evidence-of-done sections.
# The order matters: more specific patterns are checked first.
SPRINT_PATTERN = re.compile(
    r"^-\s+\[x\]\s+"
    r"((?:Hardening\s+Sprint\s+\d+|Lineage\s+Sprint\s+\d+|Sprint\s+\d+|P[0-9]+)\b)",
    re.IGNORECASE,
)
GENERAL_PATTERN = re.compile(r"^-\s+\[x\]\s+(.+?)(?:\s*\(P\d+\))?$")


def extract_done_items(text: str) -> list[dict]:
    """
    Parse BACKLOG.txt and return a list of done items.

    A done item is defined as a `- [x] ...` line followed (within the next 10
    indented lines) by a line matching `Status: done`.
    """
    lines = text.splitlines()
    done_items: list[dict] = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if re.match(r"^-\s+\[x\]", line):
            # Check if this item has Status: done in the following indented block
            title = line.lstrip("- [x]").strip()
            # Remove trailing (Px) classification suffix
            title = re.sub(r"\s*\(P\d+\)\s*$", "", title).strip()
            has_done_status = False
            j = i + 1
            while j < len(lines) and j < i + 20:
                sub = lines[j]
                if re.match(r"^\s+-\s+Status:\s+done", sub, re.IGNORECASE):
                    has_done_status = True
                    break
                # Stop at next top-level item
                if re.match(r"^-\s+\[", sub):
                    break
                j += 1
            if has_done_status:
                done_items.append({"title": title, "line": i + 1})
        i += 1
    return done_items


def extract_evidence_sections(text: str) -> list[str]:
    """
    Return all ## section headings from evidence-of-done.md, normalised to lowercase.
    """
    return [
        line.lstrip("# ").strip().lower()
        for line in text.splitlines()
        if line.startswith("## ")
    ]


def make_search_key(title: str) -> str:
    """
    Derive a normalised search key from a backlog item title.
    The key is used for fuzzy matching against evidence-of-done section headings.
    """
    # Normalise separators: colon → em-dash, multiple spaces → single
    key = title.lower()
    key = re.sub(r"\s*:\s*", " — ", key, count=1)
    key = re.sub(r"\s+", " ", key).strip()
    return key


def sprint_prefix(title: str) -> str | None:
    """
    Extract a short sprint identifier from the title, e.g. 'hardening sprint 0',
    'lineage sprint 9', 'sprint 13', 'demo sprint 14', 'p1', 'p2'.
    """
    m = re.match(
        r"(?:\w+\s+)?(hardening\s+sprint\s+\d+|lineage\s+sprint\s+\d+|sprint\s+\d+|p[0-9]+)\b",
        title.lower(),
    )
    if m:
        return m.group(1)
    # Direct numeric sprint prefix anywhere in title
    m2 = re.search(r"\bsprint\s+(\d+)\b", title.lower())
    if m2:
        return f"sprint {m2.group(1)}"
    return None


# Common English/Dutch stop words to exclude from keyword matching
_STOPWORDS = frozenset({
    "the", "and", "for", "with", "from", "that", "this", "are", "has", "have",
    "been", "into", "offf", "test", "phase", "sprint", "initial", "demo",
    "minimal", "basic", "simple", "implementation", "support", "naar", "van",
    "een", "met", "voor", "wordt", "worden",
})


def keyword_tokens(title: str) -> list[str]:
    """Extract significant lowercase keywords (≥ 4 chars, not stopwords) from title."""
    words = re.findall(r"[a-z0-9]+", title.lower())
    return [w for w in words if len(w) >= 4 and w not in _STOPWORDS]


def item_matches_any_section(title: str, sections: list[str]) -> bool:
    """
    Return True if the backlog item title matches at least one evidence-of-done section.

    Matching strategy (in order):
    1. Sprint prefix match (e.g. 'sprint 14' in section heading).
    2. First 3 significant keywords all appear in the same section heading.
    3. Any single highly distinctive keyword (≥ 6 chars) appears in a section heading.
    """
    # Strategy 1: sprint/phase prefix
    prefix = sprint_prefix(title)
    if prefix:
        for section in sections:
            if prefix in section:
                return True

    # Strategy 2: first 3 significant keywords all present in same section
    tokens = keyword_tokens(title)
    if len(tokens) >= 2:
        top3 = tokens[:3]
        for section in sections:
            if all(t in section for t in top3):
                return True
        # Relax to 2 tokens
        top2 = tokens[:2]
        for section in sections:
            if all(t in section for t in top2):
                return True

    # Strategy 3: single highly distinctive keyword (≥ 6 chars)
    for token in tokens:
        if len(token) >= 6:
            for section in sections:
                if token in section:
                    return True

    # Strategy 4: section heading (after prefix like "p2 — ") is a substring of item title
    item_lower = title.lower()
    for section in sections:
        # Strip common prefixes like "p1 — ", "p2 — ", "phases a–h — "
        core = re.sub(r"^p\d+\s*[—\-]+\s*", "", section).strip()
        core = re.sub(r"^phases?\s+[\w–-]+\s*[—\-]+\s*", "", core).strip()
        if len(core) >= 4 and core in item_lower:
            return True

    return False


def main() -> int:
    if not BACKLOG.exists():
        print(f"ERROR: {BACKLOG} not found", file=sys.stderr)
        return 1

    if not EVIDENCE.exists():
        print(f"ERROR: {EVIDENCE} not found", file=sys.stderr)
        return 1

    backlog_text = BACKLOG.read_text(encoding="utf-8")
    evidence_text = EVIDENCE.read_text(encoding="utf-8")

    done_items = extract_done_items(backlog_text)
    evidence_sections = extract_evidence_sections(evidence_text)

    if not done_items:
        print("WARNING: no done items found in BACKLOG.txt — check format", file=sys.stderr)
        return 0

    print(
        f"Checking {len(done_items)} done backlog item(s) against "
        f"{len(evidence_sections)} evidence-of-done section(s)..."
    )

    gaps: list[dict] = []
    for item in done_items:
        if not item_matches_any_section(item["title"], evidence_sections):
            gaps.append(item)

    if gaps:
        print(f"\nFAIL: {len(gaps)} done item(s) lack evidence-of-done entries:\n")
        for g in gaps:
            print(f"  Line {g['line']:4d}: {g['title']}")
        print(
            "\nAdd a matching '## <title>' section to docs/evidence-of-done.md for each gap."
        )
        return 1

    print(f"OK: all {len(done_items)} done items have evidence-of-done entries.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
