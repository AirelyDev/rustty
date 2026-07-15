" Vim filetype plugin file
" Language: Rustty config file
" Maintainer: Rustty <https://github.com/airely/rustty>
"
" THIS FILE IS AUTO-GENERATED

if exists('b:did_ftplugin')
  finish
endif
let b:did_ftplugin = 1

setlocal commentstring=#\ %s
setlocal iskeyword+=-

" Use syntax keywords for completion
setlocal omnifunc=syntaxcomplete#Complete

" Ask rustty to explain config keywords
setlocal keywordprg=rustty\ +explain-config

let b:undo_ftplugin = 'setl cms< isk< ofu< kp<'

if !exists('current_compiler')
  compiler rustty
  let b:undo_ftplugin .= " makeprg< errorformat<"
endif
