#!/bin/sh
cargo release --workspace --allow-branch main --tag-name 'v{{version}}' -v -x $1
