CUTTY-MSG(1)

# NAME

cutty-msg - Send messages to CuTTY.

# SYNOPSIS

This command communicates with running CuTTY instances through a socket,
making it possible to control CuTTY without directly accessing it.

# OPTIONS

*-s, --socket* _<SOCKET>_

	Path for IPC socket communication.

# MESSAGES

*create-window*

	Create a new window in the same CuTTY process.

	*FLAGS*
		*--hold*

			Remain open after child process exits.

	*OPTIONS*
		*--working-directory* _<WORKING_DIRECTORY>_

			Start the shell in the specified working directory.

		*-T, --title* _<TITLE>_

			Defines the window title.

			Default: _CuTTY_

		*--class* _<GENERAL>_ | _<GENERAL>_,_<INSTANCE>_

			Defines window class/app_id on X11/Wayland.

			Default: _CuTTY,CuTTY_

		*-o, --option* _<OPTION>..._

			Override configuration file options.

			Example: _cutty msg create-window -o 'cursor.style="Beam"'_

		*-e, --command* _<COMMAND>..._

			Command and args to execute (must be last argument).

*config*

	Update the CuTTY configuration.

	*ARGS*
		*<CONFIG_OPTIONS>...*

			Configuration file options.

			Example: _cutty msg config 'cursor.style="Beam"'_

	*FLAGS*
		*-r, --reset*

			Clear all runtime configuration changes.

	*OPTIONS*
		*-w, --window-id* _<WINDOW_ID>_

			Window ID for the new config.

			Use _-1_ to apply this change to all windows.

			Default: _$CUTTY_WINDOW_ID_

*get-config*

	Read runtime CuTTY configuration.

	*OPTIONS*
		*-w, --window-id* _<WINDOW_ID>_

			Window ID for the config request.

			Use _-1_ to get the global config.

			Default: _$CUTTY_WINDOW_ID_

# SEE ALSO

*cutty*(1), *cutty*(5), *cutty-bindings*(5), *cutty-escapes*(7)

# BUGS


# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>
