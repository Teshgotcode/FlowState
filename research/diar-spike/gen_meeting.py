import csv, json, random, subprocess, os
random.seed(42)
VOICES = {"Henrik": "Henrik (Enhanced)", "Nora": "Nora (Enhanced)", "Alva": "Alva"}
turns = []
with open("meeting.tsv") as f:
    for row in csv.reader(f, delimiter="\t"):
        spk, text = row[0], row[1]
        aiff, wav = f"turn_{len(turns):02d}.aiff", f"turn_{len(turns):02d}.wav"
        subprocess.run(["say", "-v", VOICES[spk], "-o", aiff, text], check=True)
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", aiff, "-ar", "16000", "-ac", "1", wav], check=True)
        dur = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration",
                                   "-of", "csv=p=0", wav], capture_output=True, text=True).stdout.strip())
        turns.append({"speaker": spk, "wav": wav, "dur": dur})
# build concat list with pauses; ground truth in seconds
open("concat.txt", "w").close()
concat = open("concat.txt", "w")
gt, t = [], 0.0
for i, tn in enumerate(turns):
    start = t
    concat.write(f"file '{tn['wav']}'\n")
    t += tn["dur"]
    gt.append({"speaker": tn["speaker"], "start": round(start, 3), "end": round(t, 3)})
    if i < len(turns) - 1:
        pause = round(random.uniform(0.35, 0.85), 2)
        swav = f"sil_{pause:.2f}.wav"
        if not os.path.exists(swav):
            subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-f", "lavfi",
                           "-i", f"anullsrc=r=16000:cl=mono", "-t", str(pause), swav], check=True)
        concat.write(f"file '{swav}'\n")
        t += pause
concat.close()
subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0",
                "-i", "concat.txt", "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", "meeting.wav"], check=True)
with open("ground_truth.json", "w") as f:
    json.dump({"speakers": ["Henrik", "Nora", "Alva"], "segments": gt}, f, indent=1)
with open("ground_truth.rttm", "w") as f:
    for g in gt:
        f.write(f"SPEAKER meeting 1 {g['start']:.3f} {g['end']-g['start']:.3f} <NA> <NA> {g['speaker']} <NA> <NA>\n")
tot = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration",
                            "-of", "csv=p=0", "meeting.wav"], capture_output=True, text=True).stdout.strip())
print(f"turns={len(turns)} total={tot:.1f}s speech={sum(t['dur'] for t in turns):.1f}s")
