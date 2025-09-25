# Dotfiles

## Utilities

### Clipboard
Cross-platform clipboard utility supporting macOS, Linux X11, and Linux Wayland.

Usage:
```
# Copy to clipboard
echo "text" | clipboard copy

# Paste from clipboard  
clipboard paste

# Copy is the default action
echo "text" | clipboard
```

Supported platforms:
- macOS: Uses `pbcopy`/`pbpaste`
- Linux X11: Uses `xclip` or `xsel`
- Linux Wayland: Uses `wl-copy`/`wl-paste`

## TODO

- [ ] use jinja to generate dotfiles for different platforms
- [ ] auto recovery
