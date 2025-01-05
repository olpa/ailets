FROM rust:latest as builder

RUN rustup target add wasm32-unknown-unknown

WORKDIR /build
COPY . .

RUN ./build.sh


FROM rust:latest

WORKDIR /dist
COPY --from=builder /build/target/wasm32-unknown-unknown/debug/*.wasm /dist/
