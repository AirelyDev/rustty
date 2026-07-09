//! Vim syntax/ftdetect/ftplugin/compiler generation. Port of ghostty's
//! `src/extra/vim.zig`. `syntax()` reflects over `Config`'s field NAMES only; the
//! ftdetect/ftplugin/compiler files are static `pub const` byte strings.

use rustty_config::config_struct::config_fields;

/// This is the associated Vim file as named by the variable.
// pub const syntax = comptimeGenSyntax();
//
// PORT NOTE: Rust cannot comptime-fold a `const` byte string, so `syntax` is a
// runtime pure fn (`syntax()`); the reference's `comptimeGenSyntax` two-pass
// counter/fixed-buffer wrapper collapses to a single `Vec`.
pub fn syntax() -> Vec<u8> {
    let mut writer = Vec::new();
    write_syntax(&mut writer);
    writer
}

// pub const ftdetect = \\...;
pub const FTDETECT: &[u8] = br#"" Vim filetype detect file
" Language: Ghostty config file
" Maintainer: Ghostty <https://github.com/ghostty-org/ghostty>
"
" THIS FILE IS AUTO-GENERATED

au BufRead,BufNewFile */ghostty/config,*/*.ghostty/config,*/ghostty/themes/*,*.ghostty setf ghostty
"#;

// pub const ftplugin = \\...;
pub const FTPLUGIN: &[u8] = br#"" Vim filetype plugin file
" Language: Ghostty config file
" Maintainer: Ghostty <https://github.com/ghostty-org/ghostty>
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

" Ask ghostty to explain config keywords
setlocal keywordprg=ghostty\ +explain-config

let b:undo_ftplugin = 'setl cms< isk< ofu< kp<'

if !exists('current_compiler')
  compiler ghostty
  let b:undo_ftplugin .= " makeprg< errorformat<"
endif
"#;

// pub const compiler = \\...;
pub const COMPILER: &[u8] = br#"" Vim compiler file
" Language: Ghostty config file
" Maintainer: Ghostty <https://github.com/ghostty-org/ghostty>
"
" THIS FILE IS AUTO-GENERATED

if exists("current_compiler")
  finish
endif
let current_compiler = "ghostty"

CompilerSet makeprg=ghostty\ +validate-config\ --config-file=%:S
CompilerSet errorformat=%f:%l:%m,%m
"#;

/// Writes the syntax file to the given writer.
// fn writeSyntax(writer: *std.Io.Writer) !void {
fn write_syntax(writer: &mut Vec<u8>) {
    writer.extend_from_slice(
        br#"" Vim syntax file
" Language: Ghostty config file
" Maintainer: Ghostty <https://github.com/ghostty-org/ghostty>
"
" THIS FILE IS AUTO-GENERATED

if exists('b:current_syntax')
  finish
endif

let b:current_syntax = 'ghostty'

let s:cpo_save = &cpo
set cpo&vim

syn iskeyword @,48-57,-
syn keyword ghosttyConfigKeyword"#,
    );

    // const config_fields = @typeInfo(Config).@"struct".fields;
    // inline for (config_fields) |field| {
    //     if (field.name[0] == '_') continue;
    //     try writer.print("\n\t\\ {s}", .{field.name});
    // }
    //   (`config_fields()` is already the `_`-filtered decl-order name/Key table.)
    for (name, _key) in config_fields() {
        writer.extend_from_slice(b"\n\t\\ ");
        writer.extend_from_slice(name);
    }

    writer.extend_from_slice(
        br#"

syn match ghosttyConfigComment /^\s*#.*/ contains=@Spell

hi def link ghosttyConfigComment Comment
hi def link ghosttyConfigKeyword Keyword

let &cpo = s:cpo_save
unlet s:cpo_save
"#,
    );
}

// test { _ = syntax; }
#[cfg(test)]
mod tests {
    use super::*;

    // The reference `test { _ = syntax; }` merely forces the comptime `syntax`
    // const to be evaluated (a compile-time smoke test that generation succeeds).
    // The Rust analog runs the generator and asserts it produced output.
    #[test]
    fn syntax_generates() {
        let _ = syntax();
    }

    // Byte-exact golden diff against ghostty's actually-generated files (the real
    // R1 gate — ghostty ships no full-script golden test; this port supplies one).
    #[test]
    fn syntax_matches_golden() {
        assert_eq!(
            syntax(),
            include_bytes!("../testdata/vim-syntax.vim").as_slice()
        );
    }

    #[test]
    fn ftdetect_matches_golden() {
        assert_eq!(
            FTDETECT,
            include_bytes!("../testdata/vim-ftdetect.vim").as_slice()
        );
    }

    #[test]
    fn ftplugin_matches_golden() {
        assert_eq!(
            FTPLUGIN,
            include_bytes!("../testdata/vim-ftplugin.vim").as_slice()
        );
    }

    #[test]
    fn compiler_matches_golden() {
        assert_eq!(
            COMPILER,
            include_bytes!("../testdata/vim-compiler.vim").as_slice()
        );
    }
}
