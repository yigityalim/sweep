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
- provider rules;
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

Rules are provider-specific because "generated" is contextual.

Examples:

```text
Cargo project + target + no protected deployment material
Node package + node_modules + Git ownership evidence
Zig project + .zig-cache + Git ownership evidence
Swift package + .build + Git ownership evidence
```

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
