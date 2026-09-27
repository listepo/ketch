# Future

ketch is designed to become one installation manager for every platform: a
single tool that installs command-line utilities and apps on macOS, Linux and
Windows, without platform-specific formulae or a different package manager on
each machine. The same `ketch install owner/repo`, the same lockfile and the
same verification should work everywhere a project publishes a release.

## Native desktop apps

Plans include native desktop applications, not only CLI tools. Today ketch
installs macOS `.app` bundles into `/Applications`; the goal is the same for
Windows and Linux desktop apps, each installed the way that platform expects
and removed cleanly on uninstall.

## Paying for it

ketch stays free for individuals and everyday use. Companies and businesses
will have a paid tier: support, services and features aimed at teams. The
money goes into supporting and maintaining the project. The code remains under
the triple license (GPLv3, royalty-free or commercial); the paid tier sits
alongside it, not in place of it.

## Governance

One tentative idea, not a commitment: the project may one day move into a
foundation.

## What stays the same

The limits in [ROADMAP.md](../ROADMAP.md) still hold: ketch installs what a
project already publishes rather than building from source, does not resolve
dependency graphs, and does not run as root. Any new place an app has to land
outside `~/.ketch` would be a user-level location, documented and recorded in
state so that uninstall can take it back, as `/Applications` is today.
