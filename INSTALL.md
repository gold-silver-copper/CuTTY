# Cargo Installation

If you're just interested in the CuTTY binary and you don't need the
[terminfo file](#terminfo), [desktop entry](#desktop-entry),
[manual page](#manual-page) or [shell completions](#shell-completions), you can
install it directly through cargo:

```sh
cargo install cutty
```

Note that you will still need to install the dependencies for your OS of choice.
Please refer to the [Dependencies](#dependencies) section.

# Manual Installation

1. [Prerequisites](#prerequisites)
    1. [Source Code](#clone-the-source-code)
    2. [Rust Compiler](#install-the-rust-compiler-with-rustup)
    3. [Dependencies](#dependencies)
        1. [Debian/Ubuntu](#debianubuntu)
        2. [Arch Linux](#arch-linux)
        3. [Fedora](#fedora)
        4. [CentOS/RHEL 7](#centosrhel-7)
        5. [openSUSE](#opensuse)
        6. [Slackware](#slackware)
        7. [Void Linux](#void-linux)
        8. [FreeBSD](#freebsd)
        9. [OpenBSD](#openbsd)
        10. [Solus](#solus)
        11. [NixOS/Nixpkgs](#nixosnixpkgs)
        12. [Gentoo](#gentoo)
        13. [Clear Linux](#clear-linux)
        14. [GNU Guix](#gnu-guix)
        15. [Alpine Linux](#alpine-linux)
        16. [Windows](#windows)
        17. [Other](#other)
2. [Building](#building)
    1. [Linux/Windows/BSD](#linux--windows--bsd)
    2. [macOS](#macos)
3. [Post Build](#post-build)
    1. [Terminfo](#terminfo)
    2. [Desktop Entry](#desktop-entry)
    3. [Manual Page](#manual-page)
    4. [Shell completions](#shell-completions)
        1. [Zsh](#zsh)
        2. [Bash](#bash)
        3. [Fish](#fish)

## Prerequisites

### Clone the source code

Before compiling CuTTY, you'll have to first clone the source code:

```sh
git clone <your-cutty-repository-url>
cd cutty
```

### Install the Rust compiler with `rustup`

1. Install [`rustup.rs`](https://rustup.rs/).

3. To make sure you have the right Rust compiler installed, run

   ```sh
   rustup override set stable
   rustup update stable
   ```

### Dependencies

These are the minimum dependencies required to build CuTTY, please note
that with some setups additional dependencies might be desired.

#### Debian/Ubuntu

If you'd like to build a local version manually, you need a few extra libraries
to build CuTTY. Here's an apt command that should install all of them. If
something is still found to be missing, please open an issue.

```sh
apt install cmake g++ pkg-config libfontconfig1-dev libxcb-xfixes0-dev libxkbcommon-dev python3
```

#### Arch Linux

On Arch Linux, you need a few extra libraries to build CuTTY. Here's a
`pacman` command that should install all of them. If something is still found
to be missing, please open an issue.

```sh
pacman -S cmake freetype2 fontconfig pkg-config make libxcb libxkbcommon python
```

#### Fedora

On Fedora, you need a few extra libraries to build CuTTY. Here's a `dnf`
command that should install all of them. If something is still found to be
missing, please open an issue.

```sh
dnf install cmake freetype-devel fontconfig-devel libxcb-devel libxkbcommon-devel g++
```

#### CentOS/RHEL 7

On CentOS/RHEL 7, you need a few extra libraries to build CuTTY. Here's a `yum`
command that should install all of them. If something is still found to be
missing, please open an issue.

```sh
yum install cmake freetype-devel fontconfig-devel libxcb-devel libxkbcommon-devel xcb-util-devel
yum group install "Development Tools"
```

#### RHEL 8

On RHEL 8, like RHEL 7, you need a few extra libraries to build CuTTY. Here's a `dnf`
command that should install all of them. If something is still found to be
missing, please open an issue.

```sh
dnf install cmake freetype-devel fontconfig-devel libxcb-devel libxkbcommon-devel
dnf group install "Development Tools"
```

#### openSUSE

On openSUSE, you need a few extra libraries to build CuTTY. Here's
a `zypper` command that should install all of them. If something is
still found to be missing, please open an issue.

```sh
zypper install cmake freetype-devel fontconfig-devel libxcb-devel libxkbcommon-devel gcc-c++
```

#### Slackware

Compiles out of the box for 14.2

#### Void Linux

On [Void Linux](https://voidlinux.org), install following packages before
compiling CuTTY:

```sh
xbps-install cmake freetype-devel expat-devel fontconfig-devel libxcb-devel pkg-config python3
```

#### FreeBSD

On FreeBSD, you need a few extra libraries to build CuTTY. Here's a `pkg`
command that should install all of them. If something is still found to be
missing, please open an issue.

```sh
pkg install cmake freetype2 fontconfig pkgconf python3
```

#### OpenBSD

On OpenBSD 6.5, you need [Xenocara](https://xenocara.org) and Rust to build
CuTTY, plus Python 3 to build its XCB dependency. If something is still
found to be missing, please open an issue.

```sh
pkg_add rust python
```

Select the package for Python 3 (e.g. `python-3.6.8p0`) when prompted.

The default user limits in OpenBSD are insufficient to build CuTTY. A
`datasize-cur` of at least 3GB is recommended (see [login.conf](https://man.openbsd.org/login.conf)).

#### Solus

On [Solus](https://solus-project.com/), you need a few extra libraries to build
CuTTY. Here's a `eopkg` command that should install all of them. If
something is still found to be missing, please open an issue.

```sh
eopkg install fontconfig-devel
```

#### NixOS/Nixpkgs

The following command can be used to get a shell with all development
dependencies on [NixOS](https://nixos.org).

```sh
nix-shell -A cutty '<nixpkgs>'
```

#### Gentoo

On Gentoo, you need a few extra libraries to build CuTTY. The following
command should install all of them. If something is still found to be missing,
please open an issue.

```sh
emerge --onlydeps x11-terms/cutty
```

#### Clear Linux

On Clear Linux, you need a few extra libraries to build CuTTY. Here's a
`swupd` command that should install all of them. If something is still found
to be missing, please open an issue.

```sh
swupd bundle-add devpkg-expat devpkg-freetype devpkg-libxcb devpkg-fontconfig
```

#### GNU Guix

The following command can be used to get a shell with all development
dependencies on [GNU Guix](https://guix.gnu.org/).

```sh
guix environment cutty
```

#### Alpine Linux

On Alpine Linux, you need a few extra libraries to build CuTTY. Here's an
`apk` command that should install all of them. If something is still found to
be missing, please open an issue.

```sh
sudo apk add cmake pkgconf freetype-dev fontconfig-dev python3 libxcb-dev
```

#### Windows

On windows you will need to have the `{architecture}-pc-windows-msvc` toolchain
installed as well as [Clang 3.9 or greater](http://releases.llvm.org/download.html).

#### Other

If you build CuTTY on another distribution, we would love some help
filling in this section of the README.

## Building

### Linux / Windows / BSD

```sh
cargo build --release
```

On Linux/BSD, if it is desired to build CuTTY without support for either the
X11 or Wayland rendering backend the following commands can be used.

```sh
# Force support for only Wayland
cargo build --release --no-default-features --features=wayland

# Force support for only X11
cargo build --release --no-default-features --features=x11
```

If all goes well, this should place a binary at `target/release/cutty`.

### macOS

```sh
make app
cp -r target/release/osx/CuTTY.app /Applications/
```

#### Universal Binary

The following will build an executable that runs on both x86 and ARM macos
architectures:

```sh
rustup target add x86_64-apple-darwin aarch64-apple-darwin
make app-universal
```

## Post Build

There are some extra things you might want to set up after installing CuTTY.
All the post build instruction assume you're still inside the CuTTY
repository.

### Terminfo

To make sure CuTTY works correctly, either the `cutty` or
`cutty-direct` terminfo must be used. The `cutty` terminfo will be
picked up automatically if it is installed.

If the following command returns without any errors, the `cutty` terminfo is
already installed:

```sh
infocmp cutty
```

If it is not present already, you can install it globally with the following
command:

```
sudo tic -xe cutty,cutty-direct extra/cutty.info
```

### Desktop Entry

Many Linux and BSD distributions support desktop entries for adding applications
to system menus. This will install the desktop entry for CuTTY:

```sh
sudo cp target/release/cutty /usr/local/bin # or anywhere else in $PATH
sudo cp extra/logo/cutty-term.png /usr/share/pixmaps/CuTTY.png
sudo desktop-file-install extra/linux/CuTTY.desktop
sudo update-desktop-database
```

The source of truth for CuTTY's icon artwork is `CuTTY.png` in the repository
root. All derived icon assets can be regenerated from it with:

```sh
./scripts/regenerate-icons.sh
```

This refreshes the packaged Linux PNG at `extra/logo/cutty-term.png`, the
runtime/window PNG at `extra/logo/compat/cutty-term.png`, the Windows icon at
`cutty/windows/cutty.ico`, and the macOS bundle icon at
`extra/osx/CuTTY.app/Contents/Resources/cutty.icns`. The script requires
`python3` with Pillow installed.

### Manual Page

Installing the manual page requires the additional dependencies `gzip` and `scdoc`.

```sh
sudo mkdir -p /usr/local/share/man/man1
sudo mkdir -p /usr/local/share/man/man5
scdoc < extra/man/cutty.1.scd | gzip -c | sudo tee /usr/local/share/man/man1/cutty.1.gz > /dev/null
scdoc < extra/man/cutty-msg.1.scd | gzip -c | sudo tee /usr/local/share/man/man1/cutty-msg.1.gz > /dev/null
scdoc < extra/man/cutty.5.scd | gzip -c | sudo tee /usr/local/share/man/man5/cutty.5.gz > /dev/null
scdoc < extra/man/cutty-bindings.5.scd | gzip -c | sudo tee /usr/local/share/man/man5/cutty-bindings.5.gz > /dev/null
```

### Shell completions

To get automatic completions for CuTTY's flags and arguments you can install the provided shell completions.

#### Zsh

To install the completions for zsh, you can place the `extra/completions/_cutty` file in any
directory referenced by `$fpath`.

If you do not already have such a directory registered through your `~/.zshrc`, you can add one like this:

```sh
mkdir -p ${ZDOTDIR:-~}/.zsh_functions
echo 'fpath+=${ZDOTDIR:-~}/.zsh_functions' >> ${ZDOTDIR:-~}/.zshrc
```

Then copy the completion file to this directory:

```sh
cp extra/completions/_cutty ${ZDOTDIR:-~}/.zsh_functions/_cutty
```

#### Bash

To install the completions for bash, you can `source` the `extra/completions/cutty.bash` file
in your `~/.bashrc` file.

If you do not plan to delete the source folder of cutty, you can run

```sh
echo "source $(pwd)/extra/completions/cutty.bash" >> ~/.bashrc
```

Otherwise you can copy it to the `~/.bash_completion` folder and source it from there:

```sh
mkdir -p ~/.bash_completion
cp extra/completions/cutty.bash ~/.bash_completion/cutty
echo "source ~/.bash_completion/cutty" >> ~/.bashrc
```

#### Fish

To install the completions for fish, from inside the fish shell, run

```
mkdir -p $fish_complete_path[1]
cp extra/completions/cutty.fish $fish_complete_path[1]/cutty.fish
```
