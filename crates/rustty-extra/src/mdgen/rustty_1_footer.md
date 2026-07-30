# FILES

_\$XDG_CONFIG_HOME/rustty/config.rustty_

: Location of the default configuration file.

_\$HOME/Library/Application Support/io.airely.rustty/config.rustty_

: **On macOS**, location of the default configuration file. This location takes
precedence over the XDG environment locations.

_\$LOCALAPPDATA/rustty/config.rustty_

: **On Windows**, if _\$XDG_CONFIG_HOME_ is not set, _\$LOCALAPPDATA_ will be searched
for configuration files.

# ENVIRONMENT

**TERM**

: Defaults to `xterm-rustty`. Can be configured with the `term` configuration option.

**RUSTTY_RESOURCES_DIR**

: Where the Rustty resources can be found.

**XDG_CONFIG_HOME**

: Default location for configuration files.

**$HOME/Library/Application Support/io.airely.rustty**

: **MACOS ONLY** default location for configuration files. This location takes
precedence over the XDG environment locations.

**LOCALAPPDATA**

: **WINDOWS ONLY:** alternate location to search for configuration files.

**RUSTTY_LOG**

: The `RUSTTY_LOG` environment variable can be used to control which
destinations receive logs. Rustty currently defines two destinations:

: - `stderr` - logging to `stderr`.
: - `macos` - logging to macOS's unified log (has no effect on non-macOS platforms).

: Combine values with a comma to enable multiple destinations. Prefix a
destination with `no-` to disable it. Enabling and disabling destinations
can be done at the same time. Setting `RUSTTY_LOG` to `true` will enable all
destinations. Setting `RUSTTY_LOG` to `false` will disable all destinations.

# BUGS

Report bugs to <gael@airely.io>.

# AUTHOR

Gael Hatchue <gael@airely.io>
Rustty contributors

# SEE ALSO

**rustty(5)**
