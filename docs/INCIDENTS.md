# Incident-Derived Regression Corpus

Sweep treats mature cleaner bug histories as input to the design.

This document records failure classes, not blame.

## Build directory contains deployment identity

A Rust `target/` tree can contain Anchor/Solana program keypairs under:

```text
target/deploy/*-keypair.json
```

These are not ordinary build artifacts and may represent deployed program identities.

**Sweep rule:** a Cargo `target` containing this signature is `protected`.

Regression: `rust_target_with_anchor_keypair_is_protected`.

## AI-tool memory mistaken for cache

Developer tools can mix cached and authored state under one product root.

Examples include project memory, notes, skills, settings, or sessions.

**Sweep rule:** no generic home-directory AI-tool cache sweep exists. Unsupported roots are protected.

## GUI and CLI name collision

A GUI app display name can collide with XDG or dot-directory data for an unrelated CLI.

Examples:

```text
Local.app  -> ~/.local
Claude.app -> ~/.claude
```

**Sweep rule:** application-name fuzzy matching is not part of the architecture.

## Active package-manager transaction state

Incomplete downloads and lock files may be live state, not stale garbage.

**Sweep rule:** package-manager cleanup should prefer owner-supported cleanup APIs. Transaction roots are not deleted by generic filesystem rules.

## Nested candidate double counting

A parent candidate and a child candidate can represent the same bytes.

**Sweep rule:** discovery stops descending once it accepts a candidate root. Plan accounting never sums a descendant separately under an accepted ancestor.

## Symlink-heavy size inflation

Recursive scanners can accidentally follow or mis-account cross-tree symlinks.

**Sweep rule:** symlink traversal is disabled. Symlink objects may be measured, but their targets are not traversed.

## Sources

The initial cases were derived from public issue reports in `tw93/Mole`, including issues 823, 906, 993, 1446, 1551, 1594, 1602, and 1607. Sweep's tests encode the failure class rather than depending on Mole implementation details.
