# Repository Settings

Recommended GitHub configuration for `yigityalim/sweep`.

## Metadata

Description:

```text
Proof-driven disk reclamation for developer Macs.
```

Topics:

```text
rust
macos
cli
developer-tools
disk-usage
disk-cleaner
filesystem
homebrew
open-source
```

## Merge policy

Recommended:

- squash merge only;
- delete head branches after merge;
- require pull requests for `main`;
- require the `CI` workflow;
- require branches to be up to date before merge;
- block force pushes;
- block branch deletion.

## Security

Enable:

- private vulnerability reporting;
- dependency graph;
- Dependabot alerts;
- Dependabot security updates;
- secret scanning;
- push protection.

## Actions

Use repository Actions only for this repository and trusted first-party actions.

Every action in committed workflows is pinned to a full commit SHA. Dependabot is responsible for proposing updates.

## Releases

Release publishing should require the tag-triggered workflow. Do not upload replacement assets to an existing stable release. Publish a new patch version instead.
