.PHONY: test lint fmt check build clean help

test:
	cargo test

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

check: lint test

build:
	cargo build --release

clean:
	cargo clean

help:
	@echo "research-agent make targets:"
	@echo "  test   - Run all tests"
	@echo "  lint   - Run clippy with deny warnings"
	@echo "  fmt    - Format code with rustfmt"
	@echo "  check  - Lint + test"
	@echo "  build  - Release build"
	@echo "  clean  - Remove build artifacts"
	@echo "  help   - Show this message"
