# Material Icon Theme subset for Alera GPUI

These SVG files are a curated subset of the same **Material Icon Theme** used
by Alera Classic's `vscode_material_icon_theme` Flutter package.

- Upstream: https://github.com/PKief/vscode-material-icon-theme
- Source directory: `icons/`
- Upstream revision observed during import: `3929c5487ca03352f31cfba675b367448449094d`
- License: MIT. See `LICENSE.upstream` in this folder.
- Imported: 2026-11-15

The files are embedded at compile time and never fetched over the network by
the desktop app. Only supported file types are mapped; unknown files use
GPUI's Lucide/Codicon fallback. To update, recheck the upstream license and
run `rustc --edition 2021 --test src/native_file_icons.rs`.
