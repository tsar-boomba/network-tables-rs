#!/bin/sh
cargo release --workspace --allow-branch main --tag-name --all-features 'v{{version}}' -v -x $1
