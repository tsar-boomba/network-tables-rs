#!/bin/sh
cargo fmt --all
git commit -a -m "cargo fmt for release"
cargo release --workspace --allow-branch main --tag-name 'v{{version}}' --all-features -v -x $1
