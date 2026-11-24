# VERSION comes from Cargo.toml, so the file name always matches `mdpeek --version`.
VERSION := $(shell sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n 1)
DIST    := dist
OS      := $(shell uname -s | tr '[:upper:]' '[:lower:]')
ARCH    := $(shell uname -m | sed -e 's/x86_64/amd64/' -e 's/aarch64/arm64/')
OUT     := $(DIST)/mdpeek-$(VERSION)-$(OS)-$(ARCH)
# Release binaries are static musl builds: they need no glibc on the host.
TARGET  := $(shell uname -m)-unknown-linux-musl

.PHONY: check dist dist-name release clean

check:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	cargo test

## dist: build the static release binary for the host into $(OUT)
dist:
	rustup target add $(TARGET)
	cargo build --release --locked --target $(TARGET)
	mkdir -p $(DIST)
	cp target/$(TARGET)/release/mdpeek $(OUT)
	ldd $(OUT) 2>&1 | grep -Eq 'statically linked|not a dynamic executable' \
		|| { echo "$(OUT) is not statically linked" >&2; exit 1; }

## dist-name: print the file name that dist produces
dist-name:
	@echo $(OUT)

## release: bump to NEW, merge develop into main and tag it (no push)
release:
	sh .github/scripts/release.sh $(NEW)

## clean: remove the release binaries
clean:
	rm -f $(DIST)/mdpeek-*
