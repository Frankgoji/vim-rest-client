# vim-rest-client

A Vim REST client similar to Postman for sending API requests and receiving the
responses.

## Install

The binary vim-rest-client is statically linked with jq-rs and MUSL so it should
be usable on any UNIX environment without needing any shared libraries.

To build the binary with Docker, use the make commands `make builder && make build`.

Copy `conf.vim` to `~/.vim/ftplugin` and copy vim-rest-client to `~/.vim/binary`.
