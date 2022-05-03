build:
	docker run --rm -it -v "$$(pwd):/home/rust/src" vim-rest-client-builder \
		cargo build

builder:
	docker build . -t vim-rest-client-builder
