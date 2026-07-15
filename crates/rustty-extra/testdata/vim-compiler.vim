" Vim compiler file
" Language: Rustty config file
" Maintainer: Rustty <https://github.com/airely/rustty>
"
" THIS FILE IS AUTO-GENERATED

if exists("current_compiler")
  finish
endif
let current_compiler = "rustty"

CompilerSet makeprg=rustty\ +validate-config\ --config-file=%:S
CompilerSet errorformat=%f:%l:%m,%m
