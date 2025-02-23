# Rust part

Build commands:

```
make build
make fix
make lint
make fix-py
make lint-py

export RUST_TEST_THREADS=1
cargo test
```

Run an actor:

```
python run_actor.py dist/cat.wasm
cat x.txt | python run_actor.py ./dist/messages_to_markdown.wasm in:=- out:=-
```

From inside an actor:

```
make -f ../Makefile fix
make -f ../Makefile lint
cargo test --test <file_name> <test_name> -- --nocapture --test-threads=1
```
