# Sample DAG: replicates stdin_dag_flow.rs using value nodes
#
# This creates a pipeline:
#   val (static text) ──┐
#                       ├── bar (Copy.bar) ── baz (Copy.baz)
#   stdin (stdin stub) ── foo (Copy.foo) ──┘

set val [value "(mee too)" "--explain=Static text"]
set stdin [value "Hello from dagsh!" "--explain=Read from stdin"]

set foo [node cat "--explain=Copy.foo"]
dep $foo $stdin

set bar [node cat "--explain=Copy.bar"]
dep $bar $val
dep $bar $foo

set baz [node cat "--explain=Copy.baz"]
dep $baz $bar

set end [alias .end $baz]

show
