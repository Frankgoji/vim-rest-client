build:
	docker run --rm -it -v "$$(pwd):/home/rust/src" vim-rest-client-builder \
		cargo build

build-release:
	docker run --rm -it -v "$$(pwd):/home/rust/src" vim-rest-client-builder \
		cargo build --release

builder:
	docker build . -t vim-rest-client-builder
