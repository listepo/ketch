# DEV Community

Platform: https://dev.to/new (tags: #rust #cli #opensource #devtools)

## Article draft

Title:
Installing CLI tools straight from GitHub releases with ketch

Body:

Most command-line tools publish a prebuilt binary for every OS in their GitHub releases. Yet the usual path to your `PATH` is either a package-manager formula someone has to write, or a manual download, unpack and `chmod +x`. [ketch](https://github.com/listepo/ketch) automates that last step for any repo that ships releases.

### Install ketch

```bash
curl -fsSL https://raw.githubusercontent.com/listepo/ketch/main/install.sh | bash
```

### Install a tool

```bash
ketch install listepo/AirTalk
```

ketch looks at the release assets, picks the one for your OS and CPU, checks the published SHA-256 and links the binary onto your `PATH`.

### See why it chose that asset

```bash
ketch why AirTalk
ketch info --assets AirTalk
```

### Upgrade and roll back

Every version lives in its own directory, so an upgrade keeps the previous one:

```bash
ketch upgrade AirTalk
ketch rollback AirTalk   # relinks the old version, no download
```

### Pin a toolset per project

```bash
ketch lock   # writes the exact versions
ketch sync   # installs them on another machine or in CI
```

### Remove everything

`ketch self uninstall` lists what it will delete under `~/.ketch`, asks, then removes it, including the PATH lines it added.

ketch is a single Rust binary for macOS, Linux and Windows. v0.6.0 is out: https://github.com/listepo/ketch/releases/tag/v0.6.0

It's triple-licensed: GPLv3, a royalty-free license for proprietary desktop, mobile and web apps, or a commercial license. Issues and pull requests are welcome at https://github.com/listepo/ketch.

github.com/listepo/ketch — drafted technical DEV article with code blocks and examples
