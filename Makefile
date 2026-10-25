# TAP build tool, subject VII.1: each target runs cargo in the three crates.
#
#   make install          fetch the dependencies of every crate
#   make run-server       start the server on port 7878
#   make run-client       start the CLI client  (HOST=... PORT=... to change)
#   make run-client-gui   start the GUI client  (HOST=... PORT=... to change)
#   make lint             rustfmt check, then clippy with warnings as errors
#   make clean            remove build outputs and server logs
#
# Also: make build, make test, make format.

PROJECT := tap_project
SERVER  := $(PROJECT)/tap_server
CLIENT  := $(PROJECT)/tap_client
GUI     := $(PROJECT)/tap_gui
CRATES  := $(SERVER) $(CLIENT) $(GUI)

HOST ?= 127.0.0.1
PORT ?= 7878

.PHONY: all build install run-server run-client run-client-gui test lint format clean

all: build

build:
	@for crate in $(CRATES); do \
		cargo build --manifest-path $$crate/Cargo.toml || exit 1; \
	done

install:
	@for crate in $(CRATES); do \
		cargo fetch --manifest-path $$crate/Cargo.toml || exit 1; \
	done

# The server reads data/world.yaml and writes logs/ in its own directory.
run-server:
	cd $(SERVER) && cargo run

run-client:
	cargo run --manifest-path $(CLIENT)/Cargo.toml -- $(HOST) $(PORT)

run-client-gui:
	cargo run --manifest-path $(GUI)/Cargo.toml -- $(HOST) $(PORT)

test:
	@for crate in $(CRATES); do \
		cargo test --manifest-path $$crate/Cargo.toml || exit 1; \
	done

lint:
	@for crate in $(CRATES); do \
		echo "lint $$crate"; \
		cargo fmt --manifest-path $$crate/Cargo.toml --check || exit 1; \
		cargo clippy --manifest-path $$crate/Cargo.toml --all-targets -- -D warnings || exit 1; \
	done

format:
	@for crate in $(CRATES); do \
		cargo fmt --manifest-path $$crate/Cargo.toml || exit 1; \
	done

clean:
	@for crate in $(CRATES); do \
		cargo clean --manifest-path $$crate/Cargo.toml || exit 1; \
	done
	rm -rf $(SERVER)/logs
