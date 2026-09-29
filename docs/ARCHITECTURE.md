# Architecture

## Goal

Sweep separates observation from mutation so that a filesystem scanner cannot accidentally become a deletion engine.

## Current crates

### `sweep-core`

Pure domain model:

- evidence;
- decisions;
- recovery contracts;
- candidates;
- filesystem identity;
- subtree metadata fingerprint;
- immutable plans.

It does not recursively walk the filesystem and does not invoke deletion.

### `sweep-scan`

Observation layer:

- candidate discovery;
- provider engine;
- Git ownership checks;
- size accounting;
- no-follow traversal.

It returns `Candidate` values and never mutates candidate paths.

### `sweep-cli`

Presentation and command routing:

- `scan`;
- `explain`;
- `plan`;
- `doctor`.

It contains no cleanup logic.

## Future mutation boundary

Mutation will be introduced as a separate crate only after the following API shape is stable:

```text
Plan
  |
  v
Revalidate every FileIdentity
  |
  +-- changed --> skip
  |
  +-- same --> approved mutation
                    |
                    v
              provider boundary
                    |
                    v
              measured result
```

The mutation engine receives an already-approved plan. It must not recursively discover additional candidates or broaden a target.

## Provider model

Generated state is contextual, so candidate recognition is owned by providers instead of a shared directory-name switch.

Each provider returns an assessment containing:

```text
candidate kind
recovery contract
decision floor
evidence
Git evidence policy
```

Current providers:

```text
Node      package.json + node_modules/.next/.turbo
Cargo     Cargo.toml + target
Zig       build.zig + .zig-cache/zig-out
SwiftPM   Package.swift + .build
Xcode     ~/Library/Developer/Xcode/DerivedData/<entry>
Python    project marker + .pytest_cache/.mypy_cache/.ruff_cache/.venv
Go        exact default macOS GOCACHE/GOPATH module-cache roots
```

Project-scoped providers require Git ownership evidence before becoming safe. Xcode DerivedData declares Git evidence not applicable because its recovery boundary is the Xcode-managed DerivedData root rather than a source repository.

Python tool caches require the standard cache-directory signature in addition to a recognized project boundary. A missing or unreadable signature lowers the candidate to review; an invalid signature protects it. `.venv` is recognized only with `pyvenv.cfg` and remains review-only because manually installed packages are not provably recoverable.

The Go provider currently recognizes only the default macOS build cache and default GOPATH module cache. The build cache is tool-owned generated state and does not require Git ownership evidence. The module cache is review-only because it contains downloaded dependency source whose exact recovery can depend on network access, upstream availability, and private-module credentials.

Future Go build-cache mutation must use the owner command (`go clean -cache`) rather than turning the recognized path into a generic recursive-delete capability.

Provider-specific protection is part of the provider assessment. For example, the Cargo provider protects `target` when `target/deploy/*-keypair.json` exists and lowers the decision to review when that protected subtree cannot be inspected conclusively.

A generic rule such as "directory named build is safe" is intentionally forbidden.

## Filesystem traversal

Traversal requirements:

- do not follow symlinks;
- remain on the same filesystem by default;
- skip candidate descendants after a candidate root is found;
- prune VCS metadata directories during broad discovery;
- record traversal failures;
- de-duplicate hard-linked files by `(device, inode)` for size accounting.

APFS clone/shared-extent uniqueness is not inferred from `st_blocks`.

## Plan semantics

A plan stores:

- root;
- creation time;
- candidate classification;
- measured sizes;
- evidence;
- recovery contract;
- filesystem identity.

A future apply operation will reject a candidate when either its root identity or its subtree metadata fingerprint no longer matches the plan.

This closes the common "scan one object, delete another object later at the same path" class of race.
