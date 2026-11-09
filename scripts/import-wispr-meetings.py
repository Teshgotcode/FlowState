#!/usr/bin/env python3
"""Import meeting history from Wispr Flow into FlowState's database.

Wispr keeps one folder per meeting under
    ~/Library/Application Support/Wispr Flow/meetings/<uuid>/
with `refined.ndjson` (the cleaned transcript, one JSON object per line, each
carrying the text, the speaker number and a start timestamp), `live.ndjson`
(the same meeting as it was recognised live, with epoch milliseconds) and
`upload.ogg` (the audio).

This copies the text and the speaker numbers into `transcription_history` as a
meeting entry, so the recordings show up in Notetaker with their speakers,
timestamps and language. The audio stays where it is: the entries carry no
`file_name` pointing at a recording, so the player has nothing to open, and the
import never doubles 378 MB of ogg.

Run it with the app quit. Running it twice is safe: an entry whose file_name
starts with `wispr-<uuid>` is skipped.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import sqlite3
import sys
from pathlib import Path

DEFAULT_SOURCE = Path.home() / "Library/Application Support/Wispr Flow/meetings"
DEFAULT_DB = (
    Path.home()
    / "Library/Application Support/com.teshgotcode.flowstate/history.db"
)


def parse_clock(value: str) -> int:
    """`00:07` or `1:02:03` as milliseconds."""
    parts = [int(part) for part in str(value).split(":") if part != ""]
    seconds = 0
    for part in parts:
        seconds = seconds * 60 + part
    return seconds * 1000


def read_lines(path: Path) -> list[dict]:
    if not path.exists():
        return []
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return [row for row in rows if str(row.get("text", "")).strip()]


def meeting_language(folder: Path) -> str | None:
    """The language the mic heard most, from Wispr's own evidence file."""
    evidence = folder / "native-language-evidence.json"
    if not evidence.exists():
        return None
    try:
        data = json.loads(evidence.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None
    counts = (data.get("mic") or {}).get("counts") or []
    if not counts:
        return None
    best = max(counts, key=lambda item: item.get("count", 0))
    return best.get("language")


def build_entry(folder: Path) -> dict | None:
    """One imported meeting: the row's columns and its metadata."""
    refined = read_lines(folder / "refined.ndjson")
    if not refined:
        return None

    starts = [parse_clock(row.get("timestamp", "00:00")) for row in refined]
    segments = []
    for index, row in enumerate(refined):
        start = starts[index]
        end = starts[index + 1] if index + 1 < len(starts) else start + 4000
        segments.append(
            {
                "t0_ms": start,
                "t1_ms": max(end, start + 500),
                "text": str(row["text"]).strip(),
                "speaker_id": (row.get("speaker") or {}).get("id"),
            }
        )

    live = read_lines(folder / "live.ndjson")
    epoch_ms = next(
        (row["startEpochMs"] for row in live if row.get("startEpochMs")), None
    )
    if epoch_ms:
        timestamp = int(epoch_ms // 1000)
    else:
        timestamp = int(folder.stat().st_mtime)

    ends = [row["endRecordingMs"] for row in live if row.get("endRecordingMs")]
    duration = int(max(ends) // 1000) if ends else int(segments[-1]["t1_ms"] // 1000)

    text = " ".join(segment["text"] for segment in segments).strip()
    return {
        "file_name": f"wispr-{folder.name}.ogg",
        "timestamp": timestamp,
        "title": text[:80],
        "text": text,
        "language": meeting_language(folder),
        "metadata": {
            "duration_secs": max(duration, 1),
            "segments": segments,
        },
    }


def import_folder(conn: sqlite3.Connection, folder: Path, dry_run: bool) -> bool:
    entry = build_entry(folder)
    if entry is None:
        return False
    exists = conn.execute(
        "SELECT 1 FROM transcription_history WHERE file_name = ?",
        (entry["file_name"],),
    ).fetchone()
    if exists:
        return False
    if dry_run:
        return True
    conn.execute(
        """
        INSERT INTO transcription_history (
            file_name, timestamp, saved, title, transcription_text,
            post_processed_text, post_process_prompt, post_process_requested,
            source_name, language, language_source, source, metadata
        ) VALUES (?, ?, 0, ?, ?, NULL, NULL, 0, NULL, ?, 'detected', 'meeting', ?)
        """,
        (
            entry["file_name"],
            entry["timestamp"],
            entry["title"],
            entry["text"],
            entry["language"],
            json.dumps(entry["metadata"], ensure_ascii=False),
        ),
    )
    return True


def selftest() -> None:
    """Parsing and segment building, on a fixture shaped like the real files."""
    assert parse_clock("00:07") == 7_000
    assert parse_clock("1:02:03") == 3_723_000

    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        folder = Path(tmp) / "abc"
        folder.mkdir()
        (folder / "refined.ndjson").write_text(
            '{"timestamp":"00:02","text":"Hei.","speaker":{"id":1}}\n'
            '{"timestamp":"00:09","text":"Hallo.","speaker":{"id":2}}\n',
            encoding="utf-8",
        )
        entry = build_entry(folder)
        assert entry is not None
        assert entry["text"] == "Hei. Hallo."
        segments = entry["metadata"]["segments"]
        assert [seg["speaker_id"] for seg in segments] == [1, 2]
        assert segments[0]["t1_ms"] == 9_000
        assert segments[1]["t1_ms"] == 13_000
    print("selftest ok")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--db", type=Path, default=DEFAULT_DB)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()

    if args.selftest:
        selftest()
        return 0

    if not args.source.is_dir():
        print(f"No Wispr meetings at {args.source}", file=sys.stderr)
        return 1
    if not args.db.exists():
        print(f"No FlowState database at {args.db}", file=sys.stderr)
        return 1

    folders = sorted(p for p in args.source.iterdir() if p.is_dir())
    conn = sqlite3.connect(args.db)
    imported = skipped = empty = 0
    try:
        for folder in folders:
            try:
                if import_folder(conn, folder, args.dry_run):
                    imported += 1
                else:
                    skipped += 1
            except (OSError, sqlite3.Error, ValueError) as error:
                print(f"{folder.name}: {error}", file=sys.stderr)
                skipped += 1
        if not args.dry_run:
            conn.commit()
    finally:
        conn.close()

    verb = "would import" if args.dry_run else "imported"
    total = imported + skipped
    print(f"{verb} {imported} of {total} meetings ({skipped} already there or empty)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
