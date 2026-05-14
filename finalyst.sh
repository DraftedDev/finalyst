#!/usr/bin/sh

set BINARY "./target/release/finalyst"

if not test -f $BINARY
    echo "Release binary not found. Please run 'build-release.sh' first."
    exit 1
end

./target/release/finalyst "$@"
