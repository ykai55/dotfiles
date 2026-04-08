# Dotfiles

## Apply On A New Machine

The repo assumes it is reachable as `~/dotfiles`. If you cloned it somewhere else,
`bin/dotfiles-apply` will create a compatibility symlink at `~/dotfiles`.

Preview the changes:

```bash
~/dotfiles/bin/dotfiles-apply --dry-run
```

Apply the default user-level mappings from `dotfiles-map.json`:

```bash
~/dotfiles/bin/dotfiles-apply
```

Replace conflicting local files with backed-up copies:

```bash
~/dotfiles/bin/dotfiles-apply --force
```

Include optional platform-specific mappings such as `niri` or `keyd`:

```bash
~/dotfiles/bin/dotfiles-apply --include-optional
```

The manifest is JSON and declares the source/target mapping for each config.
`link_children` is useful for directories like `fish/`, where tracked config
should be linked in while local machine state such as `fish_variables` stays
local.

`dotfiles-map.json` now references `dotfiles-map.schema.json`, so editors that
support JSON Schema can validate the manifest and offer completion.

Supported manifest fields:

- `name`: display name used in output
- `source`: repo-relative source path
- `target`: destination path
- `mode`: `symlink` or `link_children`
- `exclude`: glob patterns skipped under `link_children`
- `optional`: only applied with `--include-optional`
- `platforms`: only apply on matching platforms such as `linux` or `macos`

Example Linux-only mapping:

```json
{
  "name": "niri",
  "source": "niri/config.kdl",
  "target": "~/.config/niri/config.kdl",
  "mode": "symlink",
  "platforms": ["linux"],
  "optional": true
}
```

## TODO

- [ ] use jinja to generate dotfiles for different platforms
- [ ] auto recovery
- [ ] use kitty in linux
- [ ] store some binary's urls for each platform, download theme when initializing
- [ ] set up shell environments with dedicated files, so that thay can be loaded across all shells and platforms
