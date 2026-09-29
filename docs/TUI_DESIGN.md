# TUI Design System

## Product intent

The Sweep TUI is an evidence explorer for developer storage. It is not a decorative wrapper around `scan`, and it is not a destructive file browser.

The interactive surface must preserve the product boundary:

```text
live filesystem
    |
    v
discovery
    |
    v
evidence + classification
    |
    v
canonical Report
    |
    v
TUI
```

The TUI consumes descriptive report data. It does not gain mutation authority from selection, animation, filtering, snapshots, or imported state.

## Technology

The TUI stack is intentionally narrow:

- Ratatui 0.30.x for layout, stateful tables, terminal lifecycle, and rendering;
- Crossterm through Ratatui's backend integration for keyboard and resize events;
- TachyonFX 0.25.x for cell-level transitions and composable animation effects;
- standard-library threads and channels for scan work.

There is no async runtime. Scanning is moved off the render thread with a bounded application-level message channel.

Ratatui's stateful-widget model is used for durable selection and scroll state. Terminal setup uses Ratatui's checked initialization/restoration path so raw mode and the alternate screen are restored on normal errors.

TachyonFX effects run after widgets render and are treated as presentation only. Animation never changes candidate state, evidence, decisions, selection semantics, or command behavior.

## Visual language

Sweep should look like a serious developer instrument rather than a generic cleaner.

The default palette is low-luminance graphite with restrained semantic color. It is designed to sit naturally inside dark Ghostty-style terminal themes without depending on terminal palette indexes:

```text
background      #101214
surface         #151719
selection       #2B2D30
foreground      #D7DBE0
bright text     #F0F2F4
muted           #6F767E
accent          #8EA4C7
safe            #83A98C
review          #D5B26E
protected       #D47777
```

The terminal owns font family, font shaping, opacity, blur, cursor rendering, and window chrome. Sweep only owns cell foreground/background/style. A JetBrains Mono Nerd Font setup therefore works well, but Sweep does not require or attempt to configure it.

Semantic color is redundant with text labels. `NO_COLOR=1` disables the palette. `SWEEP_ASCII=1` replaces Unicode navigation and status marks with ASCII-compatible alternatives.

Rounded borders are used as grouping, not decoration. Dense data remains tabular.

## Information architecture

Wide layout:

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ SWEEP  developer storage graph             READY / SCANNING / PARTIAL      │
│ scope ~/Developer   filter all   sort size                                  │
├──────────────────┬──────────────────┬──────────────────┬─────────────────────┤
│ OBSERVED         │ SAFE             │ REVIEW           │ PROTECTED           │
│ 12.0 GB          │ 10.2 GB          │ 4                │ 1                   │
├───────────────────────────────────────────────────┬──────────────────────────┤
│ ALLOCATED  DECISION  TYPE  RECOVERY  PATH         │ evidence                 │
│ ...                                               │ selected candidate       │
│ ...                                               │ recovery + proof chain   │
├───────────────────────────────────────────────────┴──────────────────────────┤
│ q quit  j/k navigate  / search  f filter  S sort  e inspect  R rescan ? help│
└──────────────────────────────────────────────────────────────────────────────┘
```

Below the wide breakpoint, the evidence inspector is removed from the persistent layout and remains available through the inspect overlay.

The interface has four stable zones:

1. identity and live scan status;
2. storage summary;
3. candidate graph;
4. command bar.

This prevents view movement as scan state changes.

## Motion system

Terminal animation has a stricter budget than GUI animation.

Sweep uses motion for state transition, not constant decoration:

- initial surface reveal: directional sweep, approximately 400 ms;
- scan-complete transition: foreground fade, approximately 250 ms;
- active scanning indicator: lightweight frame-based spinner;
- idle UI: no continuous effect loop.

The render loop targets roughly 30 FPS only while scanning or while a TachyonFX effect is active. Idle polling backs off to reduce CPU use.

Animation is applied after semantic widgets render. This guarantees that an interrupted effect can only affect presentation.

## Interaction model

Initial key surface:

```text
j / k / arrows    navigate
g g / G           first / last
enter / e         inspect
/                 live search
f                 cycle decision filter
S                 cycle sort
R                 live rescan
?                 help
q                 quit
```

Search covers path, candidate type, recovery metadata, evidence code, and evidence detail.

Filtering and sorting are presentation operations over the canonical report. They never reclassify candidates.

Future multi-key yank/save families remain reserved by the interaction model and should be layered on this state machine rather than implemented as ad-hoc terminal commands.

## Scan lifecycle

The terminal opens immediately. Scanning happens on a worker thread.

```text
TUI init
  |
  +--> worker scan
  |      |
  |      +--> ScanResult
  |             |
render loop <---+--> Report
```

The main thread remains responsive to resize and quit events.

A scan failure is rendered in-place. A discovery-incomplete scan remains visible as partial through the report status. Rescan never mutates storage.

## Responsive behavior

Minimum supported interactive viewport is 74x18.

- width >= 124: table + persistent 48-column evidence inspector;
- narrower: full-width table, inspect overlay on demand;
- below minimum: explicit resize message rather than broken layout.

The table owns scroll state through Ratatui's `TableState`, allowing selection to remain stable as the viewport changes.

## Performance constraints

The first implementation intentionally avoids:

- per-frame filesystem calls;
- per-frame process spawning;
- an async runtime;
- background telemetry;
- unbounded animation loops;
- image protocols;
- rendering hidden candidate detail.

A typical report contains hundreds, not millions, of candidates. Rebuilding filtered index vectors during interaction is acceptable at this scale and keeps state transitions deterministic. If provider breadth changes that assumption, indexing can move to cached derived state without changing the report contract.

## Safety invariants

The TUI adds these UI-specific constraints:

1. rendering cannot mutate;
2. selection cannot authorize mutation;
3. animation cannot modify domain state;
4. search/filter/sort cannot change classification;
5. live rescan creates a new report rather than upgrading old evidence in place;
6. imported snapshots remain outside live cleanup authority;
7. partial discovery must be visible;
8. error states stay inside the terminal rather than silently falling back to destructive commands.

## Future layers

The foundation is intentionally ready for:

- command palette;
- composable filters;
- snapshot growth column;
- selected-set planning;
- report copy/save families;
- accessible screen-reader table mode;
- configurable themes and keymaps;
- mutation confirmation flow only after plan revalidation exists.

Those features should extend the application state model, not bypass it.
