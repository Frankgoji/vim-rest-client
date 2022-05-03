" Function setting up REST Client properties
" Defines fold markers and mappings
function! RestClientSetup()
    setlocal foldmethod=marker
    setlocal foldmarker=###{,###}
    nnoremap <buffer> <leader>rc :call RestClientFilter()<cr>
    nnoremap <buffer> <leader>ce :call ClearEnv()<cr>
endfunction

" Function that filters the content of the fold under the cursor to the REST
" Client script which will return the content as well as the response.
function! RestClientFilter()
    try
        normal! zc
    catch
        return
    endtry
    execute "normal! V:!~/.vim/binary/vim-rest-client\<cr>"
    normal! za
endfunction

" Function that deletes .env.json, assuming it exists in the current directory.
function! ClearEnv()
    if filereadable(".env.json")
        call delete(".env.json")
        echom "Deleted .env.json"
    endif
endfunction

nnoremap <buffer> <leader>rs :call RestClientSetup()<cr>

" Delete .env.json if closing a .rest file.
augroup Rest
    autocmd!
    autocmd VimLeavePre *.rest :call ClearEnv()
augroup END
