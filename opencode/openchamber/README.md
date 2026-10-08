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
opencode/openchamber/sync.sh apply
```

Then reopen OpenChamber. Apply replaces the complete preferences file and saves
an existing, different file as `preferences.json.backup.*` beside the local file.
Other local configuration and data are preserved.

The default local directory is `~/.config/openchamber`; set
`OPENCHAMBER_DATA_DIR` to use another instance. The script requires `jq` and
`realpath`. Both commands copy files rather than link them because OpenChamber
atomically replaces its preferences file when saving. `dotfiles-apply` excludes
this directory; run `sync.sh apply` explicitly when restoring a machine.
