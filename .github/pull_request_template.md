## Change

Describe what changed and why.

## Safety

- [ ] This change does not broaden a recovery contract from a directory name alone.
- [ ] Unknown probe results fail closed.
- [ ] Symlink behavior remains no-follow.
- [ ] Source-controlled and nested-repository protection remains intact.
- [ ] New safe rules include incident regression coverage.
- [ ] No destructive operation was added outside the mutation boundary.

## Verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features`
