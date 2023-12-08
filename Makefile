default: build

test: build
	cargo test --all --tests

build:
	cargo rustc --manifest-path=emitter/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	cargo rustc --manifest-path=pool-factory/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	cargo rustc --manifest-path=backstop/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	cargo rustc --manifest-path=pool/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	mkdir -p target/wasm32-unknown-unknown/optimized
	soroban contract optimize \
		--wasm target/wasm32-unknown-unknown/release/emitter.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/emitter.wasm
	soroban contract optimize \
		--wasm target/wasm32-unknown-unknown/release/pool_factory.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/pool_factory.wasm
	soroban contract optimize \
		--wasm target/wasm32-unknown-unknown/release/backstop.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/backstop.wasm
	soroban contract optimize \
		--wasm target/wasm32-unknown-unknown/release/pool.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/pool.wasm
	cd target/wasm32-unknown-unknown/optimized/ && \
		for i in *.wasm ; do \
			ls -l "$$i"; \
		done

fmt:
	cargo fmt --all

clean:
	cargo clean

generate-js:
	soroban contract bindings typescript --overwrite \
		--contract-id CBWH54OKUK6U2J2A4J2REJEYB625NEFCHISWXLOPR2D2D6FTN63TJTWN \
		--wasm ./target/wasm32-unknown-unknown/optimized/backstop.wasm --output-dir ./js/js-backstop/
	soroban contract bindings typescript --overwrite \
		--contract-id CBWH54OKUK6U2J2A4J2REJEYB625NEFCHISWXLOPR2D2D6FTN63TJTWN \
		--wasm ./target/wasm32-unknown-unknown/optimized/emitter.wasm --output-dir ./js/js-emitter/
	soroban contract bindings typescript --overwrite \
		--contract-id CBWH54OKUK6U2J2A4J2REJEYB625NEFCHISWXLOPR2D2D6FTN63TJTWN \
		--wasm ./target/wasm32-unknown-unknown/optimized/pool_factory.wasm --output-dir ./js/js-pool-factory/
	soroban contract bindings typescript --overwrite \
		--contract-id CBWH54OKUK6U2J2A4J2REJEYB625NEFCHISWXLOPR2D2D6FTN63TJTWN \
		--wasm ./target/wasm32-unknown-unknown/optimized/pool.wasm --output-dir ./js/js-pool/
