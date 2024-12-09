#!/bin/bash

# From Claude Sonnet:
# In conclusion, while you don't necessarily need wasm-pack for generating
# only WASM files, it can still be a useful tool due to its automation and
# optimization features.

# Install wasm-pack if not already installed
if ! command -v wasm-pack &> /dev/null; then
    #curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
    cargo install wasm-pack
fi

# Build each crate for wasm
for crate in ailets-types ailets-runtime ailets-stdlib; do
    echo "Building $crate for wasm..."
    wasm-pack build $crate --target nodejs
done
