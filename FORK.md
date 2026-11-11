# This fork

The shop's fork of `longbridge/gpui-fast`. Each `shop/vX.Y.Z` branch is one upstream release tag plus a short series of patches that have not landed upstream. Each patch is one commit; comments it adds start with "Fork patch:". The shop's UI kit pins one commit on the newest shop branch (a full sha, never a branch name), `gpui` and `gpui_platform` at the same rev, and takes its wgpu line from this repo's Cargo.toml in the same change.

Remotes: `origin` is github.com/bootlegcheeseburger/gpui-fast; `upstream` is github.com/longbridge/gpui-fast, read only. longbridge imports its crates from zed-industries/zed itself, so the shop never imports from zed directly.

## Patches

| Patch | Subject | Upstreamable |
|---|---|---|
| e11c1cc | App menu item state: `App::on_validate_menu_item` returns a `MenuItemState` (enabled, checked, title) each time a menu opens; macOS applies it. | yes, generic and tested; touches many upstream files |
| 61b2051 | Presented-frame timing: the display link's output time travels with the frame request, and `Window::take_presented` reports when each frame reached the display. | maybe; a new public API with cross-platform stubs |
| d687771 | `surface()` draws `420f` and `420v` buffers with the buffer's YCbCr matrix, and skips other formats with a one-time log instead of panicking. | yes, bug-fix shaped, Apple crate only |
| 67f66a9 | File drags set the input modality to mouse, so a drop after keyboard input still reaches `on_drop`. | yes, small bug fix |
| receipt-drain | `gpui: Drain presentation receipts into reusable buffers`: `Window::drain_presented` appends receipts while keeping the callback queue and caller buffer allocations. | yes, tested at queue capacity |
| 5b80865 | `gpui: Count layout work per view (test-support)`: dense view slots attribute created, claim-reused, kept, measured, shaped and dirtied work. | maybe, a new test-support API |

The named rows identify commits by subject. The hashes are the series on `shop/v0.1.14`; a rebase gives new hashes, and the table names each patch's hash on the newest branch. Planned: an input dispatch observer for the shop kit's input diagnostics More than about two new patches goes back to the person first (shop ruling, 2026-10-30).

## Upstream-check baseline

The existing shop patches predate the fast-only rule. Replaying them on
`v0.1.14` (`a427a7d9a6072473ad092b1673cd5235f99284cc`) leaves 17 existing
`script/check-upstream` violations. New patches keep their logic in `fast/`
and add no new violations; migrating the legacy patches is separate work.
The replay adapts the receipt callback and surface shader uniforms to objc2 Metal,
uses the Metal display link's presentation target on macOS 14+, and imports the
layout engine from its internal module for the per-view counters.

## Moving to a new upstream base

The series is rebased onto each new upstream release tag on a new branch; an old shop branch is never edited once the next one exists. This replaces the earlier merge-only rule (person, 2026-11-06: a short named patch series rebased per upstream base).

```sh
git fetch upstream --tags
git switch -c shop/vNEW shop/vOLD
git rebase --onto vNEW vOLD
```

`blc.toml` sets the shop's push check to the fork's own checks (the fast tests, `menu_item_state`, `gpui_platform`), not upstream's whole workspace, whose formatting and unrelated test failures are upstream's (person, 2026-11-10).

Use the newest `vX.Y.Z` tag (`git tag --sort=-v:refname -l 'v*' | head -1`). After the rebase: `cargo test -p gpui --test menu_item_state`, `cargo check -p gpui_platform` on macOS, then bump the shop kit's pin. Drop a patch when upstream ships the same fix, and say so in the table.

Pushing branches, opening pull requests upstream and changing the GitHub repo belong to the person.

## Decisions

- 2026-11-10, person: approved per-view work counters as a new fork patch beyond two (shop kit's per-view idle checks).
