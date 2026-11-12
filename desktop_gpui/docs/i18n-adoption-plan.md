# Alera GPUI i18n — Settings-first implementation and migration

## Decision

Use rust-i18n 4.2.4 (already in GPUI Kit 0.9.0's dependency graph).
Use Classic source English keys and Classic zh-TW / zh-CN / ja dictionaries as the translation source.
Do not introduce another persistent settings store or wire inert preview rows to backend calls.

## Completed on GPUI Kit integration branch

- Selectively ported the UI-only settings catalog, locale importer, overrides, and shared native_i18n facade from the older GPUI 0.2.2 worktree.
- Kept the current GPUI Kit 0.9.0, editor, dual-pane Terminal, Runtime Host and other integration implementation untouched.
- Native Settings uses all 15 Classic category names with Preferences/Resources grouping, section header, descriptions and group chips.
- Search matches translated text and original English labels.
- Settings-only in-memory locale picker supports English, Traditional Chinese, Simplified Chinese and Japanese; no preference persistence.
- All added preview rows have **no write callbacks**. Pre-existing functional Native controls in Application, Editor, Terminal, Agents and Agent Profiles remain available in their own clearly labeled section.
- The app locale is view-local, deliberately not a process-global mutable setting; existing Kit components are not globally translated.
- Locale generation: python tool/generate_settings_locales.py
- Drift check: python tool/generate_settings_locales.py --check
- Locale coverage: 211 entries for English, zh-TW, zh-CN and ja.

## Next phase — shared app locale and GPUI Kit widgets

1. Resolve system locale from native OS APIs including Windows (do not rely solely on LANG).
2. Create one observable Native app locale owner; read the existing Classic language preference only via approved settings access.
3. Register GPUI Kit component translation overrides once at startup, before Kit initialization, where the component namespace is actually needed.
4. Propagate locale change notifications to multiple native windows and popups; never reset PTYs, editor state, or Host sessions.
5. Translate existing Native-only connected settings explanations, statuses, dynamic user messages and buttons using stable semantic keys and interpolation.
6. Verify Windows/macOS UI screen widths, IME focus, four-language search, all 15 tabs, fallback behavior, and accidental preference mutations.
7. Expand to project/worktree management, Explorer, Git Graph, Editor, Terminal chrome, MCP surfaces, context menus and errors.
8. Leave branch names, filesystem paths, raw Terminal contents, shell commands and API payloads untranslated.

## Acceptance gates

- Native GPUI Kit cargo check succeeds; Rust tests for locale mapping, source-string fallback, matching search and exact five connected sections pass.
- Generated YAML is up to date; all Settings catalog strings have en, zh-TW, zh-CN and ja values.
- Existing Native Settings controls retain their original handlers, identity checks and durable compare-and-set behavior.
- Newly added preview rows are visual-only; no new Native settings mutation callbacks.
- No production merge, push or release is implied by integrating this feature into the GPUI development branch.
