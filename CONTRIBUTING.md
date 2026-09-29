# Contributing

Sweep is conservative by design. New cleanup coverage is accepted only when the recovery contract is stronger than the directory-name heuristic.

## Before changing a rule

Document:

1. what created the data;
2. how the data is deterministically recovered;
3. which sibling paths are explicitly out of scope;
4. how active runtime state is detected;
5. how Git/source ownership is handled;
6. which incident fixtures protect the boundary.

"It looks like a cache" is not sufficient evidence.

## Required checks

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Destructive code

Do not add direct deletion calls to scanners, rule providers, UI code, or output code.

Mutation work belongs behind the dedicated mutation boundary described in `docs/ARCHITECTURE.md`. Until that boundary exists, destructive operations should not be introduced.
