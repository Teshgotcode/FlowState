#!/usr/bin/env python3
"""imgdiff — structural layout comparator (colour-blind by design).

Compares two wireframe *block maps* (JSON) and returns a layout score in
[0, 100]. It deliberately ignores colour, type, icons and copy: only each
region's role and normalised rectangle enter the metric. That is the "same
information architecture, own visual profile" check from
replica/design-parity.md.

Wireframe format (coordinates are normalised to a 1000x1000 canvas):
  {"screen": "S05", "blocks": [
     {"role": "sidebar", "x": 0, "y": 0, "w": 190, "h": 1000, "kind": "nav"},
     ...]}

Metric: for every block in the REFERENCE map, find the candidate block with
the same role and take the intersection-over-union of the two rectangles.
The per-role IoUs are then weighted by the reference block's area and
normalised, so a misplaced sidebar costs far more than a nudged chip:

    score = 100 * sum(IoU_r * area_r) / sum(area_r)

A reference role with no candidate counterpart scores 0 (the region is
gone); candidate-only roles ("foreign" chrome that is ours, not theirs) do
not subtract, but they can only dilute IoU where they overlap a reference
region — so adding our own elements never inflates the score.

Usage:
  python3 imgdiff.py --reference ref/S05.json --candidate flow/S05.json
  python3 imgdiff.py --batch replica/wireforms   # score every ref/flow pair
Exit code 1 if any score is outside the 75-90 target band (batch mode).
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

MIN_SCORE, MAX_SCORE = 75.0, 90.0


def _load(path: Path) -> dict:
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def _rect(block: dict) -> tuple[float, float, float, float]:
    return float(block["x"]), float(block["y"]), float(block["w"]), float(block["h"])


def _iou(a: tuple, b: tuple) -> float:
    ax, ay, aw, ah = a
    bx, by, bw, bh = b
    ix = max(0.0, min(ax + aw, bx + bw) - max(ax, bx))
    iy = max(0.0, min(ay + ah, by + bh) - max(ay, by))
    inter = ix * iy
    union = aw * ah + bw * bh - inter
    return inter / union if union > 0 else 0.0


def layout_score(reference: dict, candidate: dict) -> tuple[float, dict]:
    """Area-weighted mean IoU of reference roles. Returns (score, detail)."""
    cand_by_role: dict[str, list] = {}
    for block in candidate.get("blocks", []):
        cand_by_role.setdefault(block["role"], []).append(_rect(block))

    total_weight = 0.0
    matched = 0.0
    detail = {}
    for block in reference.get("blocks", []):
        rect = _rect(block)
        area = rect[2] * rect[3]
        best = max((_iou(rect, c) for c in cand_by_role.get(block["role"], [])),
                   default=0.0)
        total_weight += area
        matched += best * area
        detail[block["role"]] = round(best, 3)
    score = 100.0 * matched / total_weight if total_weight else 0.0
    foreign = sorted(set(cand_by_role) - {b["role"] for b in reference.get("blocks", [])})
    return score, {"per_role": detail, "foreign_roles": foreign}


def main() -> int:
    ap = argparse.ArgumentParser(description="structural layout comparator")
    ap.add_argument("--reference", type=Path)
    ap.add_argument("--candidate", type=Path)
    ap.add_argument("--batch", type=Path, help="wireframes dir with ref/ and flow/")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    args = ap.parse_args()

    if args.batch:
        ref_dir, flow_dir = args.batch / "ref", args.batch / "flow"
        rows, ok = [], True
        for ref_path in sorted(ref_dir.glob("*.json")):
            cand_path = flow_dir / ref_path.name
            if not cand_path.exists():
                print(f"missing candidate for {ref_path.name}", file=sys.stderr)
                ok = False
                continue
            score, detail = layout_score(_load(ref_path), _load(cand_path))
            in_band = MIN_SCORE <= score <= MAX_SCORE
            ok = ok and in_band
            rows.append((ref_path.stem, score, detail, in_band))
            if args.json:
                print(json.dumps({"screen": ref_path.stem, "score": round(score, 1),
                                  "in_band": in_band, "detail": detail}))
            else:
                print(f"{ref_path.stem}: {score:5.1f}  "
                      f"{'OK' if in_band else 'OUT OF BAND'}")
        if rows and not args.json:
            avg = sum(r[1] for r in rows) / len(rows)
            print(f"mean: {avg:.1f}  (target {MIN_SCORE:.0f}-{MAX_SCORE:.0f})")
        return 0 if ok else 1

    if not (args.reference and args.candidate):
        ap.error("need --reference/--candidate or --batch")
    score, detail = layout_score(_load(args.reference), _load(args.candidate))
    print(json.dumps({"score": round(score, 1), "detail": detail})
          if args.json else f"layout score: {score:.1f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
