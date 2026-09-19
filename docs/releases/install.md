## Install

**macOS** — unzip and drag *ZEN Manager* to Applications. The build is not
signed with an Apple developer certificate, so the first time, right-click the
app and choose **Open**. libmtp is bundled; there is nothing else to install.

**Linux** — extract the archive and run `./install.sh`. It installs into
`~/.local` and needs no root. You also need libmtp from your distribution:

```sh
sudo apt install libmtp9 libmtp-runtime   # Debian / Ubuntu
sudo dnf install libmtp                   # Fedora
sudo pacman -S libmtp                     # Arch
```

The runtime package also installs the rules that let a normal user access the
player.
