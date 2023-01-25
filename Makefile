default: build

test: build
	cargo test --all --tests

build:
	cargo build --target wasm32-unknown-unknown --release -p oracle
	cargo build --target wasm32-unknown-unknown --release -p emitter
	cargo build --target wasm32-unknown-unknown --release -p pool-factory
	cargo build --target wasm32-unknown-unknown --release -p backstop-module
	cargo build --target wasm32-unknown-unknown --release -p lending-pool
	cargo build --target wasm32-unknown-unknown --release
	cd target/wasm32-unknown-unknown/release/ && \
		for i in *.wasm ; do \
			ls -l "$$i"; \
		done

fmt:
	cargo fmt --all

clean:
	cargo clean
