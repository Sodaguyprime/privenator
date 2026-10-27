# Design system: Academia / Classical

"A library at night." Mahogany wood, antique parchment, polished brass, crimson leather. Restrained ornament, three serifs, slow and dignified motion. This file is the platform-neutral reference, written so the Android UI can reproduce the look. On desktop, the implementation is `src/styles/theme.css` (tokens) and `src/styles/ornaments.css` (decorations).

## Tokens

### Color

| Token | Hex | Use |
| --- | --- | --- |
| `background` | `#1C1714` | Page (deep mahogany) |
| `background-alt` | `#251E19` | Cards, panels, inputs (aged oak) |
| `foreground` | `#E8DFD4` | Primary text (antique parchment) |
| `muted` | `#3D332B` | Tertiary surfaces, disabled fills |
| `muted-foreground` | `#9C8B7A` | Secondary text, metadata |
| `border` | `#4A3F35` | Dividers, card borders (wood grain) |
| `accent` | `#C9A962` | **Every interactive element**: buttons, links, focus, icons (brass) |
| `accent-bright` / `accent-deep` | `#D4B872` / `#B8953F` | Brass gradient stops, hover |
| `accent-secondary` | `#8B2635` | Crimson: secondary-button hover, the recording seal. Sparingly |
| `accent-foreground` | `#1C1714` | Text on brass |
| `error` | `#D98A95` | Error text ("crimson ink"). Crimson itself is too dark to read as text on mahogany |

Brass gradient: `linear-gradient(180deg, #D4B872 0%, #C9A962 50%, #B8953F 100%)`.
Never use pure black or pure white, and never use cold tones.

### Type

| Role | Family | Size | Notes |
| --- | --- | --- | --- |
| Display / page title | Cormorant Garamond 400 | 48–72 px | line-height 1.1, slight negative tracking |
| Section heading | Cormorant Garamond 400–500 | 24–48 px | |
| Body / journal text | Crimson Pro 400 | 17–18 px | line-height 1.625; emphasis is *italic*, not bold |
| Labels, buttons, numerals | Cinzel 500–600 | 10–12 px | uppercase, tracking 0.15–0.3em |

Fonts are bundled with the app, never downloaded.

### Shape, depth, motion

- Radius: **4 px** everywhere. Full circles for icon medallions and the wax seal. Arch-top (`40% 40% 0 0 / 20% 20% 0 0`) for images, if the app ever shows any.
- Cards have no shadow at rest; on hover they get `0 8px 24px rgb(0 0 0 / .3)` and a brass-tinted border.
- Engraved buttons: `inset 0 1px 0 rgb(255 255 255/.2), inset 0 -1px 0 rgb(0 0 0/.2), 0 2px 8px rgb(0 0 0/.3)`. Hover adds a brass glow `0 4px 12px rgb(201 169 98/.3)`.
- Focus: 2 px brass ring with a 2 px offset. Always visible.
- Motion is ease-out only. Durations: 150 ms (press/focus), 300 ms (hover), 500 ms (lift), 700 ms (reveals). No bounce, no spring, no slide-ins. `prefers-reduced-motion` turns transitions off.

## Ornaments (use with restraint)

| Ornament | Where it appears in this app |
| --- | --- |
| Paper grain (3% noise, overlay) + vignette | Once, fixed over the whole app |
| Corner flourish, 40 px | Major frames only: welcome, lock screen, recovery key sheet. Drops to 24 px on mobile |
| Corner flourish, 24 px | Entry cards, the active entry |
| Ornate divider (✶ ❧ ✤ ❦) | Between days in the sidebar; under page titles |
| Drop cap | First paragraph of an entry in reading view, and the welcome text |
| Roman numerals | Entry numbering within a day (I, II, III, oldest first); "Volume" labels on major screens; recovery-key groups |
| Wax seal (crimson) | Recording only: the record buttons, the pulsing indicator while recording, the big "Begin recording" seal. Never for anything else |
| Engraved text | Buttons and the recording timer |

Adaptations from the source style guide: it targets marketing pages (pricing ledgers, testimonials, sepia hero photos). This app has no photos, so arch-tops and sepia transitions stay unused until there's an image to apply them to.

## Components

