# Sweep

**Proof-driven disk reclamation for developer Macs.**

Sweep is a macOS-first CLI that identifies developer-generated data only when it can explain why the data is reproducible. The executable is `sw`.

```console
sw scan ~/Developer
sw explain ~/Developer/project/target
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
- machine-readable JSON;
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

- Node.js `node_modules`;
- Next.js `.next`;
- Turborepo `.turbo`;
- Rust/Cargo `target`;
- Zig `.zig-cache`;
- Zig `zig-out`;
- Swift Package Manager `.build`;
- Xcode DerivedData entries when explicitly scanned.

The rule set is intentionally small. Broad names such as `build`, `dist`, and `cache` are not safe by default.

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

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and [`docs/SAFETY_MODEL.md`](docs/SAFETY_MODEL.md).

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

The workspace uses Rust 2024 edition and pins the repository toolchain while keeping the package MSRV at the Rust 2024 baseline.

## Release

A `v*` tag builds native Apple Silicon and Intel macOS archives containing the `sw` executable. Release archives are intended for the `yigityalim/tap` Homebrew tap.

See [`docs/RELEASE.md`](docs/RELEASE.md) and [`docs/REPOSITORY_SETTINGS.md`](docs/REPOSITORY_SETTINGS.md).

## License

MIT
