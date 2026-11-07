#!/bin/sh
# Installs the latest `flowmark` binary from GitHub Releases.
#   curl -fsSL https://raw.githubusercontent.com/daniel100097/flowmark/main/scripts/install.sh | sh
# Installs to $FLOWMARK_INSTALL_DIR (default ~/.local/bin). Falls back to building with cargo when there is
# no prebuilt binary for this platform.
set -eu
repo=daniel100097/flowmark
dir=${FLOWMARK_INSTALL_DIR:-$HOME/.local/bin}

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Darwin-arm64) target=aarch64-apple-darwin ;;
  *) target= ;;
esac

if [ -n "$target" ]; then
  url="https://github.com/$repo/releases/latest/download/flowmark-$target.tar.gz"
  mkdir -p "$dir"
  if curl -fsSL "$url" | tar xz -C "$dir" flowmark 2>/dev/null; then
    echo "installed $dir/flowmark ($target)"
    case ":$PATH:" in *":$dir:"*) ;; *) echo "add $dir to your PATH" ;; esac
    exit 0
  fi
  echo "no release download for $target; building with cargo" >&2
fi

command -v cargo > /dev/null || { echo "install Rust (https://rustup.rs) or download flowmark from https://github.com/$repo/releases" >&2; exit 1; }
cargo install --locked --git "https://github.com/$repo" flowmark_cli
