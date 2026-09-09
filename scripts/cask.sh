#!/bin/bash
#
# Print the Homebrew cask for one ketch release.
#
#   scripts/cask.sh <version> <sha256 of the aarch64 tarball> <sha256 of the x86_64 tarball>
#
# The release workflow writes the output to Casks/ketch.rb in listepo/homebrew-tap
# on every release, so the file there is generated, never edited by hand: change
# this script instead.
#
# It is a cask rather than a formula on purpose. ketch lives in ~/.ketch like
# everything it installs, and a formula's post_install runs in Homebrew's
# sandbox with no access to $HOME at all. Cask install steps are sandboxed too,
# but per step they can be granted network access and a writable path under
# the home directory, which is exactly what `ketch self install` needs and
# nothing more. Homebrew keeps only the bootstrap binary it downloaded; the
# installed ketch is one ketch fetched and verified itself.

set -euo pipefail

if [ $# -ne 3 ]; then
  echo "usage: $0 <version> <sha256-aarch64> <sha256-x86_64>" >&2
  exit 2
fi

VERSION="${1#v}"
SHA_ARM="$2"
SHA_INTEL="$3"

for sha in "$SHA_ARM" "$SHA_INTEL"; do
  if ! printf '%s' "$sha" | grep -Eq '^[0-9a-f]{64}$'; then
    echo "not a sha256: $sha" >&2
    exit 2
  fi
done

cat <<EOF
cask "ketch" do
  arch arm: "aarch64", intel: "x86_64"

  version "${VERSION}"
  sha256 arm:   "${SHA_ARM}",
         intel: "${SHA_INTEL}"

  url "https://github.com/listepo/ketch/releases/download/v#{version}/ketch-#{arch}-apple-darwin.tar.gz"
  name "ketch"
  desc "Catch releases straight from GitHub"
  homepage "https://github.com/listepo/ketch"

  livecheck do
    url :homepage
    strategy :github_latest
  end

  # ketch installs itself into ~/.ketch as one of its own packages; Homebrew
  # only delivers the binary that does so. Nothing is linked into the prefix.
  stage_only true

  postflight_steps do
    # The staged binary runs exactly once, to install a copy that ketch
    # downloads and verifies itself. Gatekeeper would refuse the quarantined
    # bootstrap otherwise, as it refuses any command-line binary that is not
    # notarised; install.sh lifts the same attribute.
    run "/usr/bin/xattr", args: ["-d", "com.apple.quarantine", "{{staged_path}}/ketch"], must_succeed: false
    # The steps run with a throwaway HOME and the DSL has no token for the
    # real one, so the shell asks the user database instead: \`~user\` expands
    # from there, not from HOME. ~/.ketch is the one path under the home
    # directory a step may write, and the only one ketch touches.
    if_path_exists ".ketch/store/ketch", base: :home do
      run "/bin/sh", args:           ["-c", 'eval "r=~\$1/.ketch" && KETCH_ROOT="\$r" exec "\$2" self update',
                                      "ketch", "{{user}}", "{{staged_path}}/ketch"],
                     network_access: true,
                     writable_paths: [".ketch"],
                     writable_base:  :home
    end
    unless_path_exists ".ketch/store/ketch", base: :home do
      run "/bin/sh", args:           ["-c", 'eval "r=~\$1/.ketch" && KETCH_ROOT="\$r" exec "\$2" self install',
                                      "ketch", "{{user}}", "{{staged_path}}/ketch"],
                     network_access: true,
                     writable_paths: [".ketch"],
                     writable_base:  :home
    end
  end

  uninstall_postflight_steps do
    # Two flags, both because of where this runs. \`--keep-packages\` removes the
    # ketch package and nothing else: what ketch installed is not Homebrew's to
    # take, and stays until \`zap\`. \`--no-brew\` stops ketch calling
    # \`brew uninstall --cask ketch\` from inside that very command.
    run "/bin/sh", args:           ["-c",
                                    'eval "r=~\$1/.ketch" && KETCH_ROOT="\$r" exec "\$r/bin/ketch" self uninstall -y --keep-packages --no-brew',
                                    "ketch", "{{user}}"],
                   must_succeed:   false,
                   writable_paths: [".ketch"],
                   writable_base:  :home
  end

  zap trash: "~/.ketch"

  caveats do
    path_environment_variable "#{Dir.home}/.ketch/bin"
  end
end
EOF
