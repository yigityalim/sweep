# Sweep

**Proof-driven disk reclamation for developer Macs.**

Sweep is a macOS-first CLI that identifies developer-generated data only when it can explain why the data is reproducible. The executable is `sw`.

```console
sw
sw scan ~/Developer
sw explain ~/Developer/project/target
sw report ~/Developer --format markdown
sw snapshot ~/Developer
sw diff before.sweep.json after.sweep.json
sw plan ~/Developer
sw doctor
```

Sweep is intentionally conservative. It does not infer that user-created data is disposable from a directory name alone.

## Status

Sweep is pre-release software.

The current repository implements the non-destructive foundation:

- candidate discovery;
- recovery-contract classification;
- filesystem identity capture;
- subtree metadata fingerprints for stale-plan detection;
- logical and allocated-size accounting;
- hard-link de-duplication;
- no-follow symlink traversal;
- Git ownership evidence;
- protected-artifact detection;
- immutable plan generation;
- versioned text, Markdown, JSON, and TOML reports;
- macOS clipboard and Downloads export;
- machine-readable JSON;
- interactive Ratatui dashboard with background scanning, search, filters, evidence inspection, and restrained terminal animation;
- incident-driven regression tests.

**No command in the current version deletes files.**

Mutation will be introduced only after plan revalidation, recovery boundaries, and rollback behavior are implemented and tested.

## Core invariant

> Sweep must never delete a byte unless it can prove a bounded recovery contract for that byte.

A directory named `target`, `build`, or `cache` is not proof. A candidate becomes safe only after its provider-specific evidence passes.

## Why this exists

Developer machines accumulate large quantities of reproducible state:

- dependency trees;
- compiler outputs;
- incremental build state;
- framework build directories;
- package-manager caches;
- simulator and IDE build data.

General-purpose cleaners frequently mix this with application state, user content, credentials, session data, or shared runtime state. Sweep deliberately narrows the problem to developer storage and models uncertainty explicitly.

## Commands

### Interactive TUI

```console
sw
```

Running `sw` without a subcommand opens the read-only interactive dashboard. It scans `~/Developer` when that directory exists and otherwise uses the current directory. The TUI provides keyboard navigation, live search, decision filters, sort modes, evidence inspection, read-only file browsing, snapshot growth, scope navigation, Finder reveal, report export, cleanup-plan previews, background rescans, responsive layouts, `NO_COLOR` support, and an ASCII mode through `SWEEP_ASCII=1`.

The TUI is a report surface only. Selection and animation do not authorize mutation. Cleanup actions currently stop at an explicit preview receipt; no file deletion is enabled.

### Scan

```console
sw scan ~/Developer
sw scan ~/Developer --json
```

`scan` discovers supported artifact roots, classifies each candidate, measures it without following symlinks, and sorts by allocated-size estimate.

### Explain

```console
sw explain ./target
sw explain ./node_modules --json
```

`explain` prints the evidence chain for one path. Unsupported or ambiguous paths are protected rather than guessed.

### Report

```console
sw report ~/Developer
sw report ~/Developer --format markdown
sw report ~/Developer --format json --copy
sw report ~/Developer --format toml --save
sw report ~/Developer --format markdown --redact-home
```

`report` renders the same canonical storage model as text, Markdown, JSON, or TOML. Reports contain explicit decision counts and allocated-size estimates; they never relabel estimates as guaranteed reclaimable capacity. Text and Markdown omit non-protected 0 B allocated-size candidates from the detailed view to reduce workspace-link noise; JSON and TOML retain the complete canonical candidate set.

`--copy` writes the report to the macOS clipboard. `--save` writes a timestamped file into `~/Downloads`. `--output <path>` writes atomically to a chosen location. `--redact-home` replaces the current home-directory prefix with `~` for shareable reports.

The report schema is versioned independently from deletion plans so snapshots and diffs remain read-only data rather than mutation authority.

### Snapshot and diff

```console
sw snapshot ~/Developer
sw snapshot ~/Developer --output ~/Downloads/dev.sweep.json
sw diff before.sweep.json after.sweep.json
sw diff before.sweep.json after.sweep.json --format markdown
sw diff before.sweep.json after.sweep.json --format json --output ~/Downloads/diff.json
```

A snapshot records candidate sizes, decisions, fingerprints, lightweight filesystem identity, and scan completeness. Without `--output`, snapshots are stored under `~/Library/Application Support/Sweep/snapshots`.

`diff` compares two snapshots of the same root and reports growth, shrinkage, additions, removals, moves, and decision changes. Exact path+kind matches take precedence; unmatched candidates may be recognized as moves only when a unique filesystem `(device, inode)` identity matches.

Snapshots are historical observations only. They cannot authorize cleanup, and an incomplete snapshot remains explicitly marked incomplete.

