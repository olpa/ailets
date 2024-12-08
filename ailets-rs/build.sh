#!/bin/bash

# Install wasm-pack if not already installed
if ! command -v wasm-pack &> /dev/null; then
    curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

# Build each crate for wasm
for crate in ailets-types ailets-runtime ailets-stdlib; do
    echo "Building $crate for wasm..."
    wasm-pack build $crate --target python
done 