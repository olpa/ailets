# DAG Shell (dagsh)

Interactive REPL for building and running DAGs manually.

## Running

```bash
cargo run
```

With a startup script (continues in interactive mode after loading):

```bash
cargo run -- --load scripts/sample.dagsh
```

Type `help` for available commands.

## Example Session

```
$ cargo run
dagsh> node value "hello"
Added value node 1: "hello"
dagsh> show
value.1 [✓ built]
dagsh> run
Running DAG from node 1...
hello
DAG execution completed.
```

## Script Format

Scripts are plain text files with one command per line. Empty lines and lines starting with `#` are ignored.

```bash
# Create nodes and assign to variables
set val = node value "hello" --explain="greeting"
set cat1 = node add cat

# Add dependencies using variable references
dep $cat1 $val

# Run
run
```
