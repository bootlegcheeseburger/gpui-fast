# This fork

The `patched` branch is the newest `longbridge/gpui-fast` release tag
(currently `v0.1.12`) plus a few patches that have not landed upstream. Each patch is one commit; comments it adds start
with "Fork patch:".

## Patches

- App menu item state: `App::on_validate_menu_item` returns a `MenuItemState`
  (enabled, checked, title) each time a menu opens; macOS applies it.
- Presented-frame timing: the display link's output time travels with the
  frame request, and `Window::take_presented` reports when each frame reached
  the display.
- `surface()` draws `420f` and `420v` buffers with the buffer's YCbCr matrix,
  and skips other formats with a one-time log instead of panicking.
- File drags set the input modality to mouse, so a drop after keyboard input
  still reaches `on_drop`.

## Syncing with upstream

The branch follows upstream's release tags, not `main`, and only moves
forward by merging the newest one. Never rebase it.

```sh
git fetch upstream --tags && git merge vX.Y.Z
```

Use the newest `vX.Y.Z` tag (`git tag --sort=-v:refname -l 'v*' | head -1`).

Drop a patch when upstream ships the same fix.
