.PHONY: check fmt test build run

check:
	./scripts/check.sh

fmt:
	cargo fmt --all

test:
	cargo test --workspace --all-features --locked

build:
	cargo build --workspace --release --locked

run:
	cargo run -p sweep-cli -- scan .
