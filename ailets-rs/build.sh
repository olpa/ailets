#!/bin/bash

DEPLOY_ENV=debug  # or "release"

# Build each crate for wasm
for crate in ailets-types ailets-runtime ailets-stdlib; do
    echo "Building $crate for wasm..."
    cd $crate
    if [ "$DEPLOY_ENV" = "release" ]; then
        cargo build --target wasm32-unknown-unknown --release
    else
        cargo build --target wasm32-unknown-unknown
    fi
    cd ..
done

# Copy wasm files to python package
mkdir -p python-bindings/ailets_rs/
cp target/wasm32-unknown-unknown/$DEPLOY_ENV/*.wasm python-bindings/ailets_rs/
