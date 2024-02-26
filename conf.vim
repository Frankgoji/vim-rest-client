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
    execute "normal! V:!~/.vim/pack/vim-rest-client/bin/vim-rest-client " . EnvFile() . "\<cr>"
    normal! za
    normal! zx
endfunction

" generate env filename from the file, should be .<filename>.env.json
function! EnvFile()
    return "." . expand("%:t") . ".env.json"
endfunction

" Function that deletes .env.json, assuming it exists in the current directory.
let g:clearenv = 1
function! ClearEnv()
    let env = EnvFile()
    if filereadable(env) && g:clearenv
        call delete(env)
        echom "Deleted " . env
    endif
endfunction

function! ClearEnvAll()
    let all = range(0, bufnr('$'))
    for b in all
        if buflisted(b)
            let path = bufname(b)
            if "rest" == fnamemodify(path, ":e")
                let name = fnamemodify(path, ":t")
                let envname = "." . name . ".env.json"
                if filereadable(envname) && g:clearenv
                    call delete(envname)
                    echom "Deleted " . envname
                endif
            endif
        endif
    endfor
endfunction

" Function that cleans up all the extra output
function! RestClientClean()
    normal! zR

    " executed (SUCCESS|ERROR)
    execute "normal! :%g/^###{/call CleanStartMarker()\<cr>"

    " ########## RESULT|ERROR
    " response
    execute "normal! :%g/^##########.*\\(RESULT\\|ERROR\\)$/call CleanResponse()\<cr>"
endfunction
function! CleanStartMarker()
    execute "normal! :s/\\s*executed\\(\\s*(\\(SUCCESS\\|ERROR\\))\\)\\?$//\<cr>"
endfunction
function! CleanResponse()
    execute "normal! V/^###}\<cr>kd"
endfunction

"""""""""" Mappings
nnoremap <buffer> <leader>rs :call RestClientSetup()<cr>
nnoremap <buffer> <leader>rl :call RestClientClean()<cr>

" With selection, filter selected text directly to vim-rest-client rather than
" filtering only the content of a foldmarker
vnoremap <buffer> <leader>rf :<c-u>execute ":'<,'>!~/.vim/pack/vim-rest-client/bin/vim-rest-client " . EnvFile()<cr>zx

" Delete .env.json if closing a .rest file.
augroup Rest
    autocmd!
    autocmd VimLeavePre *.rest :call ClearEnvAll()
augroup END
