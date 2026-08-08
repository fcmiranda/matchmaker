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
