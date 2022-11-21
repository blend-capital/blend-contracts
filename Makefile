default: build

all: build test

test: build
	cargo test

build:
	rm -rf target/wasm32-unknown-unknown # TODO: Determine why integration tests cache these
	cargo build --target wasm32-unknown-unknown --release -p oracle
	cargo build --target wasm32-unknown-unknown --release
	cd target/wasm32-unknown-unknown/release/ && \
		for i in *.wasm ; do \
			ls -l "$$i"; \
		done

fmt:
	cargo fmt --all

clean:
	cargo clean
