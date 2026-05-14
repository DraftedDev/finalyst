#!/usr/bin/sh

BINARY="./target/release/finalyst"

if [ ! -f "$BINARY" ]; then
    echo "Release binary not found. Please run 'build-release.sh' first."
    exit 1
fi

./target/release/finalyst "$@"
