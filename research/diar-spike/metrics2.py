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

def evaluate(gt_segs, gt_speakers, pred_segs, label, compute=True):
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
    # speaker-change boundaries: gt transitions where speaker changes
    changes = [(gt_segs[i][1], gt_segs[i][2], gt_segs[i+1][2]) for i in range(len(gt_segs)-1) if gt_segs[i][2] != gt_segs[i+1][2]]
    pb = sorted({round(st, 2) for st, _, _ in pred_segs} | {round(en, 2) for _, en, _ in pred_segs})
    deltas = []
    for t, a, b in changes:
        dmin = min(abs(t - x) for x in pb)
        deltas.append(round(dmin, 2))
    deltas.sort()
    def med(x): return x[len(x)//2] if x else None
    def p90(x): return x[min(int(len(x)*0.9), len(x)-1)] if x else None
    # per-turn speaker attribution: is the mapped pred speaker active for >50% of the turn?
    mapped_lbl = {g: best_map[g] for g in range(len(gt_speakers))}
    ok_turns = 0
    for st, en, s in gt_segs:
        k = int((en - st) / DT)
        hit = 0
        for f in range(int(st / DT), int(en / DT) + 1):
            if f < n and mapped_lbl[gidx[s]] in prv[f]: hit += 1
        if hit >= 0.5 * k: ok_turns += 1
    print(f"--- {label} ---")
    print(f"speakers: {len(plabels)} (gt 3) | pred segs: {len(pred_segs)} | DER {der*100:.1f}%  (FA {fa:.1f}s MD {md:.1f}s SC {sc:.1f}s)")
    print(f"speaker-change boundaries (n={len(changes)}): nearest-pred-boundary median {med(deltas)}s, p90 {p90(deltas)}s, within 0.5s: {sum(1 for d in deltas if d <= 0.5)}/{len(deltas)}")
    print(f"turns attributed to right speaker (>50% dur): {ok_turns}/{len(gt_segs)}")
    print(f"map: " + ", ".join(f"{s}->S{plabels[best_map[gidx[s]]]}" for s in gt_speakers))
    return der

gt_segs, gt_speakers = load_gt()
runs = [("audio/sortformer_segments.txt", "sortformer Q8_0 (default preset)"),
        ("audio/sherpa_segments_oracle.txt", "sherpa oracle num_clusters=3 (4 threads)"),
        ("audio/sherpa_segments_auto.txt", "sherpa auto threshold=0.5")]
for path, label in runs:
    evaluate(gt_segs, gt_speakers, load_pred(path), label)
