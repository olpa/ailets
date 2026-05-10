# dagsh — DAG shell

dagsh is an interactive shell for building and running directed acyclic graphs (DAGs) of actors. Actors are small programs that read from their inputs and write to their outputs; the DAG wires them together.

## Live core model

dagsh keeps a persistent execution environment for the duration of the session, similar to the Erlang shell. Nodes you create, dependencies you declare, and data actors have produced all remain available after a run finishes. The next `run` is incremental — it sees all prior state and only executes what is needed.

This means you can:
- Build a DAG incrementally across multiple commands
- Re-run after fixing a broken node without starting over
- Inspect the output of any node at any time with `cat`
- Have multiple nodes running concurrently in the background

## Commands

See [commands.md](commands.md) for the full command reference.

## Output

Output from nodes is only shown when you explicitly ask for it — via `run`, `join`/`await`, or `follow`. Nodes running in the background produce no output unless you attach to them.

Each output line is prefixed with the node name so you can tell sources apart when multiple nodes are active:

```
[nodename] some output text
```

When any node terminates — including ones running in the background — dagsh immediately prints a notification, even if you are mid-input:

```
[nodename] done
[nodename] FAILED (exit 1)
```

This is always active, matching Linux shell behavior with `set -b`.

## Session exit

On `quit` or `exit`, dagsh terminates all running nodes before shutting down.
