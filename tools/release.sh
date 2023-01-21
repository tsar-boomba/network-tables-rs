#!/bin/sh
cargo release --workspace --allow-branch main --tag-name 'v{{version}}' --all-features -v -x $1
