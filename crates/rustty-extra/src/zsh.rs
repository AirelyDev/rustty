//! Zsh completions. Port of ghostty's `src/extra/zsh.zig`.

use rustty_cli::ghostty::{Action, OptCompletionKind};
use rustty_config::config_struct::{config_completion_kind, config_fields, ConfigCompletionKind};

/// A zsh completions configuration that contains all the available commands
/// and options.
// pub const completions = comptimeGenerateZshCompletions();
pub fn completions() -> Vec<u8> {
    let mut writer = Vec::new();
    write_zsh_completions(&mut writer);
    writer
}

// const equals_required = "=-:::";
const EQUALS_REQUIRED: &[u8] = b"=-:::";

// fn writeZshCompletions(writer: *std.Io.Writer) !void {
fn write_zsh_completions(writer: &mut Vec<u8>) {
    writer.extend_from_slice(
        br#"#compdef ghostty

_fonts () {
  local font_list=$(ghostty +list-fonts | grep -Z '^[A-Z]')
  local fonts=(${(f)font_list})
  _describe -t fonts 'fonts' fonts
}

_themes() {
  local theme_list=$(ghostty +list-themes | sed -E 's/^(.*) \(.*$/\1/')
  local themes=(${(f)theme_list})
  _describe -t themes 'themes' themes
}
"#,
    );

    writer.extend_from_slice(b"_config() {\n");
    writer.extend_from_slice(b"  _arguments \\\n");
    writer.extend_from_slice(b"    \"--help\" \\\n");
    writer.extend_from_slice(b"    \"--version\" \\\n");
    for (name, key) in config_fields() {
        writer.extend_from_slice(b"    \"--");
        writer.extend_from_slice(name);

        if name.starts_with(b"font-family") {
            writer.extend_from_slice(EQUALS_REQUIRED);
            writer.extend_from_slice(b"_fonts");
        } else if *name == *b"theme" {
            writer.extend_from_slice(EQUALS_REQUIRED);
            writer.extend_from_slice(b"_themes");
        } else if *name == *b"working-directory" {
            writer.extend_from_slice(EQUALS_REQUIRED);
            writer.extend_from_slice(b"{_files -/}");
        } else {
            match config_completion_kind(*key) {
                // field.type == Config.RepeatablePath
                ConfigCompletionKind::RepeatablePath => {
                    writer.extend_from_slice(EQUALS_REQUIRED);
                    writer.extend_from_slice(b"_files"); // todo check if this is needed
                }
                ConfigCompletionKind::Bool => {}
                ConfigCompletionKind::Enum(values) => {
                    writer.extend_from_slice(EQUALS_REQUIRED);
                    writer.extend_from_slice(b"(");
                    for (i, v) in values.iter().enumerate() {
                        if i > 0 {
                            writer.extend_from_slice(b" ");
                        }
                        writer.extend_from_slice(v);
                    }
                    writer.extend_from_slice(b")");
                }
                ConfigCompletionKind::PackedFlags(fields) => {
                    // The reference writes `equals_required` before the packed
                    // check; a packed struct emits `(name no-name …)`.
                    writer.extend_from_slice(EQUALS_REQUIRED);
                    writer.extend_from_slice(b"(");
                    for (i, f) in fields.iter().enumerate() {
                        if i > 0 {
                            writer.extend_from_slice(b" ");
                        }
                        writer.extend_from_slice(f);
                        writer.extend_from_slice(b" no-");
                        writer.extend_from_slice(f);
                    }
                    writer.extend_from_slice(b")");
                }
                // The reference's non-packed-struct + general-else arms both emit
                // `equals_required ++ "( )"`:
                //resize-overlay-duration
                //keybind
                //window-padding-x ...-y
                //link
                //palette
                //background
                //foreground
                //font-variation*
                //font-feature
                ConfigCompletionKind::Other => {
                    writer.extend_from_slice(EQUALS_REQUIRED);
                    writer.extend_from_slice(b"( )");
                }
            }
        }

        writer.extend_from_slice(b"\" \\\n");
    }
    writer.extend_from_slice(b"\n}\n\n");

    writer.extend_from_slice(
        br#"_ghostty() {
  typeset -A opt_args
  local context state line
  local opt=('-e' '--help' '--version')

  _arguments -C \
    '1:actions:->actions' \
    '*:: :->rest' \

  if [[ "$line[1]" == "--help" || "$line[1]" == "--version" || "$line[1]" == "-e" ]]; then
    return
  fi

  if [[ "$line[1]" == -* ]]; then
    _config
    return
  fi

  case "$state" in
    (actions)
      local actions; actions=(
"#,
    );

    {
        // how to get 'commands'
        let padding: &[u8] = b"        ";
        for action in Action::ALL {
            writer.extend_from_slice(padding);
            writer.extend_from_slice(b"'+");
            writer.extend_from_slice(action.tag_name().as_bytes());
            writer.extend_from_slice(b"'\n");
        }
    }

    writer.extend_from_slice(
        br#"      )
      _describe '' opt
      _describe -t action 'action' actions
    ;;
    (rest)
      if [[ "$line[2]" == "--help" ]]; then
        return
      fi

      local help=('--help')
      _describe '' help

      case $line[1] in
"#,
    );
    {
        let padding: &[u8] = b"        ";
        for action in Action::ALL {
            let specs = action.option_specs();
            // assumes options will never be created with only <_name> members
            if specs.is_empty() {
                continue;
            }

            writer.extend_from_slice(padding);
            writer.extend_from_slice(b"(+");
            writer.extend_from_slice(action.tag_name().as_bytes());
            writer.extend_from_slice(b")\n");
            writer.extend_from_slice(padding);
            writer.extend_from_slice(b"  _arguments \\\n");
            for spec in specs {
                writer.extend_from_slice(padding);
                writer.extend_from_slice(b"    '--");
                writer.extend_from_slice(spec.name);

                match spec.kind {
                    OptCompletionKind::Bool => {}
                    OptCompletionKind::Enum(values) => {
                        writer.extend_from_slice(EQUALS_REQUIRED);
                        writer.extend_from_slice(b"(");
                        for (i, v) in values.iter().enumerate() {
                            if i > 0 {
                                writer.extend_from_slice(b" ");
                            }
                            writer.extend_from_slice(v);
                        }
                        writer.extend_from_slice(b")");
                    }
                    // .optional whose child is an enum
                    OptCompletionKind::OptionalEnum(values) => {
                        writer.extend_from_slice(EQUALS_REQUIRED);
                        writer.extend_from_slice(b"(");
                        for (i, v) in values.iter().enumerate() {
                            if i > 0 {
                                writer.extend_from_slice(b" ");
                            }
                            writer.extend_from_slice(v);
                        }
                        writer.extend_from_slice(b")");
                    }
                    // .optional whose child is not an enum
                    OptCompletionKind::OptionalOther => {
                        writer.extend_from_slice(EQUALS_REQUIRED);
                        if *spec.name == *b"config-file" {
                            writer.extend_from_slice(b"_files");
                        } else {
                            writer.extend_from_slice(b"( )");
                        }
                    }
                    // else
                    OptCompletionKind::Other => {
                        writer.extend_from_slice(EQUALS_REQUIRED);
                        if *spec.name == *b"config-file" {
                            writer.extend_from_slice(b"_files");
                        } else {
                            writer.extend_from_slice(b"( )");
                        }
                    }
                }

                writer.extend_from_slice(b"' \\\n");
            }
            writer.extend_from_slice(padding);
            writer.extend_from_slice(b";;\n");
        }
    }
    writer.extend_from_slice(
        br#"      esac
    ;;
  esac
}

_ghostty "$@"
"#,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // Byte-exact golden diff against ghostty's actually-generated file (R1 gate).
    #[test]
    fn completions_match_golden() {
        assert_eq!(
            completions(),
            include_bytes!("../testdata/_ghostty").as_slice()
        );
    }
}
