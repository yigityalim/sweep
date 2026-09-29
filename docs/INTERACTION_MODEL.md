# Interaction Model

## Product position

Sweep is a developer storage manager and debugger, not a generic Mac cleaner.

The primary workflow is:

```text
why is this Mac full?
    |
    v
scan and attribute developer storage
    |
    v
explain safety + recovery + cost
    |
    v
select an explicit remediation plan
    |
    v
apply only after revalidation
    |
    v
measure observed reclamation
    |
    v
track growth and prevent recurrence
```

The interactive UI, CLI reports, future GUI, snapshots, and automation must share the same domain and report models.

## Research inputs

The interaction model intentionally borrows proven ideas without copying another product's scope:

- dua-cli: multi-stage deletion, configurable keybindings, candidate grouping, refresh/recheck, APFS clone de-duplication, folded-stack and flamegraph exports;
- gdu: interactive/non-interactive/export modes, JSON export/import, persistent scan storage, selected export attributes, hard-link de-duplication;
- ncdu: versioned export/import, compressed snapshots, read-only behavior for imported scans;
- dust: JSON output, screen-reader-oriented output, collapse controls;
- Yazi and Lazygit: discoverable multi-key command families, search/filter, range selection, copy/open actions, contextual menus;
- diskonaut: session-level accounting of space freed;
- kondo: project-age filtering and dry-run workflows;
- Ratatui/Crossterm: the native Rust TUI foundation;
- TachyonFX: presentation-only terminal transitions layered after semantic widgets render.

## Canonical report

`sweep-report` is the presentation contract.

Every surface consumes the same versioned report model:

```text
Candidate[]
    |
    v
Report
    |
    +-- terminal text
    +-- Markdown
    +-- JSON
    +-- TOML
    +-- clipboard
    +-- Downloads/file export
    +-- future TUI
    +-- future GUI
    +-- future snapshots/diff
```

A report is descriptive and can never authorize mutation.

Imported reports and snapshots are permanently read-only. A live rescan must produce a new plan before any cleanup action is possible.

## TUI

Running `sw` without a subcommand opens the TUI.

Main view:

```text
Sweep

ALLOCATED     DECISION    TYPE               ACTIVITY       RECOVERY        PATH
12.4 GB       safe        Xcode DerivedData  19d*           automatic       ~/Library/...
 7.8 GB       review      Docker cache       current*       medium*          docker://...
 3.1 GB       safe        node_modules       41d*           npm ci           ~/Developer/...
```

An asterisk means inferred or provider-specific metadata. Sweep must not label mtime as "last used" unless the provider can prove that semantic.

## Key families

Navigation:

```text
j / down       next
k / up         previous
g g            top
G              bottom
enter          inspect
h / left       back
l / right      enter group
tab            next pane
shift+tab      previous pane
/              search
f              filter
S              sort
space          toggle selection
v              range selection
R              rescan current scope
r              revalidate current item
?              help
q              quit
```

Yank/copy is a command family so Vim navigation keeps `j`:

```text
y y            human-readable summary
y p            path
y m            Markdown
y j            JSON
y t            TOML
```

Save/export mirrors the same formats:

```text
s y            text
s m            Markdown
s j            JSON
s t            TOML
```

Save defaults to a timestamped file under `~/Downloads`. The UI must show the exact destination.

Other actions:

```text
e              explain evidence
r              recovery details / revalidate by context
o              reveal in Finder or open provider location
:              action palette
[ / ]          previous / next view
```

Do not bind a single unmodified key to immediate permanent deletion. Cleanup remains plan-based and multi-stage.

## Preview operations

The current TUI includes read-only workflow previews before mutation exists:

```text
1  candidates
2  browse
3  growth
4  history
```

Candidate selection uses `space`. `v` starts/stops range selection and extends with normal navigation. `a` marks all currently visible `safe` candidates, `u` clears marks, and `x` toggles a selected-only view. These controls change presentation state only.

`c` opens a side-panel cleanup preview. Only candidates already classified `safe` are eligible; review and protected candidates remain visible as excluded. Inside the drawer, `j/k` moves through eligible items, `space` enables/disables an item, `a` enables all, and `u` disables all. Disabled safe items stay visible and are not counted in the preview estimate. Confirming the preview stops at a receipt-like boundary and explicitly reports that no files changed.

Browse is bounded to the current scan scope. It supports parent/child navigation without following symlink directories. `o` reveals the selected path in Finder through `/usr/bin/open -R`.

The action palette (`:`) exposes scope changes, report export, clipboard copy, Finder reveal, view navigation, rescan, and clean preview. It is searchable by typing and uses arrow keys for selection. Report export uses the canonical report model.

The documented copy/save families are active in the TUI:

```text
y y            copy text report
y p            copy selected path
y m            copy Markdown report
y j            copy JSON report
y t            copy TOML report

s y            save text report
s m            save Markdown report
s j            save JSON report
s t            save TOML report
```

The inspect overlay is scrollable with `j/k`, arrow keys, and page up/down. Its evidence summary reports proven/unknown/refuted counts without reinterpreting classification.

Growth uses the latest valid snapshot for the exact current scope as a baseline once a live scan completes. The Candidates table shows a `Δ SNAPSHOT` column and can sort by growth; the inspector shows the selected candidate's live delta. Before a live scan exists, the Growth view can still compare the latest two valid snapshots. Invalid snapshots are ignored with an explicit count.

