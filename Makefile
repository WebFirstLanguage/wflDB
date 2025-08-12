# wflDB Development Makefile

.PHONY: help build test bench clean lint fmt check-fmt run-server

# Default target
help:
	@echo "wflDB Development Commands:"
	@echo "  build        - Build all crates"
	@echo "  test         - Run all tests"
	@echo "  bench        - Run performance benchmarks"
	@echo "  lint         - Run clippy linting"
	@echo "  fmt          - Format code with rustfmt"
	@echo "  check-fmt    - Check code formatting"
	@echo "  run-server   - Run development server"
	@echo "  clean        - Clean build artifacts"

# Build all crates
build:
	cargo build --workspace

# Build optimized release
build-release:
	cargo build --workspace --release

# Run all tests
test:
	cargo test --workspace

# Run tests with output
test-verbose:
	cargo test --workspace -- --nocapture

# Run benchmarks
bench:
	cargo bench --workspace

# Run specific benchmark suite
bench-hotpath:
	cargo bench --bench hot_path

# Run benchmarks and generate HTML reports
bench-html:
	cargo bench --workspace -- --output-format html

# Lint code
lint:
	cargo clippy --workspace -- -D warnings

# Format code
fmt:
	cargo fmt --all

# Check formatting
check-fmt:
	cargo fmt --all -- --check

# Run development server
run-server:
	cargo run --bin wfldb-server -- --bind 127.0.0.1:8080 --data-dir ./dev-data

# Run server in release mode
run-server-release:
	cargo run --release --bin wfldb-server -- --bind 127.0.0.1:8080 --data-dir ./dev-data

# Clean build artifacts
clean:
	cargo clean
	rm -rf dev-data/

# Install development dependencies
install-deps:
	cargo install cargo-criterion
	cargo install cargo-tarpaulin

# Run tests with coverage
coverage:
	cargo tarpaulin --workspace --out Html

# Quick development cycle
dev-cycle: check-fmt lint test

# Pre-commit checks
pre-commit: check-fmt lint test bench-hotpath

# Setup development environment
setup:
	mkdir -p dev-data
	@echo "Development environment ready!"
	@echo "Run 'make run-server' to start the server"

# Performance profiling (requires flamegraph)
profile:
	cargo bench --bench hot_path -- --profile-time=5

# Docker build (if Dockerfile exists)
docker-build:
	docker build -t wfldb:latest .

# Show project statistics
stats:
	@echo "=== Project Statistics ==="
	@find . -name "*.rs" -not -path "./target/*" | xargs wc -l | tail -1
	@echo "Crates:"
	@ls -1 */Cargo.toml | wc -l
	@echo "Tests:"
	@grep -r "#\[test\]" --include="*.rs" . | wc -l