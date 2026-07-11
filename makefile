default: build

all: test

test: build
	cargo test

build:
	stellar contract build
	@ls -l target/wasm32v1-none/release/*.wasm

fmt:
	cargo fmt --all

coverage:
	cargo llvm-cov --workspace --summary-only --fail-under-lines 90 --fail-under-regions 85

clean:
	cargo clean


.PHONY: audit
audit:
	@command -v cargo-audit >/dev/null 2>&1 || cargo install cargo-audit --locked
	cargo audit --deny warnings
 
.PHONY: deny
deny:
	@command -v cargo-deny >/dev/null 2>&1 || cargo install cargo-deny --locked
	cargo deny check
 