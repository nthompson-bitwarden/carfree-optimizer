#!/usr/bin/env bash
set -euo pipefail

# Requires: Git and an authenticated GitHub CLI (`gh auth login`).
# Creates a PRIVATE GitHub repo. Change to --public after reviewing the code
# and choosing an open-source license if desired.

cd "$(dirname "$0")/.."

if ! command -v gh >/dev/null 2>&1; then
  echo "Install GitHub CLI (gh): https://cli.github.com/" >&2
  exit 1
fi

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git init -b main
fi

git add .
if ! git diff --cached --quiet; then
  git commit -m "chore: initialize Rust-first car independence optimizer"
fi

gh repo create nthompson-bitwarden/carfree-optimizer --private --source=. --remote=origin --push
