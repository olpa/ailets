.PHONY: build test install

build:
	./build.sh
	cp */pkg/*.wasm python-bindings/ailets_rs/

test: build
	cd python-bindings && python -m pytest tests/

install: build
	cd python-bindings && pip install -e . 