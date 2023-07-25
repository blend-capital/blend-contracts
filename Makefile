default: build

test: build
	cargo test --all --tests

build:
	cargo rustc --manifest-path=mocks/mock-lending-pool/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	cargo rustc --manifest-path=mocks/mock-oracle/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	cargo rustc --manifest-path=pool-factory/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	cargo rustc --manifest-path=backstop-module/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	cargo rustc --manifest-path=emitter/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	cargo rustc --manifest-path=lending-pool/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	mkdir -p target/wasm32-unknown-unknown/optimized
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
