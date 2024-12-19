.PHONY: build test install

build:
	./build.sh
	mkdir -p dist
	cp ./target/wasm32-unknown-unknown/debug/*.wasm ./dist
	cp ./target/wasm32-unknown-unknown/debug/*.wasm ../pylib-v1/ailets/wasm/

test: build
	cd python-bindings && python -m pytest tests/

install: build
	cd python-bindings && pip install -e . 

fix:
	cargo fmt

lint:
	cargo check
	cargo clippy -- -W clippy::pedantic

lint-py:
	flake8 --max-line-length=88 run_actor.py
	mypy --strict run_actor.py

fix-py:
	black run_actor.py
