# Replicate stdin_dag_flow.rs using TCL
# Instead of the real stdin actor, use a value node as a stub

# Create value node for static text
set val [value "(mee too)" "--explain=Static text"]

# Create value node as stdin stub (instead of stdin actor)
set stdin [value "Hello from dagsh!" "--explain=Read from stdin"]

# Create Copy.foo: cat depending on stdin stub
set foo [node cat "--explain=Copy.foo"]
dep $foo $stdin

# Create Copy.bar: cat depending on val and foo
set bar [node cat "--explain=Copy.bar"]
dep $bar $val
dep $bar $foo

# Create Copy.baz: cat depending on bar
set baz [node cat "--explain=Copy.baz"]
dep $baz $bar

# Create alias
set end [alias .end $baz]

# Show the DAG
show $end

# List all nodes
nodes

# Run the DAG
# run $end
