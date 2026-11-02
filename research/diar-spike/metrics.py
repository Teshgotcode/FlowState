import json, re, sys
from itertools import permutations

DT = 0.01

def load_gt(path="audio/ground_truth.json"):
    d = json.load(open(path))
    return [(g["start"], g["end"], g["speaker"]) for g in d["segments"]], d["speakers"]

def load_pred(path):
    segs = []
    for line in open(path):
        m = re.match(r"\[\s*([\d.]+)\s*->\s*([\d.]+)\s*\]\s*S(\d+)", line.strip())
        if m:
            segs.append((float(m.group(1)), float(m.group(2)), int(m.group(3))))
    return segs

def evaluate(gt_segs, gt_speakers, pred_segs, label):
    dur = max(max(e for _, e, _ in gt_segs), max(e for _, e, _ in pred_segs))
    n = int(dur / DT) + 2
    gidx = {s: i for i, s in enumerate(gt_speakers)}
    plabels = sorted(set(s for _, _, s in pred_segs))
    pidx = {s: i for i, s in enumerate(plabels)}
    gtv = [set() for _ in range(n)]
    prv = [set() for _ in range(n)]
    for st, en, s in gt_segs:
        for f in range(int(st / DT), int(en / DT) + 1):
            if f < n: gtv[f].add(gidx[s])
    for st, en, s in pred_segs:
        for f in range(int(st / DT), int(en / DT) + 1):
            if f < n: prv[f].add(pidx[s])
    # co-occurrence for optimal mapping
    co = [[0] * len(plabels) for _ in gt_speakers]
    for f in range(n):
        for g in gtv[f]:
            for p in prv[f]:
                co[g][p] += DT
    best, best_map = -1, None
    for perm in permutations(range(len(plabels)), len(gt_speakers)):
        tot = sum(co[g][perm[g]] for g in range(len(gt_speakers)))
        if tot > best:
            best, best_map = tot, perm
    fa = md = sc = 0.0
    for f in range(n):
        ng, npd = len(gtv[f]), len(prv[f])
        if ng == 0 and npd > 0: fa += DT
        elif ng > 0 and npd == 0: md += DT
        elif ng > 0 and npd > 0:
            mapped = {best_map[g] for g in gtv[f]}
            if not mapped & prv[f]: sc += DT
    total_speech = sum(en - st for st, en, _ in gt_segs)
    der = (fa + md + sc) / total_speech
    # boundary offsets: match each gt seg to best-overlap pred seg of the mapped speaker
    offs = []
    mapped_lbl = {g: best_map[g] for g in range(len(gt_speakers))}
    for st, en, s in gt_segs:
        cands = [(a, b) for a, b, p in pred_segs if p == mapped_lbl[gidx[s]] and b > st and a < en]
        if not cands:
            offs.append(None); continue
        a, b = max(cands, key=lambda ab: min(ab[1], en) - max(ab[0], st))
        offs.append((round(a - st, 2), round(b - en, 2)))
    so = sorted(abs(o[0]) for o in offs if o)
    eo = sorted(abs(o[1]) for o in offs if o)
    def med(x): return x[len(x)//2] if x else None
    def p90(x): return x[int(len(x)*0.9)] if x else None
    print(f"--- {label} ---")
    print(f"pred speakers: {len(plabels)} (gt: {len(gt_speakers)})")
    print(f"pred segments: {len(pred_segs)} | DER: {der*100:.1f}% (FA {fa:.1f}s, MD {md:.1f}s, SC {sc:.1f}s)")
    print(f"start-boundary offset  median {med(so)}s  p90 {p90(so)}s  matched {len(so)}/{len(offs)}")
    print(f"end-boundary offset    median {med(eo)}s  p90 {p90(eo)}s")
    print(f"label map: " + ", ".join(f"{s}->S{plabels[best_map[gidx[s]]]}" for s in gt_speakers))
    return der

gt_segs, gt_speakers = load_gt()
for path, label in [("audio/sortformer_segments.txt", "sortformer Q8_0 (default preset)"),
                    ("audio/sherpa_segments_oracle.txt", "sherpa-onnx (oracle num_clusters=3)"),
                    ("audio/sherpa_segments_auto.txt", "sherpa-onnx (auto, threshold=0.5)")]:
    evaluate(gt_segs, gt_speakers, load_pred(path), label)
