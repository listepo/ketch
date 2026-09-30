# Medium

Platform: https://medium.com/new-story (suggested publications: Better Programming, ITNEXT)

## Story draft

Title:
The last mile of installing software is still manual. ketch fixes it.

Subtitle:
One command to install, verify and roll back tools from GitHub releases on any OS.

Body:

If you work across a Mac, a Linux server and the occasional Windows box, you know the routine. You find a great command-line tool, open its GitHub releases, pick the right archive for this machine, unpack it, make it executable and move it into your PATH. Then you do it again on the next machine, and again when a new version comes out.

ketch turns that routine into one command:

    ketch install listepo/AirTalk

It picks the right asset for your platform, checks it against the published checksum, and installs it into a versioned store. If the new version misbehaves, `ketch rollback AirTalk` brings back the previous one without downloading anything. If you want the same tools on every machine, `ketch lock` and `ketch sync` take care of it. And if you ever want ketch gone, `ketch self uninstall` removes every file it created.

Getting started takes one line:

    curl -fsSL https://raw.githubusercontent.com/pyrlyn/ketch/main/install.sh | bash

Version 0.6.0 is out now: https://github.com/pyrlyn/ketch/releases/tag/v0.6.0. The code is at https://github.com/pyrlyn/ketch, under a triple license: GPLv3, a royalty-free license for proprietary desktop, mobile and web apps, or a commercial license.

github.com/pyrlyn/ketch — drafted Medium story explaining ketch to a broader audience
