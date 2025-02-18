.PHONY: build test install

build:
	./build.sh
	mkdir -p dist
	cp ./target/wasm32-unknown-unknown/debug/*.wasm ./dist
	cp ./target/wasm32-unknown-unknown/debug/*.wasm ../pylib-v1/ailets/wasm/

fix:
	cargo fmt

lint:
	cargo check
	cargo clippy -- -W clippy::pedantic -W clippy::panic -W clippy::unwrap_used -W clippy::expect_used -W clippy::indexing_slicing

lint-py:
	flake8 --max-line-length=88 run_actor.py
	mypy --strict run_actor.py

fix-py:
	black run_actor.py

build-docker-image:
	docker build -t ailets-wasm-actors .

