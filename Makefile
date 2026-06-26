.PHONY: setup dev lint format test \
	update-dependencies-on-lockfile upgrade-dependencies

setup:
	cargo install cargo-binstall --locked
	cargo install cargo-upgrades --locked
	cargo install cargo-edit --locked
	cargo install cargo-watch --locked
	cargo binstall cargo-nextest --secure

dev:
	RUST_LOG=debug cargo watch -x 'run'

lint:
	cargo clippy --all-targets --all-features -- -D warnings

format:
	cargo fmt --all

update-dependencies-on-lockfile:
	cargo update

upgrade-dependencies:
	cargo upgrade

test:
	cargo nextest run

