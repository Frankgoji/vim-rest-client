# vim-rest-client

A Vim REST client similar to Postman for sending API requests and receiving the
responses.

## Install

The binary vim-rest-client is statically linked with jq-rs and MUSL so it should
be usable on any UNIX environment without needing any shared libraries.

To build the binary with Docker, use the make commands `make builder && make build`.

To build the release version of the binary, use `make build-release`.

To create a zip of the plugin as a whole, use `make publish`, which will create
the zip file `vim-rest-client.zip`.

To install the plugin for vim, unzip the package inside your `~/.vim/pack`:
```
$ mkdir ~/.vim/pack
$ cp build/vim-rest-client.zip ~/.vim/pack
$ cd ~/.vim/pack
$ unzip vim-rest-client.zip
Archive:  vim-rest-client.zip
   creating: vim-rest-client/
   creating: vim-rest-client/bin/
  inflating: vim-rest-client/bin/vim-rest-client
   creating: vim-rest-client/start/
   creating: vim-rest-client/start/vim-rest-client/
   creating: vim-rest-client/start/vim-rest-client/ftplugin/
  inflating: vim-rest-client/start/vim-rest-client/ftplugin/conf.vim
```
