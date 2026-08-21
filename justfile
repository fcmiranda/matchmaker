default:
	@just --list

build:
	cargo build --workspace

install:
	cargo build --release --workspace
	mkdir -p "$HOME/.local/bin"
	install -m 755 target/release/mm "$HOME/.local/bin/mm"

# Run the CLI locally; pass extra args after `--`, e.g. `just preview -- --help`
run *args:
	cargo run -p matchmaker-cli -F experimental -- {{args}}

# Build static x86_64 binary for Linux (musl)
build-x86:
	cargo zigbuild --release --target x86_64-unknown-linux-musl

# Build static ARM64 binary for Linux (musl)
build-arm:
	cargo zigbuild --release --target aarch64-unknown-linux-musl

