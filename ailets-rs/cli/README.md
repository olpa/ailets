# DAG Shell (dagsh)

Interactive REPL for building and running DAGs manually.

## Building

```bash
cargo build -p dagsh
```

## Running

```bash
cargo run -p dagsh
```

## Commands

### Node Management

```
node add <actor> [--explain="text"]   Add actor node (actors: cat)
node value <data> [--explain="text"]  Add value node (constant data)
node alias <name> <target>            Add alias node
node list                             List all nodes with status
```

### Dependencies

```
dep <node> <dependency>               Add dependency (node depends on dependency)
deps <node>                           Show direct dependencies
```

### Visualization

```
show [node]                           Tree view (default: whole DAG)
```

### Execution

```
run [node]                            Run the DAG (default: last node)
```

### I/O

```
cat <node>                            Show output of a node
```

### Status

```
status                                Overall DAG status
status <node>                         Node status
```

### Session

```
source <file>                         Run script file
reset                                 Clear all nodes and start fresh
help                                  Show help
quit                                  Exit
```

## Example Session

Replicate the `stdin_dag_flow.rs` example using value nodes instead of stdin:

```
dagsh> node value "(mee too)" --explain="Static text"
Added value node 1: "(mee too)" (Static text)

dagsh> node value "Hello from dagsh!" --explain="Read from stdin"
Added value node 2: "Hello from dagsh!" (Read from stdin)

dagsh> node add cat --explain="Copy.foo"
Added node 3: cat (Copy.foo)

dagsh> dep 3 2
Added dependency: 3 depends on 2

dagsh> node add cat --explain="Copy.bar"
Added node 4: cat (Copy.bar)

dagsh> dep 4 1
dagsh> dep 4 3
Added dependency: 4 depends on 1
Added dependency: 4 depends on 3

dagsh> node add cat --explain="Copy.baz"
Added node 5: cat (Copy.baz)

dagsh> dep 5 4
Added dependency: 5 depends on 4

dagsh> node alias .end 5
Added alias 6: .end -> 5

dagsh> show 6
cat.5 [⋯ not built] # Copy.baz
└── cat.4 [⋯ not built] # Copy.bar
    ├── value.1 [✓ built] # Static text
    └── cat.3 [⋯ not built] # Copy.foo
        └── value.2 [✓ built] # Read from stdin

dagsh> run 6
Running DAG from node 6...
(mee too)Hello from dagsh!
DAG execution completed.
```

Or run from a script:

```
dagsh> source examples/stdin_dag_flow.dagsh
```

## Script Format

Scripts are plain text files with one command per line. Empty lines and lines starting with `#` are ignored.

```bash
# This is a comment
node value "hello" --explain="greeting"
node add cat
dep 2 1
run
```
