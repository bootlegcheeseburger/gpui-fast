# This fork

The shop's fork of `longbridge/gpui-fast`. Each `shop/vX.Y.Z` branch is one upstream release tag plus a short series of patches that have not landed upstream. Each patch is one commit; comments it adds start with "Fork patch:". The shop's UI kit pins one commit on the newest shop branch (a full sha, never a branch name), `gpui` and `gpui_platform` at the same rev, and takes its wgpu line from this repo's Cargo.toml in the same change.

Remotes: `origin` is github.com/bootlegcheeseburger/gpui-fast; `upstream` is github.com/longbridge/gpui-fast, read only. longbridge imports its crates from zed-industries/zed itself, so the shop never imports from zed directly.

## Patches

| Patch | Subject | Upstreamable |
|---|---|---|
| a57ee60 | App menu item state: `App::on_validate_menu_item` returns a `MenuItemState` (enabled, checked, title) each time a menu opens; macOS applies it. | yes, generic and tested; touches many upstream files |
| 700dc27 | Presented-frame timing: the display link's output time travels with the frame request, and `Window::take_presented` reports when each frame reached the display. | maybe; a new public API with cross-platform stubs |
| f09f76f | `surface()` draws `420f` and `420v` buffers with the buffer's YCbCr matrix, and skips other formats with a one-time log instead of panicking. | yes, bug-fix shaped, Apple crate only |
| 0dfb972 | File drags set the input modality to mouse, so a drop after keyboard input still reaches `on_drop`. | yes, small bug fix |

The hashes are the series on `shop/v0.1.12`; a rebase gives new hashes, and the table names each patch's hash on the newest branch. Planned: an input dispatch observer for the shop kit's input diagnostics More than about two new patches goes back to the person first (shop ruling, 2026-10-30).

## Moving to a new upstream base

The series is rebased onto each new upstream release tag on a new branch; an old shop branch is never edited once the next one exists. This replaces the earlier merge-only rule (person, 2026-11-06: a short named patch series rebased per upstream base).

```sh
git fetch upstream --tags
git switch -c shop/vNEW shop/vOLD
git rebase --onto vNEW vOLD
```

Use the newest `vX.Y.Z` tag (`git tag --sort=-v:refname -l 'v*' | head -1`). After the rebase: `cargo test -p gpui --test menu_item_state`, `cargo check -p gpui_platform` on macOS, then bump the shop kit's pin. Drop a patch when upstream ships the same fix, and say so in the table.

Pushing branches, opening pull requests upstream and changing the GitHub repo belong to the person.
