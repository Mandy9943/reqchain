#!/usr/bin/env bash
# One-time prerequisites for building reqchain and its desktop app on
# Debian/Ubuntu, per README.md and docs/superpowers/specs/2026-09-16-reqchain-design.md §12.
set -euo pipefail

if ! command -v rustup >/dev/null 2>&1; then
  echo "Installing rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
else
  echo "rustup already installed, skipping."
fi

echo "Installing system packages (apt, sudo)..."
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  libwebkit2gtk-4.1-dev \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev

echo "Done. From the repo root:"
echo "  cargo build --release            # CLI"
echo "  pnpm --dir apps/desktop install"
echo "  pnpm --dir apps/desktop tauri build   # desktop app .deb"
