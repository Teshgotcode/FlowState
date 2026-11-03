#!/usr/bin/env python3
"""sweep — finn fremmede merkevarer i FlowStates kode og assets.

Del av S-26-gate: ingen referanser til referanseappen (navn, produktnavn,
deres skrifttyper, domener) skal finnes i app-kode eller assets.

Søker etter Wispr-referanser (navn, produktnavn, deres skrifttyper Figtree/
Manrope, lenker) i all kode og assetnavn. FlowState skal bare referere seg
selv: egen logo, egne ikoner (Lucide), egen typografi (Bricolage Grotesque +
Instrument Sans), egen tekst.

Omfang (gate): src/, src-tauri/src/, src-tauri/tauri.conf.json, public/,
index.html. Binære filer sjekkes bare på filnavn. Utelatt: node_modules,
target, dist, .git, upstream-filer (README-upstream.md, sponsor-images) og
replica/ + research/ + docs/ — metodedokumentene omtaler referansen
analystisk, de er ikke app-kode.

Usage: python3 replica-brand/sweep.py
Exit 0 = rent, 1 = funn.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

# Fremmede merkevarer: navn, produktnavn, deres skrifttyper, domener.
PATTERNS = [
    (r"\bwispr\b", "Wispr-navn"),
    (r"wispr\s*flow", "Wispr Flow-produkt"),
    (r"\bfigtree\b", "fremmed skrifttype (Figtree)"),
    (r"\bmanrope\b", "fremmed skrifttype (Manrope)"),
    (r"wispr\.ai|wisprflow\.com", "fremmed domene"),
]
TEXT_SUFFIXES = {
    ".rs", ".ts", ".tsx", ".css", ".js", ".json", ".html", ".md", ".txt",
    ".toml", ".yml", ".yaml", ".svg", ".conf",
}
IGNORE_DIRS = {"node_modules", "target", "dist", ".git", "replica"}
IGNORE_FILES = {"README-upstream.md", "Cargo.lock"}
ROOTS = [
    Path("src"), Path("src-tauri/src"), Path("src-tauri/tauri.conf.json"),
    Path("public"), Path("index.html"),
]


def iter_files(roots):
    for root in roots:
        if root.is_file():
            yield root
            continue
        for path in sorted(root.rglob("*")):
            if not path.is_file():
                continue
            if any(part in IGNORE_DIRS for part in path.parts):
                continue
            yield path


def sweep(roots):
    findings = []
    for path in iter_files(roots):
        name_hit = name = None
        for pattern, label in PATTERNS:
            if re.search(pattern, path.name, re.IGNORECASE):
                findings.append((path, "filnavn", label))
                break
        if path.suffix.lower() in TEXT_SUFFIXES:
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            for pattern, label in PATTERNS:
                for m in re.finditer(pattern, text, re.IGNORECASE):
                    line = text.count("\n", 0, m.start()) + 1
                    findings.append((path, f"linje {line}", label))
    return findings


def main() -> int:
    findings = sweep(ROOTS)
    if findings:
        for path, where, label in findings:
            print(f"FUNN: {path} ({where}): {label}")
        print(f"{len(findings)} funn — FlowState skal ikke referere Wispr.")
        return 1
    print("rent: ingen Wispr-referanser i kode eller assets")
    return 0


if __name__ == "__main__":
    sys.exit(main())
