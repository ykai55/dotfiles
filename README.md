# Dotfiles

## Apply On A New Machine

The repo assumes it is reachable as `~/dotfiles`, and that location is treated
as a trusted path. If you cloned it somewhere else, `bin/dotfiles-apply` will
create a compatibility symlink at `~/dotfiles`.

Preview the changes:

```bash
~/dotfiles/bin/dotfiles-apply
```

Apply the default user-level mappings from `dotfiles-map.json`:

```bash
~/dotfiles/bin/dotfiles-apply --apply
```

Select categories by their mapping names with `--only`; use commas for multiple
categories. `git` selects local Git configuration setup, and `downloads` selects
managed downloads. Omitting `--only` selects everything. Unknown or empty names
are rejected before changes are made. The shared `~/dotfiles` compatibility link
is still prepared for selected categories.

```bash
~/dotfiles/bin/dotfiles-apply --only fish,git,tmux
~/dotfiles/bin/dotfiles-apply --apply --only fish,git,tmux
~/dotfiles/bin/dotfiles-apply --apply --only downloads --downloads always
```

`dotfiles-apply` also clones git downloads from `downloads.json`.
Tide is managed this way and fish will prefer
`~/dotfiles/.managed/tide` when it is present.

Replace conflicting local files with backed-up copies:

```bash
~/dotfiles/bin/dotfiles-apply --apply --force
```

Refresh existing downloads and git repositories:

```bash
~/dotfiles/bin/dotfiles-apply --apply --downloads always
```

The manifest is JSON and declares the source/target mapping for each config.
`link_children` is useful for directories like `fish/`, where tracked config
should be linked in while local machine state such as `fish_variables` stays
local.

`copy` installs an independent file, suitable for applications such as
OpenChamber that replace their settings files when saving. Identical content is
left alone; conflicting destinations require `--force` and are backed up before
replacement. Dry runs preview copies and backups without writing files.

After successful mapping, `dotfiles-apply` runs `scripts/setup-gitconfig`.
It keeps `~/.gitconfig` as a local file and prepends an include of
`~/dotfiles/git/config` when missing. Existing content is preserved, so local
single-value settings after the include override shared defaults. Repeated
runs leave an existing include alone. Before changing an existing file or
converting a symlink, the script saves its contents in a regular `.gitconfig.bak`
file (with a numeric suffix if needed). Symlink conversion preserves all current
settings, including any duplicated shared defaults; those local copies can be
removed manually when no longer needed. Dry runs make no changes.

`dotfiles-map.json` now references `dotfiles-map.schema.json`, so editors that
support JSON Schema can validate the manifest and offer completion.

Supported manifest fields:

- `name`: display name used in output
- `source`: repo-relative source path
- `target`: destination path
- `mode`: `symlink`, `link_children`, or `copy` (single file)
- `exclude`: glob patterns skipped under `link_children`
- `platforms`: only apply on matching platforms such as `linux` or `macos`

Example Linux-only mapping:

```json
{
  "name": "niri",
  "source": "niri/config.kdl",
  "target": "~/.config/niri/config.kdl",
  "mode": "symlink",
  "platforms": ["linux"]
}
```

## TODO

- [ ] use jinja to generate dotfiles for different platforms
- [ ] auto recovery
- [ ] use kitty in linux
- [ ] store some binary's urls for each platform, download theme when initializing
- [ ] set up shell environments with dedicated files, so that thay can be loaded across all shells and platforms
