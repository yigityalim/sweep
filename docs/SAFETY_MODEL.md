# Safety Model

## Threat model

Sweep's primary risk is local data loss caused by a false classification, stale plan, path substitution, symlink escape, or provider command that mutates more than expected.

Remote-code-execution risk is secondary to the integrity of local user data.

## Evidence is tri-state

Every relevant probe is conceptually:

```text
proven
refuted
unknown
```

`unknown` is not equivalent to `refuted`.

Examples:

- Git command timed out: unknown.
- Process table could not be queried: unknown.
- Path could not be read: unknown.
- No tracked descendants returned by a successful Git query: proven absence for that query.

Automatic deletion requires positive evidence, not absence of an error message.

## Recovery contracts

A safe candidate must describe how it is recreated.

Examples:

- `cargo build`;
- package-manager install;
- `zig build`;
- `swift build`.

"Can probably be downloaded again" is not a sufficient contract for mixed-state stores.

## Protected data classes

The following must never become automatically safe through a broad parent rule:

- credentials and key material;
- deployment identities;
- authored notes and memory;
- shell or editor configuration;
- application sessions;
- package-manager transaction state;
- source-controlled files;
- nested repositories;
- shared XDG roots;
- user documents.

## Identity binding

`FileIdentity` records:

- device;
- inode;
- object size;
- modification timestamp.

Future mutation must call `symlink_metadata` again immediately before acting, compare the root identity, and recompute the subtree metadata fingerprint. Any mismatch means skip.

Identity comparison is a safety gate, not a synchronization primitive. Provider-specific liveness checks may still be required.

## Symlinks

Sweep does not follow symlinks while sizing or discovering candidate contents.

A future destructive operation must validate the lexical target and the physical parent chain before mutation. A resolved path can only reduce permission; it cannot grant permission that the original path did not have.

## Size accounting

Logical size and allocated blocks are descriptive.

On APFS:

- clones may share blocks;
- snapshots may retain blocks after deletion;
- sparse files may have logical size far above allocated size.

Therefore pre-delete size is not guaranteed reclaimed capacity.

Future cleanup reporting must measure filesystem free-space change separately and label it as observed reclamation.

## Privilege

Sweep should run unprivileged.

System-wide cleanup requiring elevated access is outside the initial product scope. If privilege is ever added, the authorization boundary must be target-specific and must reuse the same plan identity checks.
