exec(open("metrics2.py").read().split("gt_segs, gt_speakers = load_gt()")[0])
gt_segs, gt_speakers = load_gt()
runs = [
    ("audio/sortformer_segments.txt", "A: sortformer Q8_0 GGUF, transcribe.cpp CLI (Metal, default preset)"),
    ("audio/sherpa_segments_oracle.txt", "B: sherpa-onnx 2.0.1, pyannote+eres2net, oracle k=3, 4 threads"),
    ("audio/sherpa_segments_auto.txt", "B: sherpa-onnx 2.0.1, auto threshold=0.50 (default)"),
    ("audio/sherpa_segments_auto_t065.txt", "B: sherpa-onnx 2.0.1, auto threshold=0.65"),
    ("audio/sherpa_segments_auto_t075.txt", "B: sherpa-onnx 2.0.1, auto threshold=0.75"),
    ("audio/sherpa_segments_auto_t085.txt", "B: sherpa-onnx 2.0.1, auto threshold=0.85"),
    ("audio/sherpa_segments_oracle_thr2.txt", "B: sherpa-onnx 2.0.1, oracle k=3, 2 threads"),
    ("audio/rsl_segments_oracle.txt", "B via sherpa-rs 0.6.8 (sherpa-onnx 1.12.9), oracle k=3"),
    ("audio/rsl_segments_t085rs.txt", "B via sherpa-rs 0.6.8, auto threshold=0.85"),
]
for path, label in runs:
    evaluate(gt_segs, gt_speakers, load_pred(path), label)
