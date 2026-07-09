//! Bash completions. Port of ghostty's `src/extra/bash.zig`.
//!
//! Notes: bash completion support for --<key>=<value> depends on setting the
//! completion system to _not_ print a space following each successful completion
//! (see -o nospace). This results leading or tailing spaces being necessary to
//! move onto the next match.
//!
//! bash completion will read = as it's own completiong word regardless of whether
//! or not it's part of an on going completion like --<key>=. Working around this
//! requires looking backward in the command line args to pretend the = is an empty
//! string. see: <https://www.gnu.org/software/gnuastro/manual/html_node/Bash-TAB-completion-tutorial.html>

use std::any::TypeId;

use rustty_cli::ghostty::{Action, OptCompletionKind};
use rustty_config::config_struct::{
    config_completion_kind, config_fields, value_type, ConfigCompletionKind, Key,
};

/// A bash completions configuration that contains all the available commands
/// and options.
// pub const completions = comptimeGenerateBashCompletions();
//
// PORT NOTE: Rust cannot comptime-fold the byte string; this is a runtime pure fn.
// The reference's two-pass counter/`.fixed(&buf)` wrapper collapses to a `Vec`.
pub fn completions() -> Vec<u8> {
    let mut writer = Vec::new();
    write_bash_completions(&mut writer);
    writer
}

/// `switch (field.type) { bool, ?bool => …, else => … }` — the config-list flag
/// test (`bool`/`?bool` get the space-flag form, everything else `--key=`).
fn is_bool_or_opt_bool(key: Key) -> bool {
    let t = value_type(key);
    t == TypeId::of::<bool>() || t == TypeId::of::<Option<bool>>()
}

