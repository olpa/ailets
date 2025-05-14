# pylib-v1 `ailets`: Actor Workflows in Python

## Summary

Ailets are a combination of the actor model and the "everything is a file" paradigm.

- [Actor model](https://en.wikipedia.org/wiki/Actor_model)
- [Everything is a file](https://en.wikipedia.org/wiki/Everything_is_a_file)

> In response to a message it receives, an actor can: make local decisions, create more actors, send more messages, and determine how to respond to the next message received. Actors may modify their own private state, but can only affect each other indirectly through messaging.

For most steps in ailets pipelines, communication can be simplified by using standard input (stdin) for incoming messages and standard output (stdout) for outgoing messages. Instead of multiple discrete messages, a single message with a streaming body is sufficient.

The Python package `ailets` contains:

- Dependency tree for actors
- An orchestrator to run actors
- Sample actors to run `gpt` and `dall-e` LLM workflows

## Orchestration is Hard, Use the Library

The plan was to write a Python proof-of-concept, then rewrite it in Rust and throw away the Python version. The plan is still the same, but considering that the orchestrator is a non-trivial piece of code, I now prefer to retain it.

If you need "actors" plus "everything is a file," I highly recommend using `ailets`. Despite the code not being published on PyPI, and despite needing to clean up unneeded LLM specifics, the time for integration into your code is much less than developing an alternative solution from scratch.

I have an advanced intuition about what can go wrong in concurrent code, but I still encountered race conditions and deadlocks in early versions. Fixing concurrency issues is a pain, and I've experienced this for you.

The rest (dependencies, plugins, and sample actors) are easy. There is no need to make a library out of them because the implementation details are project-specific, and I can't guess a good generalization.

# Complete Example

The ready-to-run code is in the folder [./example/](./example/).

## "Copy" Actor

A regular actor does:

- Read input
- Process it
- Write output

The interface with functions like `open`, `read`, `write`, and `errno` resembles POSIX.

```python
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

## "Stdin" Actors

This actor doesn't get input from other actors. Instead, it asks the operating system for the input.

```python
async def stdin_actor(runtime: INodeRuntime) -> None:
    try:
        while True:
            s = await asyncio.to_thread(input)
            logging.debug(f"{runtime.get_name()}: read {len(s)} bytes: '{s}'")
            await write_all(runtime, StdHandles.stdout, s.encode("utf-8"))
    except EOFError:
        pass
```

## Build a Workflow

```python
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

The logic is obvious; here is the visualization of the DAG:

```
├── baz.18 [⋯ not built] (Copy)
│   ├── bar.17 [⋯ not built] (Copy)
│   │   ├── value.13 [✓ built] (Static text)
│   │   ├── foo.16 [⋯ not built] (Copy)
│   │   │   ├── stdin.15 [⋯ not built] (Read from stdin)
```

The flow starts with the `stdin.15` node and ends with the `baz.18` node. In the middle of the flow, the static value is prepended to the stream.

## All Together

The `main` function creates a build environment, defines a dependency tree by calling `build_flow`, and finally starts the orchestrator. An optional tweak is the use of SQLite to track the communication between actors.

Run, type: "1111", "&lt;Enter>", "2222", "&lt;Enter>", "3333", "&lt;Ctrl+D>".

```bash
$ python example/example.py
1111
(mee too)11112222
22223333
3333
```

Check the actors' output:

```bash
$ sqlite3 example.db 'SELECT * FROM Dict'
stdin.15|111122223333
foo.16|111122223333
bar.17|(mee too)111122223333
baz.18|(mee too)111122223333
```

# Very Technical Details

## Running Actors

The orchestrator will only start actors when all dependencies are progressed. An actor is progressed by writing anything to the output.

The code for this can be found in `processes.py`. The main loop logic follows these steps:

- Create an `awaker` coroutine if one does not already exist.
- Attempt to add more actors to the pool.
- Wait for any actor or the `awaker` to finish.
- Repeat.

The purpose of the `awaker` is to restart the loop iteration to potentially start a new actor:

- When an actor starts up, the orchestrator code sets up observers to trigger an event when the actor makes progress.
- The `awaker` waits for this event and exits once it receives it.
- Exiting the `awaker` unblocks the main loop.

Communication between actors occurs through pipes outside of the main orchestrator loop.

## Pipes

A pipe includes several important elements:

- An output stream for an actor
- A buffer for the stream data
- An associated writer to the buffer
- A reader-factory to read from the buffer
- An entry in a key-value storage

All these elements work together harmoniously.

The concept of pipes has been a significant advancement in the project. By introducing pipes, the code was able to be organized more effectively, moving away from its reliance on abstract files.

The code can be found in `ailets.io`.

## Notification Queue

The queue enables synchronization on handles, which are primarily handles of open files, but can also be handles of operations, such as "actor is progressed" or "kv entry is created". The code can be found in `notification_queue.py`.

The queue supports two methods: observers and await.

In the first method, using observers or listeners, a callback subscribes to a handle. When an event occurs, notifiers notify all the listeners. A callback must be prepared to be called in a different thread (the notifier's thread) than where it was created.

The second method, using await, requires a complex client workflow (check-lock-check-wait) outlined within the source code. It allows writing:

```python
await queue.wait_unsafe(handle, "debug hint")
```

This is compatible with asynchronous code, and it resumes in the same thread as before the wait.

## Loops in RAG

Cycles in actor dependencies are not allowed, but loops are necessary for LLM function-calling workflows.

The solution is to dynamically unroll the loop. If there is a need for an additional iteration, the dag interface enables the addition of new actors to the dependencies. The dispatcher will detect the new actors and proceed with their construction.

Below is an example of the environment immediately after executing an LLM:

```
├── .messages_to_markdown.21 [⋯ not built]
│   ├── .gpt.response_to_messages.20 [⋯ not built]
│   │   ├── .query.19 [✓ built]
│   │   │   ├── .gpt.messages_to_query.18 [✓ built]
│   │   │   │   ├── .prompt_to_messages.17 [✓ built]
│   │   │   │   │   ├── value.15 [✓ built] (Prompt)
│   │   │   │   ├── (param: toolspecs)
│   │   │   │   │   ├── value.13 [✓ built] (Tool spec get_user_name)
```

When processing the result of the `query`, the `response_to_messages` step will detect that the language model hasn't generated content but instead intends to use a tool. At this point, the step stops acting as an actor and communicates with the orchestrator to construct a new dependency tree.

```
├── .messages_to_markdown.21 [⋯ not built]
│   ├── .gpt.response_to_messages.20 [✓ built]
│   │   ├── .query.19 [✓ built]
│   │   │   ├── .gpt.messages_to_query.18 [✓ built]
│   │   │   │   ├── .prompt_to_messages.17 [✓ built]
│   │   │   │   │   ├── value.15 [✓ built] (Prompt)
│   │   │   │   ├── (param: toolspecs)
│   │   │   │   │   ├── value.13 [✓ built] (Tool spec get_user_name)
│   ├── .gpt.response_to_messages.39 [⋯ not built]
│   │   ├── .query.38 [⋯ not built]
│   │   │   ├── .gpt.messages_to_query.37 [⋯ not built]
│   │   │   │   ├── .prompt_to_messages.17 [✓ built]
│   │   │   │   │   ├── value.15 [✓ built] (Prompt)
│   │   │   │   ├── value.29 [✓ built] (tool calls in chat history - get_user_name)
│   │   │   │   ├── .toolcall_to_messages.36 [⋯ not built]
│   │   │   │   │   ├── .tool.get_user_name.call.33 [⋯ not built]
│   │   │   │   │   │   ├── value.31 [✓ built] (tool input - get_user_name)
│   │   │   │   │   ├── (param: llm_tool_spec)
│   │   │   │   │   │   ├── value.34 [✓ built] (tool call spec - get_user_name)
│   │   │   │   ├── (param: toolspecs)
│   │   │   │   │   ├── value.13 [✓ built] (Tool spec get_user_name)
```

The previous loop iteration `.gpt.response_to_messages.20` has been completed, and the new iteration `.gpt.response_to_messages.39` is now in the build plan.
