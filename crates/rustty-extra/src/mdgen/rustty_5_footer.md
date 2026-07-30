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

**XDG_CONFIG_HOME**

: Default location for configuration files.

**$HOME/Library/Application Support/io.airely.rustty**

: **MACOS ONLY** default location for configuration files. This location takes
precedence over the XDG environment locations.

**LOCALAPPDATA**

: **WINDOWS ONLY:** alternate location to search for configuration files.

# BUGS

Report bugs to <gael@airely.io>.

# AUTHOR

Gael Hatchue <gael@airely.io>
Rustty contributors

# SEE ALSO

**rustty(1)**
