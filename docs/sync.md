# Sync between your own Macs — without a cloud

Wispr syncs through its cloud. FlowState does the same job locally: the same
dictionary, snippets and (optionally) history on two Macs, with no account and
no server. All traffic is file copy.

## Why a shared folder is the main path

NTNU-eduroam (and eduroam generally) runs client isolation and per-user
subnets: two devices on the school network cannot find or contact each other.
LAN discovery can therefore never be the primary route. A folder both
machines can see — iCloud Drive, Dropbox, OneDrive or a USB stick — works
everywhere, including in airplane mode, because it is just files.

## How it works

- Each machine picks the same folder. FlowState writes a `FlowState Sync`
  subfolder into it with one JSON document per device
  (`devices/<device_id>.json`) plus a shared `conflicts.jsonl`.
- Every dictionary word and snippet carries an `updated_at` timestamp
  (journalled in the app data dir at edit time). Deletions are tombstones.
- Merge is **last-writer-wins per entry**, three-way against a per-peer
  snapshot of what we last saw from that machine. Unchanged entries never
  travel.
- **Conflicts never lose data.** When both machines changed the same entry
  since the last sync, the newer version stays live and the other survives:
  snippets get a second live snippet with a `(conflict)` trigger, words are
  preserved verbatim in the conflict log. Every collision is appended to
  `conflicts.jsonl` in the shared folder and to the machine's own log, both
  shown in Settings → Sync.
- **Edits beat deletions.** A deletion only wins when the other side hasn't
  touched the entry since the last sync. History (and its audio) is never
  deleted by sync at all — it merges as a union by content key.
- **Off stops everything.** The master switch stops publishing, merging and
  writing. A refused sync (off, no folder, unreadable folder) changes
  nothing.

## The FlowState bundle

For a machine with no shared folder yet, Settings → Sync exports a sealed
zip: settings excerpt, dictionary, snippets, optional history with audio.
`manifest.sig` seals the manifest and every member is checksummed; import
verifies all of it first and refuses a damaged or tampered bundle without
touching local data. Merge semantics are the same as the folder sync —
union, conflicts logged, nothing silently dropped.

## Not in scope

- No server, no account, no telemetry (see the local-first rules).
- Direct two-machine transfer (manual IP + one-time code) is deliberately
  not the discovery path: broadcast/mDNS is blocked on eduroam, so a folder
  remains the only route that works everywhere.
