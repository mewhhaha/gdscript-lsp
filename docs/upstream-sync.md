# Upstream Sync

This project can refresh GDScript metadata snapshots from a local Godot checkout.

## Prerequisites

- A local Godot source tree (for example `/tmp/godot-upstream`).
- `rg` available in PATH.

## Run

```bash
./scripts/sync_godot_metadata.sh /tmp/godot-upstream
```

This updates:

- `data/godot_4_6_warning_codes.txt`
- `data/godot_4_6_utility_functions.txt`

## Notes

- The sync is deterministic (sorted outputs).
- The tool currently snapshots Godot 4.6-oriented metadata paths.
