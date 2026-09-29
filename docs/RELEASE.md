# Release

Sweep releases native macOS archives for Homebrew and direct installation.

## Targets

- `aarch64-apple-darwin`;
- `x86_64-apple-darwin`.

Each archive contains:

- `sw`;
- `LICENSE`;
- `README.md`.

## Tagging

```console
git tag -s v0.1.0
git push origin v0.1.0
```

The release workflow verifies the tag-triggered commit, builds both native architectures, creates SHA-256 sidecars, and publishes a GitHub Release.

## Homebrew

The personal tap is:

```text
yigityalim/homebrew-tap
```

The intended user flow is:

```console
brew install yigityalim/tap/sweep
```

The tap formula should reference immutable GitHub Release archives and pin each architecture with its SHA-256 digest.

## Versioning

Sweep uses Semantic Versioning.

Before `1.0.0`, plan schemas and command output may evolve. Machine-readable schema versions must still change explicitly when compatibility breaks.