**Button.** Cinzel, uppercase, `text-xs`, tracking 0.15em. Sizes: sm 40 px, md 48 px, lg 56 px tall.
- *Primary*: brass gradient, dark text, engraved shadow; hover brightness 110% + glow; active is an inset press.
- *Secondary*: 2 px brass outline, brass text; hover turns crimson (fill + border) with parchment text.
- *Ghost*: brass text; hover brightens, underlines (offset 4 px) and widens the tracking.
- *Disabled*: 50% opacity, no pointer.

**Input.** 48 px tall, `background-alt` fill, 1 px wood-grain border, Crimson Pro, italic faded-ink placeholder. Focus gives a brass border and a soft brass ring. Labels are in Cinzel above the field. Journal-text fields set `spellcheck="false"` (privacy, see SPEC.md).

**Frame.** The centred "certificate" used for every screen outside the journal: 40 px flourishes, `background-alt` at 50%, a medallion icon, a Cinzel overline, a Cormorant title, then a ❧ divider. `narrow` (max-w-xl) for forms, `wide` (max-w-3xl) for the recovery key.

**Password strength meter.** Four 4 px bars under the field. They fill brass when the password is acceptable and crimson when not, with the score label in brass or error ink plus zxcvbn's feedback in muted text.

**Recovery key.** Cinzel, brass, in a `background` panel with a 40% brass border. Seven groups, four per row, each with a small Roman numeral underneath (I–VII), matching the "Group V" labels on the check step.

**Level meter.** 24 thin vertical bars, rising in height, brass when lit and `muted` otherwise; fills with the RMS level (−60…0 dBFS). Used in the microphone test and while recording. `role="meter"`.

**Mood picker.** Five outlined buttons labelled Very low … Very good; the selected one gets a brass border and a 15% brass fill. Choosing it again clears the mood.

**Tag input.** A field-styled box holding brass-outlined chips (× to remove), with an inline input. Enter or comma adds; Backspace on an empty input removes the last chip; suggestions from existing tags.

**Sidebar (journal).** 320 px, `background-alt` at 30%. Header with the "Privenator" inscription, Settings and Lock icon buttons; a full-width crimson wax-seal "Record" button with a feather icon button for typed entries; the search field with a filter toggle (mood, tag, from/to dates); then entries grouped by day: a Cinzel day inscription, ornate dividers cycling ✶ ❧ ✤ ❦ between days, and per entry a brass Roman numeral, the Cormorant title, a muted meta line (time · duration · mood) and, when searching, a two-line snippet with matches as `<mark>` in a 25% brass wash.

**Recording panel.** "Speak freely" with a 112 px wax seal to begin. While recording: the engraved Cinzel timer as the title, a 48 px pulsing seal (no pulse under reduced motion) beside the level meter, Pause/Resume (secondary) and Stop (primary), then the transcript in a panel: finals in parchment, the preview in italic faded ink.

**Save dialog.** "Seal this entry": the transcript (scrolls past 240 px), "Keep the audio" checkbox with an explanation, mood picker, tag input, a large primary "Save entry".

**Entry view.** Capped at 72 ch. A Cinzel date/time/duration inscription, the title as a borderless Cormorant input, a ✶ divider, the audio button (secondary) or player, then the reading view (paragraphs, drop cap on the first) with Edit, "View original"/"View edited" and "Revert to original". Mood and tags below a wood-grain rule; delete at the bottom behind an inline confirmation.

**Notices.** A 40% brass-bordered strip with a 10% brass wash and a "Dismiss" text button: recovered recordings, backup reminders.

**Settings.** Sections separated by wood-grain rules, each headed by a brass Cinzel label: Locking, Recording, Password, Recovery key, Backup, Restore. Restore's warning is in error ink and needs the word "replace" typed.

**Icon medallion.** A Lucide icon at stroke 1.5, in brass, inside a circle with a 30% brass border on `background`.

## Layout

- 8 px spacing grid. Generous whitespace.
- Desktop: sidebar (entries by day) plus a reading/editing column capped at about 72 ch.
- Mobile/Android (< 768 px): single column, touch targets of 48 px or more, large flourishes reduced to 24 px, full-width buttons. The journal shows either the sidebar or the open pane, with an "← Entries" link back.

## App icon

A crimson leather diary with a brass corner, clasp, keyhole and ribbon on a mahogany tile. The source is `src-tauri/icons/source.svg`. Regenerate every size (desktop and Android) with:

```
npx tauri icon src-tauri/icons/source.svg
```
