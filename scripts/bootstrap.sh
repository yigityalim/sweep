#!/usr/bin/env bash
set -euo pipefail

if ! command -v rustup >/dev/null 2>&1; then
  printf '%s\n' "rustup is required: https://rustup.rs" >&2
  exit 1
fi

rustup toolchain install 1.98.1 --profile minimal --component clippy,rustfmt
rustup override set 1.98.1

cargo generate-lockfile
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --locked

printf '%s\n' "Sweep development environment is ready."
printf '%s\n' "Commit Cargo.lock before pushing the repository."
