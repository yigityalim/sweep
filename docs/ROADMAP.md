# Roadmap

## Phase 0 — observation

Implemented foundation:

- domain model;
- scanner;
- provider classification;
- Git evidence;
- size accounting;
- `scan`;
- `explain`;
- `plan`;
- `doctor`;
- incident regressions.

## Phase 1 — plan integrity

Before deletion exists:

- stable plan schema;
- deterministic candidate identifiers;
- complete identity revalidation;
- path-parent physical containment checks;
- cancellation semantics;
- structured failure reasons;
- benchmark corpus.

## Phase 2 — reversible mutation

Introduce a dedicated mutation crate:

- apply exact plan only;
- skip changed candidates;
- provider-owned cleanup when possible;
- Trash/quarantine strategy for review-grade operations;
- operation journal;
- observed free-space delta;
- interrupted-run recovery.

## Phase 3 — developer providers

Expand only with explicit recovery contracts:

- Xcode DerivedData;
- SwiftPM;
- Cargo;
- Node package managers;
- Zig;
- Go caches;
- Python virtual environments and caches;
- Homebrew owner-supported cleanup;
- Docker build cache with strict volume exclusion.

## Explicit non-goals

- generic `~/Library` sweeping;
- app uninstall;
- launch agent cleanup;
- DNS or LaunchServices maintenance;
- battery/system monitoring;
- browser profile cleanup;
- document cleanup.
