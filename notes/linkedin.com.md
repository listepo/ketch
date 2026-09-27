# LinkedIn

Platform: https://www.linkedin.com/feed/ (start a post)

## Post draft

I've just released ketch v0.6.0, an open-source tool for installing command-line software directly from GitHub releases on macOS, Linux and Windows.

Engineering teams spend real time on the last mile of tooling: finding the right binary for each machine, verifying it, and keeping versions consistent across laptops and CI. ketch handles that in one command:

    ketch install listepo/AirTalk

What it provides:
- Checksum verification by default, with signature checks when a project publishes them
- A versioned store with instant rollback (`ketch rollback AirTalk`)
- Reproducible toolsets per project with `ketch lock` and `ketch sync`
- Clean, complete uninstall

ketch is available under a triple license: GPLv3, a royalty-free license for proprietary desktop, mobile and web apps, or a commercial license. Businesses that need commercial terms or support are welcome to get in touch.

Release: https://github.com/listepo/ketch/releases/tag/v0.6.0
Repository: https://github.com/listepo/ketch

#opensource #developertools #rust #devops

github.com/listepo/ketch — drafted professional LinkedIn announcement for ketch v0.6.0 release
