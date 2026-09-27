# X (Twitter)

Platform: https://x.com/compose/post

## Thread draft

1/
Most CLI tools already ship binaries in their GitHub releases. Installing them is still manual.

ketch v0.6.0 fixes that: one command, any repo, macOS, Linux and Windows.

https://github.com/listepo/ketch

2/
`ketch install listepo/AirTalk`

It picks the right asset for your OS and CPU, checks the published SHA-256, and puts it on your PATH.

3/
Curious why it chose that file?
`ketch why AirTalk`

New version broke something?
`ketch rollback AirTalk`
No re-download. Every version is kept.

4/
Same tools on every machine: `ketch lock` then `ketch sync`.
Want it gone: `ketch self uninstall` removes everything it added.

5/
Single Rust binary. Triple-licensed: GPLv3, royalty-free for apps, or commercial.

Release notes: https://github.com/listepo/ketch/releases/tag/v0.6.0
Feedback and stars welcome.

github.com/listepo/ketch — drafted five-post X thread announcing ketch v0.6.0