### Plan

```console
sw plan ~/Developer
sw plan ~/Developer --output sweep-plan.json
```

`plan` serializes only candidates currently classified as safe. The plan records each path's filesystem identity so a future apply phase can refuse changed candidates instead of deleting a different object at the same path.

The current version cannot apply plans.

### Doctor

```console
sw doctor
sw doctor --json
```

`doctor` reports the runtime platform, Git availability, and filesystem type probes relevant to Sweep.

## Classification

Sweep has three decisions:

| Decision | Meaning |
| --- | --- |
| `safe` | A supported recovery contract is proven and no protection evidence failed. |
| `review` | The path may be reproducible, but required ownership or recovery evidence is incomplete. |
| `protected` | The path is unsupported, contains protected state, is tracked as source, or violates a safety boundary. |

`unknown` evidence is never converted to `false`. Uncertainty lowers trust.

## Supported artifact families

The initial rule set recognizes:

- Node.js `node_modules`, including repository-scoped workspace lockfile inheritance and npm/pnpm/Yarn/Bun lockfiles;
- Next.js `.next`;
- Turborepo `.turbo`;
- Rust/Cargo `target`;
- Zig `.zig-cache`;
- Zig `zig-out`;
- Swift Package Manager `.build`;
- Xcode DerivedData entries when explicitly scanned.
- Go's default macOS build cache (`~/Library/Caches/go-build`);
- Go's default module cache (`~/go/pkg/mod`) as review-only;
- pytest `.pytest_cache`;
- mypy `.mypy_cache`;
- Ruff `.ruff_cache`;
- Python `.venv` environments as review-only.

The rule set is intentionally small. Python tool caches require a Python project boundary, a standard cache-directory tag, and Git ownership evidence before becoming safe. Virtual environments remain review-only because they may contain undeclared packages. Go module caches remain review-only because exact re-download depends on module availability and credentials. Broad names such as `build`, `dist`, and `cache` are not safe by default.

## Size semantics

Sweep reports two values:

- **logical bytes**: file lengths visible through filesystem metadata;
- **allocated bytes estimate**: allocated 512-byte block accounting from `st_blocks`.

On APFS, clone/shared-extent semantics mean summed allocated bytes are not guaranteed to equal bytes that would become free after deletion. Sweep therefore labels this value as an estimate and does not present it as guaranteed reclaimable capacity.

Actual reclaimed capacity will be measured at the mutation boundary in a future release.

## Git evidence

For project-scoped artifacts, Sweep prefers proof that:

1. the candidate belongs to a recognized project;
2. Git reports no tracked descendants;
3. the candidate is ignored;
4. no nested repository boundary is present;
5. provider-specific protected signatures are absent.

If Git is unavailable, the path is not automatically promoted to safe.

## Safety lessons encoded from real incidents

The test suite includes regression cases derived from failures observed in mature macOS cleaner projects, including:

- deployment key material stored under otherwise-generated build trees;
- authored AI-tool memory mistaken for cache;
- XDG roots matched by GUI application names;
- active package-manager transaction state;
- nested candidates double-counted during previews;
- symlink-heavy trees inflating disk usage.

See [`docs/INCIDENTS.md`](docs/INCIDENTS.md).

## Architecture

```text
filesystem
    |
    v
discovery
    |
    v
provider rules
    |
    v
evidence + recovery contract
    |
    v
candidate classification
    |
    v
immutable plan
    |
    v
[future] identity revalidation
    |
    v
[future] mutation boundary
```

Scanners do not mutate. Rules do not mutate. Plans do not mutate. The future mutation engine will not be allowed to discover new targets.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), [`docs/SAFETY_MODEL.md`](docs/SAFETY_MODEL.md), and [`docs/INTERACTION_MODEL.md`](docs/INTERACTION_MODEL.md).

## Development

Requirements:

- macOS;
- Rust 1.98.1 via `rustup`;
- Git.

Initialize the checkout once:

```console
./scripts/bootstrap.sh
```

This installs/selects the pinned toolchain, generates `Cargo.lock`, and runs the complete verification suite. Commit the generated lockfile before the first push.

Normal development:

```console
./scripts/check.sh
cargo run -p sweep-cli -- scan ~/Developer
```

The workspace uses Rust 2024 edition, pins Rust 1.98.1 for repository development, and declares Rust 1.88 as the package MSRV for the current Ratatui stack.

## Release

A `v*` tag builds native Apple Silicon and Intel macOS archives containing the `sw` executable. Release archives are intended for the `yigityalim/tap` Homebrew tap.

See [`docs/RELEASE.md`](docs/RELEASE.md) and [`docs/REPOSITORY_SETTINGS.md`](docs/REPOSITORY_SETTINGS.md).

## License

MIT
