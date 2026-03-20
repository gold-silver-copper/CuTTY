CUTTY(1)

# NAME

CuTTY, the Copper Teletype.

# SYNOPSIS

CuTTY is a modern terminal emulator that comes with sensible defaults, but
allows for extensive configuration. By integrating with other applications,
rather than reimplementing their functionality, it manages to provide a flexible
set of features with high performance.

# FLAGS

*-h, --help*

	Prints help information.

*--hold*

	Remain open after child process exits.

*--daemon*

	Do not spawn an initial window.

*--print-events*

	Print all events to STDOUT.

*-q*

	Reduces the level of verbosity (the min level is *-qq*).

*--ref-test*

	Generates ref test

*-v*

	Increases the level of verbosity (the max level is *-vvv*).

*-V, --version*

	Prints version information.

# OPTIONS

*--class* _<GENERAL>_ | _<GENERAL>_,_<INSTANCE>_

	Defines the window class hint on Linux.

	When only the general class is passed, instance will be set to the same value.

	On Wayland the general class sets the _app\_id_, while the instance class is ignored.

	Default: _CuTTY,CuTTY_

*-e, --command* _<COMMAND>..._

	Command and args to execute (must be last argument).

*--config-file* _<CONFIG_FILE>_

	Specify alternative configuration file.

	CuTTY doesn't create the config file for you, but it looks for one in the
	following locations on UNIX systems:

		. _$XDG_CONFIG_HOME/cutty/cutty.toml_
		. _$XDG_CONFIG_HOME/cutty.toml_
		. _$HOME/.config/cutty/cutty.toml_
		. _$HOME/.cutty.toml_
		. _/etc/cutty/cutty.toml_

	On Windows, the config file will be looked for in:

		. _%APPDATA%\\cutty\\cutty.toml_

*--embed* _<PARENT>_

	X11 window ID to embed CuTTY within (decimal or hexadecimal with _0x_ prefix).

*-o, --option* _<OPTION>..._

	Override configuration file options.

	Example: _cutty -o 'cursor.style="Beam"'_

*--socket* _<SOCKET>_

	Path for IPC socket creation.

*-T, --title* _<TITLE>_

	Defines the window title.

	Default: _CuTTY_

*--working-directory* _<WORKING_DIRECTORY>_

	Start the shell in the specified working directory.

# SUBCOMMANDS

*msg*

	Send IPC socket messages (see *cutty-msg*(1)).

# SEE ALSO

*cutty-msg*(1), *cutty*(5), *cutty-bindings*(5), *cutty-escapes*(7)

# BUGS


# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>
