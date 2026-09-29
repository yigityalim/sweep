# Sweep Engineering Contract

This file is normative for automated coding agents and contributors.

## Product boundary

Sweep is a proof-driven disk reclamation engine for developer Macs.

Do not broaden Sweep into a generic macOS cleaner, app uninstaller, system optimizer, battery monitor, process monitor, or user-data janitor.

## Non-negotiable invariants

1. Discovery cannot mutate.
2. Classification cannot mutate.
3. Provider rules cannot mutate arbitrary paths.
4. Planning cannot mutate.
5. A future mutation engine must not discover new targets.
6. Unknown evidence fails closed.
7. Symlinks are never followed during candidate traversal.
8. A directory name alone never proves recoverability.
9. Filesystem identity is rebound immediately before mutation.
10. A changed candidate is skipped, not reinterpreted.
11. Authored, credential, deployment, session, and configuration data are protected.
12. Shared roots such as `~/.local`, `~/.config`, and `~/.cache` are never selected from fuzzy application-name matching.
13. Source-controlled descendants block automatic deletion.
14. Nested repositories block automatic deletion.
15. Reported allocated size is an estimate on filesystems with shared extents.
16. Reclaimed bytes are measured after mutation rather than inferred from pre-delete sums.
17. Reports and imported snapshots are descriptive data and never authorize mutation.
18. Report formats must label allocated bytes as estimates and must not call them guaranteed reclaimable bytes.
19. Home-directory redaction changes presentation only; it must never affect filesystem identity or classification.
20. Clipboard, export, TUI, and future GUI surfaces must consume the same versioned report model.
21. Snapshot completeness must preserve discovery and candidate-traversal uncertainty.
22. Snapshot diff identity may describe historical continuity but never proves current filesystem identity.
23. Imported snapshots and diffs cannot be converted directly into mutation plans.
24. TUI cleanup previews are descriptive only and must not call mutation code or write cleanup-history receipts.
25. TUI Browse is read-only; it must never expose arbitrary filesystem deletion.
26. Finder reveal, report export, clipboard, navigation, and action-palette commands never alter classification or deletion authority.
27. TUI range selection, bulk selection, selected-only views, and clean-preview item toggles are presentation state only; they cannot promote review/protected candidates into an eligible plan.
28. Preview receipts may exist only in memory for the current TUI session. They must never be persisted as cleanup history and must never claim reclaimed bytes or successful mutation.
29. New snapshot candidate records must not persist absolute candidate paths; relative-to-root paths are sufficient for historical comparison.
30. A candidate outside the declared snapshot root must never be rewritten into an absolute-looking relative path. It must be excluded and the snapshot marked incomplete.
31. Default snapshot creation must not overwrite an existing snapshot path on timestamp collision.
32. An ancestor Node lockfile proves recovery only when the package's workspace membership is independently proven; repository ancestry alone is insufficient.
33. An immutable plan must validate before persistence or revalidation: every target must be safe, root-contained, traversal-complete, identity-bound, fingerprint-bound, and recoverable.
34. Plan revalidation may inspect only targets already present in the plan. It must never discover, classify, add, replace, or broaden targets.
35. Any identity mismatch, subtree fingerprint mismatch, symlink replacement, missing target, containment failure, or unverifiable state prevents that candidate from being considered unchanged.

## Rust rules

- Use Rust 2024 idioms.
- Do not use `unsafe` without a documented platform requirement and a focused test.
- Avoid panics for user-controlled filesystem state.
- Preserve typed distinctions between `proven`, `refuted`, and `unknown`.
- Do not collapse tri-state evidence into a boolean.
- Prefer explicit domain types over strings at safety boundaries.
- Avoid shell pipelines for filesystem mutation.
- External commands must receive arguments as separate argv entries through `std::process::Command`.
- Do not parse human-oriented command output when a stable machine interface exists.
- No hidden network access.
- No telemetry.

## Rule acceptance

A new `safe` rule must include:

- a recognized producer;
- a bounded root;
- a documented recovery contract;
- tracked-descendant protection where source control applies;
- nested-repository protection;
- provider-specific sensitive signatures;
- incident tests for plausible false positives.

If any required evidence cannot be obtained, classify as `review` or `protected`.

## Review priority

Changes touching classification, identity, plan serialization, report schema, path redaction, symlink behavior, Git ownership, or future mutation code require line-by-line safety review.
