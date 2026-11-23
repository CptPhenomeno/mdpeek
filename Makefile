# VERSION comes from Cargo.toml, so the file name always matches `mdpeek --version`.
VERSION := $(shell sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n 1)
DIST    := dist
OS      := $(shell uname -s | tr '[:upper:]' '[:lower:]')
ARCH    := $(shell uname -m | sed -e 's/x86_64/amd64/' -e 's/aarch64/arm64/')
OUT     := $(DIST)/mdpeek-$(VERSION)-$(OS)-$(ARCH)

.PHONY: check dist dist-name clean

check:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	cargo test

## dist: build the release binary for the host into $(OUT)
dist:
	cargo build --release --locked
	mkdir -p $(DIST)
	cp target/release/mdpeek $(OUT)

## dist-name: print the file name that dist produces
dist-name:
	@echo $(OUT)

## clean: remove the release binaries
clean:
	rm -f $(DIST)/mdpeek-*
