.PHONY: help build build-debug start install install-local install-local-debug fmt check

PREFIX ?= /usr
BINDIR ?= $(PREFIX)/bin
LOCAL_BINDIR ?= $(HOME)/.local/bin

help:
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  help                Show this help message"
	@echo "  build               Build the project (release)"
	@echo "  build-debug         Build the project (debug)"
	@echo "  start               Run the build"
	@echo "  install             Install the release build system-wide (supports DESTDIR and PREFIX, needs sudo)"
	@echo "  install-local       Copy the release build to ~/.local/bin (no sudo, survives cargo clean)"
	@echo "  install-local-debug Copy the debug build to ~/.local/bin (no sudo, survives cargo clean)"
	@echo "  fmt                 Format the code"
	@echo "  check               Format, check and lint the code"

build:
	cargo build --release

build-debug:
	cargo build

start: build
	./target/release/lumen

install: build
	install -Dm755 target/release/lumen $(DESTDIR)$(BINDIR)/lumen

# Unlike `install`, these don't touch system paths and need no sudo. `cargo
# clean` only wipes `target/`, so re-running this after each build is what
# keeps ~/.local/bin/lumen current without a second, separate compile (the
# way `cargo install --path .` would do it).
install-local: build
	install -Dm755 target/release/lumen $(LOCAL_BINDIR)/lumen

install-local-debug: build-debug
	install -Dm755 target/debug/lumen $(LOCAL_BINDIR)/lumen

fmt:
	cargo fmt

check: fmt
	cargo check
	cargo clippy -- -D warnings
