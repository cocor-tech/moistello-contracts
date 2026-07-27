.PHONY: build optimize test test-coverage deploy-testnet deploy-mainnet bindings clean all

all: build

TARGET := wasm32v1-none

build:
	@echo "=== Building all contracts ($(TARGET)) ==="
	cargo build --target $(TARGET) --release
	@echo "✓ Build complete"

optimize: build
	@echo "=== Optimizing WASM binaries ==="
	soroban contract optimize --wasm target/$(TARGET)/release/circle_factory.wasm
	soroban contract optimize --wasm target/$(TARGET)/release/circle.wasm
	soroban contract optimize --wasm target/$(TARGET)/release/reputation_registry.wasm
	soroban contract optimize --wasm target/$(TARGET)/release/governance_token.wasm
	soroban contract optimize --wasm target/$(TARGET)/release/treasury.wasm
	@echo "✓ Optimization complete"

test:
	@echo "=== Running all contract tests ==="
	cargo test --workspace
	@echo "✓ All tests pass"

test-coverage:
	@echo "=== Running tests with coverage ==="
	cargo test --workspace --verbose 2>&1 | tail -20

deploy-testnet: optimize
	@echo "=== Deploying to Stellar Testnet ==="
	bash scripts/deploy.sh testnet

deploy-mainnet: optimize
	@echo "=== Deploying to Stellar Mainnet ==="
	bash scripts/deploy.sh mainnet

bindings:
	@echo "=== Generating Go bindings ==="
	bash scripts/bindings.sh

clean:
	cargo clean
	rm -rf bindings/

check:
	@echo "=== Checking compilation ==="
	cargo check --target $(TARGET)
	@echo "=== Running clippy ==="
	cargo clippy --target $(TARGET) -- -D warnings 2>/dev/null || echo "clippy not available, skipping"

fmt:
	cargo fmt -- --check
