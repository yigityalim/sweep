# Security Policy

Sweep performs filesystem analysis and is expected to become a destructive system utility. Safety regressions are treated as security issues even when they do not involve remote exploitation.

## Report privately

Do not publish a proof of concept for unintended deletion, path traversal, symlink escape, privilege-boundary bypass, or release-integrity failure before a fix is available.

Use GitHub private vulnerability reporting when enabled for the repository.

## High-severity classes

The following are security-sensitive:

- deletion outside an approved plan;
- path replacement between scan and mutation;
- following a symlink into a protected tree;
- deletion of authored data classified as generated;
- deletion of credentials, keys, session state, or deployment identity;
- broadening a provider-owned cleanup command beyond its documented roots;
- privilege escalation beyond the exact mutation target;
- release artifact or checksum substitution.

## Current mutation status

The current pre-release codebase does not contain a destructive apply command. Any new mutation path must satisfy the controls documented in `docs/SAFETY_MODEL.md`.
