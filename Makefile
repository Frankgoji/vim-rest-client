test:
	SHELL=/bin/bash cargo test -- --test-threads=1

enter-build:
	docker run --rm -it -v "$$(pwd):/home/rust/src" vim-rest-client-builder bash

build:
	docker run --rm -it -v "$$(pwd):/home/rust/src" vim-rest-client-builder \
		cargo build

build-local:
	cargo build --release --target=x86_64-unknown-linux-musl

build-release:
	docker run --rm -it -v "$$(pwd):/home/rust/src" vim-rest-client-builder \
		cargo build --release

builder:
	docker build . -t vim-rest-client-builder

package:
	mkdir -p build/vim-rest-client/bin
	cp target/x86_64-unknown-linux-musl/release/vim-rest-client build/vim-rest-client/bin
	mkdir -p build/vim-rest-client/start/vim-rest-client/ftplugin
	cp conf.vim build/vim-rest-client/start/vim-rest-client/ftplugin
	cd build && zip -r vim-rest-client.zip .

clean:
	rm -rf build
