# pylib-v1 `ailets`: actor workflows in Python


## Summary

Ailets are a combination of the actor model and the "everything is a file" paradigm.

- [actor model](https://en.wikipedia.org/wiki/Actor_model)
- [everything is a file](https://en.wikipedia.org/wiki/Everything_is_a_file)

> In response to a message it receives, an actor can: make local decisions, create more actors, send more messages, and determine how to respond to the next message received. Actors may modify their own private state, but can only affect each other indirectly through messaging

For most steps in ailets pipelines, communication can be simplified by using standard input (stdin) for incoming messages and standard output (stdout) for outgoing messages. Instead of multiple discrete messages, a single message with a streaming body is sufficient.

The Python package `ailets` contains:

- Dependency tree for actors
- An orchestrator to run actors
- Sample actors to run `gpt4o` and `dall-e` LLM workflows


## Orchestration is hard, use the library

The plan was to write a Python proof-of-concept, then rewrite it in Rust and throw away the Python version. The plan is still the same, but considering that the orchestrator is a non-trivial piece of code, now I prefer to retain it.

If you need "actors" plus "everything is a file", I highly recommend to use `ailets`. Despite the code is not published on pypy, despite you need to cleanup unneeded LLM specifics, the time for integration in your code is much less than developing an alternative solution from scratch.

I have an advanced intuition about what can go wrong in concurrent code, but anyway I got race conditions and deadlocks in early versions. Fixing concurrency issues is a pain, and I've pained for you.

The rest (dependecies, plugins, sample actors) is easy. There is no need to make a library out of them because the implementation details are project-specific and I can't guess a good generalization.


# Complete example

The ready-to-run code is in the folder [./example/](./example/).

## "Copy" actor

A regular actor does:

- Read input
- Process it
- Write output

The interface with functions like `open`, `read`, `write`, `errno` resembles POSIX:

```
#
# Actor itself
#

async def copy_actor(runtime: INodeRuntime) -> None:
    buffer = bytearray(1024)

    while True:
        count = await runtime.read(StdHandles.stdin, buffer, len(buffer))
        if count == 0:
            break
        if count == -1:
            raise io_errno_to_oserror(runtime.get_errno())
        data = buffer[:count]
        logging.debug(f"{runtime.get_name()}: read {count} bytes: '{data.decode()}'")
        await write_all(runtime, StdHandles.stdout, data)


#
# Helpers
#

def io_errno_to_oserror(ecode: int) -> OSError:
    msg = "unknown error"
    try:
        msg = os.strerror(ecode)
    except ValueError:
        pass
    return OSError(ecode, msg)


async def write_all(runtime: INodeRuntime, fd: int, data: bytes) -> None:
    pos = 0
    while pos < len(data):
        count = await runtime.write(fd, data[pos:], len(data) - pos)
        if count == -1:
            raise io_errno_to_oserror(runtime.get_errno())
        pos += count

```

## "Stdin" actors

This actor doesn't get input from other actors. Instead, it asks the operating system for the input.

```
async def stdin_actor(runtime: INodeRuntime) -> None:
    try:
        while True:
            s = await asyncio.to_thread(input)
            logging.debug(f"{runtime.get_name()}: read {len(s)} bytes: '{s}'")
            await write_all(runtime, StdHandles.stdout, s.encode("utf-8"))
    except EOFError:
        pass
```

## Build a workflow

```
def build_flow(env: Environment) -> None:
    val = env.dagops.add_value_node(
        "(mee too)".encode("utf-8"),
        env.piper,
        env.processes,
        explain="Static text",
    )
    stdin = env.dagops.add_node(
        "stdin",
        stdin_actor,
        [],
        explain="Read from stdin",
    )
    foo = env.dagops.add_node(
        "foo",
        copy_actor,
        [Dependency(stdin.name)],
        explain="Copy",
    )
    bar = env.dagops.add_node(
        "bar",
        copy_actor,
        [Dependency(val.name), Dependency(foo.name)],
        explain="Copy",
    )
    baz = env.dagops.add_node(
        "baz",
        copy_actor,
        [Dependency(bar.name)],
        explain="Copy",
    )

    env.dagops.alias(".end", baz.name)
```

The logic is obvious, here is the visualization of the DAG:

```
├── baz.18 [⋯ not built] (Copy)
│   ├── bar.17 [⋯ not built] (Copy)
│   │   ├── value.13 [✓ built] (Static text)
│   │   ├── foo.16 [⋯ not built] (Copy)
│   │   │   ├── stdin.15 [⋯ not built] (Read from stdin)
```

The flow starts with `stdin.15` node and ends with `baz.18` node. In the middle of the flow, the static value is prepended to the stream.

## All together

The `main` function creates a build environment, defines a dependency tree by calling `build_flow` and finally starts the orchestrator. An optional tweak is the use of Sqlite to track the communication between actors.

Run, type: "1111", "&lt;Enter>", "2222", "&lt;Enter>", "3333", "&lt;Ctrl+D>".

```
$ python example/example.py
1111
(mee too)11112222
22223333
3333
```

Check the actors output:

```
$ sqlite3 example.db 'SELECT * FROM Dict'
stdin.15|111122223333
foo.16|111122223333
bar.17|(mee too)111122223333
baz.18|(mee too)111122223333
```


# Very technical details

## Running actors

The orchestrator starts actors only when needed: Only when all dependecies are progressed. An actor is progressed if it wrote anything to the output.

The code is in `processes.py`. The logic of the main loop is:

- If not yet, create an `awaker` coroutine
- Try to extend the pool of actors
- Wait until any of them (the awaker and the actors) finishes

The reason to have `awaker` is to repeat the loop iteration to potentially start a new actor:

- At the actor startup, the orchestrator code set ups observers that trigger an event when the actor progresses
- `awaker` waits on the event and exits after receiving it
- The `awaker` exit unblocks the main loop

The communication between actors happens through pipes outside the main orchestrator loop.
