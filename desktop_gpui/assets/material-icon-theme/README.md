# Material Icon Theme subset for Alera GPUI

These SVG files are a curated subset of the same **Material Icon Theme** used
by Alera Classic's `vscode_material_icon_theme` Flutter package.

- Upstream: https://github.com/PKief/vscode-material-icon-theme
- Source directory: `icons/`
- Upstream revision observed during import: `3929c5487ca03352f31cfba675b367448449094d`
- License: MIT. See `LICENSE.upstream` in this folder.
- Imported: 2026-11-15; extended folder coverage: 2026-11-17

## Expanded-folder SVGs

The closed SVGs are from the pinned Material Icon Theme Git revision above.
The `*-open.svg` variants preserve every original fill and `#motive` path
and replace **only** the `d` attribute of `<path id="folder">` with the
upstream `folderIconOpen` path in
`src/core/generator/folderGenerator.ts` (at the same revision).
This is the expansion geometry used by Classic's Material Icon Theme,
not an independently designed substitute.

The additional pinned source icons cover `app`, `components`, `config`,
`dist`, `docs`, `github`, `images`, `node`, `scripts`, `test`, and `vscode`.
The existing `src`, `theme`, and generic `base` folder icons now have
matching opened counterparts. Their directory aliases follow the
upstream folder icon associations; unknown directories retain the
generic Material folder icon.

The assets are embedded at compile time and never fetched over the network
by the desktop app. Unknown file types retain the Lucide/Codicon fallback.
To update, verify the upstream revision and MIT license, then rerun the
Native icon tests (`cargo test --bin alera-gpui-desktop`).
