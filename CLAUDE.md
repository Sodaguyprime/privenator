# Privenator: private voice journal

Offline, encrypted voice journal. Tauri v2 (Rust) + React/TS/Vite/Tailwind v4. An Android app will be built from this later, so **everything must be documented.**

## Documentation is part of done

A change isn't finished until the docs match it. In the same change:

| If you… | Update |
| --- | --- |
| add or change user-facing behavior | `docs/SPEC.md` |
| change a file format, the DB schema, crypto or key handling | `docs/SPEC.md` (Data formats / Security model) **and** `docs/DECISIONS.md`; formats are cross-platform contracts, so bump the version |
| add, change or remove a Tauri command or event | `docs/ARCHITECTURE.md` → Command API |
| touch structure, dependencies, plugins, CSP or capabilities | `docs/ARCHITECTURE.md` + `docs/DECISIONS.md` |
| change tokens, components or visual rules | `docs/DESIGN.md` (and `src/styles/theme.css`) |
| make a judgment call, or deviate from the spec | `docs/DECISIONS.md` (dated, with why) |
| finish or defer part of a build step | `docs/PROGRESS.md` |
| learn something Android will need | `docs/ARCHITECTURE.md` → Android port |

## Rules

- Sensitive logic goes in `crates/journal-core`, which must not depend on Tauri. `src-tauri` is a thin shell.
- No Tauri plugins and no network access. Adding a plugin needs a DECISIONS.md entry.
- New command checklist: `src-tauri/src/commands/<area>.rs` → `lib.rs` handler list → `build.rs` list → `capabilities/main.json` → `src/lib/ipc.ts` → ARCHITECTURE.md.
- Journal text (transcripts, entries) never goes to the UI while locked, and never into logs or error messages.
- Never log journal text, passwords or keys. Never write plaintext to disk.
- Styling: use tokens from `theme.css` (`bg-background`, `text-accent`, `font-display`…), not raw hex values. Fonts are bundled through `@fontsource`.

## Commands

- `npm run fetch-model`: download the Whisper model (needed by the app and two tests)
- `npm run tauri dev`: run the app
- `npm run tauri build`: release build and installers
- `npm run lint`, `npm run build`: frontend checks
- `cargo test --workspace`, `cargo clippy --workspace`: Rust checks
- `npx tauri icon src-tauri/icons/source.svg`: regenerate icons
