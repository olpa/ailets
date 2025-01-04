#!/bin/bash

DEPLOY_ENV=debug  # or "release"

# Build each crate for wasm
for crate in cat gpt messages_to_markdown; do
    echo "Building $crate for wasm..."
    cd $crate
    if [ "$DEPLOY_ENV" = "release" ]; then
        cargo build --target wasm32-unknown-unknown --release
    else
        cargo build --target wasm32-unknown-unknown
    fi
    cd ..
done
