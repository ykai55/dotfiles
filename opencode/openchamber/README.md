# OpenChamber preferences

`preferences.json` is a complete snapshot of the local OpenChamber profile,
including field timestamps and recent selections. Review exported changes before
committing: custom instructions and draft starters can contain private content.

Run from the repository root after changing preferences in OpenChamber:

```bash
opencode/openchamber/sync.sh export
git diff -- opencode/openchamber/preferences.json
```

To restore the snapshot, close OpenChamber (including any background server), run:

```bash
bin/dotfiles-apply --apply
```

Then reopen OpenChamber. The `openchamber-preferences` mapping uses `copy` mode.
Identical files are left alone; existing, different files are skipped. To restore
over local changes, use `bin/dotfiles-apply --apply --force`, which backs up
conflicting paths as `.bak`, `.bak.1`, etc. This flag affects all mappings;
preview with `bin/dotfiles-apply --force` first. Other OpenChamber configuration
and data are preserved.

The mapping restores to `~/.config/openchamber/preferences.json`. For a custom
instance, change its target in `dotfiles-map.json` and set `OPENCHAMBER_DATA_DIR`
when exporting. The export script requires `jq` and `realpath`; its `export`
argument is optional. Files are copied because OpenChamber atomically replaces
its preferences file when saving. This directory is excluded from the OpenCode
configuration links.