History now has two intentionally separate concepts. During preview-only development, confirming a clean preview creates a volatile in-memory session receipt so the future history UX can be exercised. It records the preview scope, enabled safe candidates, excluded count, paths, timestamp, and allocated-size estimate. It is never written to disk and is clearly labeled `NO FILES CHANGED`. Persistent cleanup history remains reserved for a future mutation engine that can record revalidation, mutation outcome, and measured reclaimed bytes.

Option-left / option-right traverses scan-scope history. Changing scope always starts a fresh live scan.

## Explain view

Explain must answer developer-specific questions rather than merely naming a cache:

- what produced this data;
- why Sweep classified it safe, review, or protected;
- which evidence was proven, unknown, or refuted;
- logical size;
- allocated-size estimate;
- whether the traversal was complete;
- recovery mechanism;
- recovery command when deterministic;
- expected network dependency;
- expected compute cost;
- expected time cost;
- activity evidence and its semantic quality;
- active process/project impact when provable.

Recovery cost should become typed provider metadata rather than prose.

Suggested future model:

```text
RecoveryProfile
├── mechanism
├── automatic
├── network_cost
├── compute_cost
├── time_cost
├── requires_credentials
└── command
```

Cost classes are qualitative until Sweep has measured local history. Do not invent minute estimates.

## Search, filters, and sorting

Search operates on path, provider, kind, evidence code, and recovery command.

Filters should compose:

- decision: safe/review/protected;
- provider;
- project;
- minimum size;
- age/activity evidence;
- recovery network cost;
- recovery compute cost;
- selected only;
- changed since snapshot.

Sort keys:

- allocated estimate;
- growth since snapshot;
- logical size;
- decision;
- provider;
- project;
- activity evidence;
- recovery cost;
- growth since snapshot.

## Snapshots and growth

Current commands:

```console
sw snapshot ~/Developer
sw snapshot ~/Developer --output ~/Downloads/dev.sweep.json
sw diff old.sweep.json new.sweep.json
sw diff old.sweep.json new.sweep.json --format markdown
```

Future history command:

```console
sw history
```

Snapshots use a dedicated versioned schema. New snapshots use schema v2; schema v1 remains readable for compatibility.

Important properties:

- imported snapshots are read-only;
- new snapshots store candidate locations only as paths relative to the snapshot root; legacy v1 absolute candidate paths are accepted on read but never emitted by v2;
- candidates that cannot be proven to reside under the requested snapshot root are omitted and force the snapshot incomplete;
- imported candidate relative paths are validated before diffing; absolute, empty, or parent-traversing relative paths are rejected;
- discovery errors and incomplete candidate traversal keep the snapshot marked incomplete;
- default snapshot filenames never overwrite an existing same-second snapshot; a numeric suffix is allocated instead;
- a process interrupted before atomic write cannot leave a complete-looking snapshot;
- diffs match relative path+kind first and only use filesystem identity for unique unmatched moves;
- snapshot roots must match before diffing;
- growth is reported independently from cleanup eligibility.

This powers the "prevent" product direction without requiring a permanent daemon.

## Budgets and prevention

Future opt-in local policy:

```text
Cargo targets       budget 15 GB
Xcode DerivedData   budget 10 GB
Docker              budget 20 GB
pnpm store          budget  8 GB
```

Sweep can evaluate budgets manually or through an optional launchd schedule. No cloud service and no telemetry are required.

Budget alerts never delete automatically.

## Additional exports

Useful future exporters:

- NDJSON for streaming/large reports;
- self-contained static HTML for offline sharing;
- folded-stack format for flamegraphs;
- compact snapshot format for long-term history;
- CSV only for flat summary tables, not as the canonical evidence format.

Static HTML must have no external network dependency by default.

## Clipboard

macOS uses `pbcopy` for the first implementation.

A future terminal-aware fallback may support OSC 52 for SSH/multiplexer workflows, but clipboard transport must remain separate from report serialization.

Clipboard failure must never affect classification or scan results.

## Accessibility

Future TUI requirements:

- no-color mode;
- no-Unicode mode;
- screen-reader-friendly table mode without bars or decorative glyphs;
- keyboard-only operation;
- terminal resize handling;
- explicit partial/incomplete scan text, not color alone.

## APFS

A future optional APFS-accurate mode may collect clone-sharing metadata, similar to dua-cli's opt-in clone de-duplication.

It must remain opt-in if metadata collection materially slows scanning.

Even APFS-aware pre-delete accounting remains an estimate when snapshots or partial shared extents are involved. Observed free-space change after mutation remains the authoritative reclamation measurement.

## Persistent local state

Potential location:

```text
~/Library/Application Support/Sweep/
├── config.toml
├── snapshots/
├── history/
└── state/
```

Configuration may eventually include keybindings, default scan roots, ignored paths, budgets, report defaults, and provider-specific policy.

Persisted state must never turn an old classification into current deletion authority.

## Scope ordering

Implementation order:

1. provider evidence and scanner correctness;
2. canonical report/export model;
3. snapshot/diff and growth model;
4. TUI on top of the report model;
5. immutable plan revalidation;
6. mutation engine;
7. measured reclamation history;
8. optional budgets and launchd scheduling.

Homebrew, Docker, simulator runtimes, Playwright browsers, IDE/agent caches, and other stateful providers should be added only when their owner/liveness contracts are understood. Provider count is not a success metric.
