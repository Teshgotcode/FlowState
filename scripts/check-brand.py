#!/usr/bin/env python3
"""Fail if a user-facing string still says Handy.

FlowState is a fork of cjpais/Handy, so "Handy" legitimately appears in
attribution, upstream links, compatibility identifiers (handy_keys, the
handy-recorder crate, HANDY_* env aliases, legacy storage keys) and the
upstream history. What must not happen is a *user* reading "Handy" in the UI
where the product is FlowState.

Scans the i18n locale files (values only) and TSX string literals. The About
panel's attribution block is allowed by key; every other hit is printed with
file and line. Exit 1 when anything is found.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCALES = ROOT / "src" / "i18n" / "locales"
SRC = ROOT / "src"

# The About panel credits the upstream project on purpose; these keys may say Handy.
ALLOWED_KEYS = {
    "settings.about.upstream.title",
    "settings.about.upstream.description",
}

# Identifiers, package names and real upstream URLs are not user-facing copy.
ALLOWED_SUBSTRINGS = (
    "handy.computer",
    "@handy-computer/models",
    "cjpais/handy",
    "handy_keys",
    "handy-keys",
    "basert på handy",
    "built on handy",
)

STRING_LITERAL = re.compile(r"""(["'`])((?:\\.|(?!\1).){0,400}?)\1""")


def key_path(prefix: str, key: str) -> str:
    return f"{prefix}.{key}" if prefix else key


def walk(node, prefix=""):
    if isinstance(node, dict):
        for key, value in node.items():
            yield from walk(value, key_path(prefix, key))
    elif isinstance(node, list):
        for index, value in enumerate(node):
            yield from walk(value, key_path(prefix, str(index)))
    elif isinstance(node, str):
        yield prefix, node


def scan_locales() -> list[str]:
    hits = []
    if not LOCALES.is_dir():
        return hits
    for path in sorted(LOCALES.rglob("*.json")):
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        for key, value in walk(data):
            if "handy" in value.lower() and key not in ALLOWED_KEYS:
                hits.append(f"{path.relative_to(ROOT)}: {key} -> {value.strip()[:120]}")
    return hits


def scan_tsx() -> list[str]:
    hits = []
    for path in sorted(SRC.rglob("*.tsx")):
        if "node_modules" in path.parts:
            continue
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if "handy" not in line.lower():
                continue
            for _, literal in STRING_LITERAL.findall(line):
                lowered = literal.lower()
                attribution = "cjpais" in lowered or "mit" in lowered
                if "handy" in lowered and not attribution and not any(
                    allowed in lowered for allowed in ALLOWED_SUBSTRINGS
                ):
                    hits.append(f"{path.relative_to(ROOT)}:{number} -> {literal.strip()[:120]}")
                    break
    return hits


def main() -> int:
    hits = scan_locales() + scan_tsx()
    if not hits:
        print("no user-facing 'Handy' strings — attribution and identifiers only")
        return 0
    print(f"{len(hits)} user-facing string(s) still say Handy:")
    for hit in hits:
        print(f"  {hit}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
