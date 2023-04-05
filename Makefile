default: build

test: build
	cargo test --all --tests

build:
	cargo build --target wasm32-unknown-unknown --release -p mock-lending-pool
	cargo build --target wasm32-unknown-unknown --release -p d-token
	cargo build --target wasm32-unknown-unknown --release -p b-token
	cargo build --target wasm32-unknown-unknown --release -p oracle
	cargo build --target wasm32-unknown-unknown --release -p mock-blend-oracle
	cargo build --target wasm32-unknown-unknown --release -p pool-factory
	cargo build --target wasm32-unknown-unknown --release -p mock-pool-factory
	cargo build --target wasm32-unknown-unknown --release -p backstop-module
	cargo build --target wasm32-unknown-unknown --release -p emitter
	cargo build --target wasm32-unknown-unknown --release -p lending-pool
	cd target/wasm32-unknown-unknown/release/ && \
		for i in *.wasm ; do \
			ls -l "$$i"; \
		done

generate-wasm: build
	mkdir -p target/wasm32-unknown-unknown/optimized
	soroban contract optimize \
		--wasm target/wasm32-unknown-unknown/release/d_token.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/d_token.wasm
	soroban contract optimize \
		--wasm target/wasm32-unknown-unknown/release/b_token.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/b_token.wasm
	soroban contract optimize \
		--wasm target/wasm32-unknown-unknown/release/oracle.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/oracle.wasm
	soroban contract optimize \
		--wasm target/wasm32-unknown-unknown/release/emitter.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/emitter.wasm
	soroban contract optimize \
		--wasm target/wasm32-unknown-unknown/release/pool_factory.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/pool_factory.wasm
	soroban contract optimize \
		--wasm target/wasm32-unknown-unknown/release/backstop_module.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/backstop_module.wasm
	soroban contract optimize \
		--wasm target/wasm32-unknown-unknown/release/lending_pool.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/lending_pool.wasm
	cd target/wasm32-unknown-unknown/optimized/ && \
		for i in *.wasm ; do \
			ls -l "$$i"; \
		done

fmt:
	cargo fmt --all

clean:
	cargo clean