// fn writeBashCompletions(writer: *std.Io.Writer) !void {
fn write_bash_completions(writer: &mut Vec<u8>) {
    // const pad1 = "  "; pad2 = pad1 ++ pad1; … pad5.
    let pad1: &[u8] = b"  ";
    let pad2: &[u8] = b"    ";
    let pad3: &[u8] = b"      ";
    let pad4: &[u8] = b"        ";
    let pad5: &[u8] = b"          ";

    writer.extend_from_slice(
        br#"_ghostty() {

  # compat: mapfile -t COMPREPLY < <( "$@" )
  _compreply() {
    COMPREPLY=()
    while IFS='' read -r line; do COMPREPLY+=("$line"); done < <( "$@" )
  }

  # -o nospace requires we add back a space when a completion is finished
  # and not part of a --key= completion
  _add_spaces() {
    for idx in "${!COMPREPLY[@]}"; do
      [ -n "${COMPREPLY[idx]}" ] && COMPREPLY[idx]="${COMPREPLY[idx]} ";
    done
  }

  _fonts() {
    local IFS=$'\n'
    COMPREPLY=()
    while read -r line; do COMPREPLY+=("$line"); done < <( compgen -P '"' -S '"' -W "$($ghostty +list-fonts | grep '^[A-Z]' )" -- "$cur")
  }

  _themes() {
    local IFS=$'\n'
    COMPREPLY=()
    while read -r line; do COMPREPLY+=("$line"); done < <( compgen -P '"' -S '"' -W "$($ghostty +list-themes | sed -E 's/^(.*) \(.*$/\1/')" -- "$cur")
  }

  _files() {
    _compreply compgen -o filenames -f -- "$cur"
    for i in "${!COMPREPLY[@]}"; do
      if [[ -d "${COMPREPLY[i]}" ]]; then
        COMPREPLY[i]="${COMPREPLY[i]}/";
      fi
      if [[ -f "${COMPREPLY[i]}" ]]; then
        COMPREPLY[i]="${COMPREPLY[i]} ";
      fi
    done
  }

  _dirs() {
    _compreply compgen -o dirnames -d -- "$cur"
    for i in "${!COMPREPLY[@]}"; do
      if [[ -d "${COMPREPLY[i]}" ]]; then
        COMPREPLY[i]="${COMPREPLY[i]}/";
      fi
    done
    if [[ "${#COMPREPLY[@]}" == 0 && -d "$cur" ]]; then
      COMPREPLY=( "$cur " )
    fi
  }

  _handle_config() {
    local config="--help"
    config+=" --version"
"#,
    );

    // for (@typeInfo(Config).@"struct".fields) |field| {
    //     if (field.name[0] == '_') continue;
    //     switch (field.type) {
    //         bool, ?bool => …"config+=\" '--" ++ name ++ " '\"\n",
    //         else => …"config+=\" --" ++ name ++ "=\"\n",
    //     }
    // }
    for (name, key) in config_fields() {
        writer.extend_from_slice(pad2);
        if is_bool_or_opt_bool(*key) {
            writer.extend_from_slice(b"config+=\" '--");
            writer.extend_from_slice(name);
            writer.extend_from_slice(b" '\"\n");
        } else {
            writer.extend_from_slice(b"config+=\" --");
            writer.extend_from_slice(name);
            writer.extend_from_slice(b"=\"\n");
        }
    }

    writer.extend_from_slice(
        br#"
    case "$prev" in
"#,
    );

    // for (@typeInfo(Config).@"struct".fields) |field| { … the per-key arm … }
    let compgen_prefix: &[u8] = b"_compreply compgen -W \"";
    let compgen_suffix: &[u8] = b"\" -- \"$cur\"; _add_spaces ;;";
    for (name, key) in config_fields() {
        writer.extend_from_slice(pad3);
        writer.extend_from_slice(b"--");
        writer.extend_from_slice(name);
        writer.extend_from_slice(b") ");

        if name.starts_with(b"font-family") {
            writer.extend_from_slice(b"_fonts ;;");
        } else if *name == *b"theme" {
            writer.extend_from_slice(b"_themes ;;");
        } else if *name == *b"working-directory" {
            writer.extend_from_slice(b"_dirs ;;");
        } else {
            match config_completion_kind(*key) {
                // field.type == Config.RepeatablePath
                ConfigCompletionKind::RepeatablePath => writer.extend_from_slice(b"_files ;;"),
                ConfigCompletionKind::Bool => writer.extend_from_slice(b"return ;;"),
                ConfigCompletionKind::Enum(values) => {
                    writer.extend_from_slice(compgen_prefix);
                    for (i, v) in values.iter().enumerate() {
                        if i > 0 {
                            writer.extend_from_slice(b" ");
                        }
                        writer.extend_from_slice(v);
                    }
                    writer.extend_from_slice(compgen_suffix);
                }
                ConfigCompletionKind::PackedFlags(fields) => {
                    writer.extend_from_slice(compgen_prefix);
                    for (i, f) in fields.iter().enumerate() {
                        if i > 0 {
                            writer.extend_from_slice(b" ");
                        }
                        // f.name ++ " no-" ++ f.name
                        writer.extend_from_slice(f);
                        writer.extend_from_slice(b" no-");
                        writer.extend_from_slice(f);
                    }
                    writer.extend_from_slice(compgen_suffix);
                }
                ConfigCompletionKind::Other => writer.extend_from_slice(b"return ;;"),
            }
        }

        writer.extend_from_slice(b"\n");
    }

    writer.extend_from_slice(
        br#"      *) _compreply compgen -W "$config" -- "$cur" ;;
    esac

    return 0
  }

  _handle_actions() {
"#,
    );

    // for (@typeInfo(Action).@"enum".fields) |field| {
    //     const options = @field(Action, field.name).options();
    //     if (@typeInfo(options).@"struct".fields.len == 0) continue;
    //     var bashName = replace '-' -> '_';
    //     pad2 ++ "local " ++ bashName ++ "=\""
    //     for opts (skip _): count>0 ? " "; bool/?bool ? "'--name '" : "--name="
    //     " --help\"\n"
    // }
    for action in Action::ALL {
        let specs = action.option_specs();
        // assumes options will never be created with only <_name> members
        if specs.is_empty() {
            continue;
        }
        let bash_name = bash_name(action);
        writer.extend_from_slice(pad2);
        writer.extend_from_slice(b"local ");
        writer.extend_from_slice(&bash_name);
        writer.extend_from_slice(b"=\"");
        for (count, spec) in specs.iter().enumerate() {
            if count > 0 {
                writer.extend_from_slice(b" ");
            }
            match spec.kind {
                OptCompletionKind::Bool => {
                    writer.extend_from_slice(b"'--");
                    writer.extend_from_slice(spec.name);
                    writer.extend_from_slice(b" '");
                }
                _ => {
                    writer.extend_from_slice(b"--");
                    writer.extend_from_slice(spec.name);
                    writer.extend_from_slice(b"=");
                }
            }
        }
        writer.extend_from_slice(b" --help\"\n");
    }

    writer.extend_from_slice(
        br#"
    case "${COMP_WORDS[1]}" in
"#,
    );

    // for (@typeInfo(Action).@"enum".fields) |field| { … per-action case block … }
    for action in Action::ALL {
        let specs = action.option_specs();
        if specs.is_empty() {
            continue;
        }
        // bash doesn't allow variable names containing '-' so replace them
        let bash_name = bash_name(action);
        let tag = action.tag_name().as_bytes();

        writer.extend_from_slice(pad3);
        writer.extend_from_slice(b"+");
        writer.extend_from_slice(tag);
        writer.extend_from_slice(b")\n");
        writer.extend_from_slice(pad4);
        writer.extend_from_slice(b"case $prev in\n");
        for spec in specs {
            writer.extend_from_slice(pad5);
            writer.extend_from_slice(b"--");
            writer.extend_from_slice(spec.name);
            writer.extend_from_slice(b") ");

            match spec.kind {
                OptCompletionKind::Bool => writer.extend_from_slice(b"return ;;"),
                OptCompletionKind::Enum(values) | OptCompletionKind::OptionalEnum(values) => {
                    writer.extend_from_slice(compgen_prefix);
                    for (i, v) in values.iter().enumerate() {
                        if i > 0 {
                            writer.extend_from_slice(b" ");
                        }
                        writer.extend_from_slice(v);
                    }
                    writer.extend_from_slice(compgen_suffix);
                }
                // .optional whose child is not an enum
                OptCompletionKind::OptionalOther => {
                    if *spec.name == *b"config-file" {
                        writer.extend_from_slice(b"return ;;");
                    } else {
                        writer.extend_from_slice(b"return;;");
                    }
                }
                // else (non-optional, non-enum, non-bool)
                OptCompletionKind::Other => {
                    if *spec.name == *b"config-file" {
                        writer.extend_from_slice(b"_files ;;");
                    } else {
                        writer.extend_from_slice(b"return;;");
                    }
                }
            }
            writer.extend_from_slice(b"\n");
        }
        writer.extend_from_slice(pad5);
        writer.extend_from_slice(b"*) _compreply compgen -W \"$");
        writer.extend_from_slice(&bash_name);
        writer.extend_from_slice(b"\" -- \"$cur\" ;;\n");
        writer.extend_from_slice(
            br#"        esac
      ;;
"#,
        );
    }

    writer.extend_from_slice(
        br#"      *) _compreply compgen -W "--help" -- "$cur" ;;
    esac

    return 0
  }

  # begin main logic
  local topLevel="-e"
  topLevel+=" --help"
  topLevel+=" --version"
"#,
    );

    // for (@typeInfo(Action).@"enum".fields) |field| {
    //     pad1 ++ "topLevel+=\" +" ++ field.name ++ "\"\n"
    // }
    for action in Action::ALL {
        writer.extend_from_slice(pad1);
        writer.extend_from_slice(b"topLevel+=\" +");
        writer.extend_from_slice(action.tag_name().as_bytes());
        writer.extend_from_slice(b"\"\n");
    }

    writer.extend_from_slice(
        br#"
  local cur=""; local prev=""; local prevWasEq=false; COMPREPLY=()
  local ghostty="$1"

  # script assumes default COMP_WORDBREAKS of roughly $' \t\n"\'><=;|&(:'
  # if = is missing this script will degrade to matching on keys only.
  # eg: --key=
  # this can be improved if needed see: https://github.com/ghostty-org/ghostty/discussions/2994

  if [ "$2" = "=" ]; then cur=""
  else                    cur="$2"
  fi

  if [ "$3" = "=" ]; then prev="${COMP_WORDS[COMP_CWORD-2]}"; prevWasEq=true;
  else                    prev="${COMP_WORDS[COMP_CWORD-1]}"
  fi

  # current completion is double quoted add a space so the cursor progresses
  if [[ "$2" == \"*\" ]]; then
    COMPREPLY=( "$cur " );
    return;
  fi

  case "$COMP_CWORD" in
    1)
      case "${COMP_WORDS[1]}" in
        -e | --help | --version) return 0 ;;
        --*) _handle_config ;;
        *) _compreply compgen -W "${topLevel}" -- "$cur"; _add_spaces ;;
      esac
      ;;
    *)
      case "$prev" in
        -e | --help | --version) return 0 ;;
        *)
          if [[ "=" != "${COMP_WORDS[COMP_CWORD]}" && $prevWasEq != true ]]; then
            # must be completing with a space after the key eg: '--<key> '
            # clear out prev so we don't run any of the key specific completions
            prev=""
          fi

          case "${COMP_WORDS[1]}" in
            --*) _handle_config ;;
            +*) _handle_actions ;;
          esac
          ;;
      esac
      ;;
  esac

  return 0
}

complete -o nospace -o bashdefault -F _ghostty ghostty
"#,
    );
}

/// `std.mem.replaceScalar(u8, bashName, '-', '_')` — bash disallows `-` in variable
/// names, so the action tag's hyphens become underscores.
fn bash_name(action: Action) -> Vec<u8> {
    action
        .tag_name()
        .bytes()
        .map(|c| if c == b'-' { b'_' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Byte-exact golden diff against ghostty's actually-generated file (R1 gate).
    #[test]
    fn completions_match_golden() {
        assert_eq!(
            completions(),
            include_bytes!("../testdata/ghostty.bash").as_slice()
        );
    }
}
