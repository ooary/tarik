# E1 Product UI pre-flight

## Design read

Local-first desktop SQL workbench for beginner data engineers, using a calm and precise IDE language with progressive disclosure.

- `DESIGN_VARIANCE: 3` - workspace geometry is stable; asymmetry is functional.
- `MOTION_INTENSITY: 2` - feedback and state changes only; no decorative loops.
- `VISUAL_DENSITY: 7` - data-dense layout with clear hierarchy and readable controls.
- Foundation: custom semantic tokens with Radix accessible primitives where they add behavior.
- Specialized future surfaces: CodeMirror, TanStack Virtual, and XYFlow.

## Adapted anti-slop checklist

This applies relevant `design-taste-frontend` rules to a dense desktop product. Landing-page rules that do not apply to an IDE, data grid, or query editor are intentionally excluded.

- [x] One restrained green accent is used throughout.
- [x] Light and dark colors use semantic tokens and preserve hierarchy.
- [x] One 5px interactive radius system is used.
- [x] No AI-purple gradient, glow, glassmorphism, or decorative mesh exists.
- [x] No generic card dashboard or fake KPI cards exist.
- [x] Structure uses workbench regions, spacing, and quiet dividers.
- [x] Visible copy is functional and contains no startup slogans or filler verbs.
- [x] No decorative status dots exist; the single status mark conveys real engine state.
- [x] Controls have visible keyboard focus through a high-contrast focus ring.
- [x] Button labels remain on one line at desktop sizes.
- [x] Data values use tabular monospace text.
- [x] Result table uses data-grid conventions rather than marketing table styling.
- [x] Loading/fallback runtime state is represented.
- [x] Empty Flow/Profile placeholders explain how to populate the panel.
- [x] Panel collapse controls expose accurate accessible labels.
- [x] Radix Tabs provides keyboard behavior for result surfaces.
- [x] Motion is limited to tactile press feedback and respects reduced motion.
- [x] Narrow-window rules remove secondary metadata before core controls.
- [x] UI uses system/native fonts without external network font loading.
- [x] No hand-drawn SVG paths, emoji controls, fake avatars, or generic personas exist.
- [x] No em dash or en dash is present in visible copy.
- [x] No full SQL result set is implied to be loaded; shown rows are a shell fixture only.

## Manual review matrix

Review at these sizes:

| View       |     Size | Verify                                                                     |
| ---------- | -------: | -------------------------------------------------------------------------- |
| Wide light | 1440x900 | Full explorer, two query tabs, editor, result table, status metadata       |
| Wide dark  | 1440x900 | Contrast and hierarchy parity                                              |
| Narrow     |  760x640 | Secondary labels collapse, core workspace remains usable                   |
| Minimum    |  680x520 | No vertical page scroll, panels remain reachable, table scrolls internally |

Keyboard checks:

1. Tab through the header, source explorer, query tabs, query actions, SQL surface, result tabs, and status controls.
2. Use arrow keys to move among Results, Flow, and Profile tabs.
3. Activate panel collapse controls with Enter and Space.
4. Confirm focus is never communicated only by color.

## Known E1 boundary

The shell displays realistic fixture content to review composition. CodeMirror, virtualized rows, live catalog sources, and the actual query plan are delivered by later EPICs. UI preference persistence is dependency-gated on E2 SQLite repositories and is not simulated with another operational store.
