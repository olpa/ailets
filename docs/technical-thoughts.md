# Technical thoughts

## Architecture

Ailets are a combination of the actor model and the "everything is a file" paradigm.

- [actor model](https://en.wikipedia.org/wiki/Actor_model)
- [everything is a file](https://en.wikipedia.org/wiki/Everything_is_a_file)

> In response to a message it receives, an actor can: make local decisions, create more actors, send more messages, and determine how to respond to the next message received. Actors may modify their own private state, but can only affect each other indirectly through messaging

For steps in LLM pipelines, communication can be simplified by using standard input (stdin) for incoming messages and standard output (stdout) for outgoing messages.  Instead of multiple discrete messages, a single message with a streaming body is sufficient.


### Components

- Ailets itself
- Shared library of functions
- Orchestrator
- Host support
  - For driving the orchestrator
  - Tools exposed as Aitlets actors


## Build system

When using LLMs with tools, the workflow is no longer a straight pipeline. Instead, branching occurs, making features of build systems beneficial.

A sample dependency tree for a simple request to an LLM is shown below:

```
├── stdout.7 [⋯ not built]
│   ├── response_to_markdown.6 [⋯ not built]
│   │   ├── query.5 [⋯ not built]
│   │   │   ├── messages_to_query.4 [⋯ not built]
│   │   │   │   ├── prompt_to_messages.2 [⋯ not built]
│   │   │   │   │   ├── value.1 [✓ built] (Initial prompt)
│   │   │   │   ├── (param: credentials)
│   │   │   │   │   ├── credentials.3 [⋯ not built]
```

The steps proceed outward from the innermost part of the tree.

- `prompt_to_messages` converts a user prompt from `value.1` to an LLM JSON object.
- Concurrently, `credentials.3` retrieves an API key.
- Then, `messages_to_query.4` combines the user message and the credentials into an HTTP request specification.
- `query.5` executes the HTTP request.
- `response_to_markdown.6`
- `stdout.7` prints the result.

Below is another dependency tree, for the use of llm with tools. The tree is taken just before executing `response_to_markdown` step.

```
├── stdout.8 [⋯ not built]
│   ├── response_to_markdown.7 [⋯ not built]
│   │   ├── query.6 [✓ built]
│   │   │   ├── messages_to_query.5 [✓ built]
│   │   │   │   ├── prompt_to_messages.3 [✓ built]
│   │   │   │   │   ├── value.2 [✓ built] (Initial prompt)
│   │   │   │   ├── (param: credentials)
│   │   │   │   │   ├── credentials.4 [✓ built]
│   │   │   │   ├── (param: toolspecs)
│   │   │   │   │   ├── tool/get_user_name/spec.1 [✓ built]
```

The tree structure is similar to one used in basic LLMs. The additions are:

- the `tool/get_user_name/spec` node
- the `toolspecs` parameter of the `messages_to_query` function

When processing the result of `query`, the `response_to_markdown` step will detect that the language model hasn't generated content but instead intends to use a tool. At this point, the step stops to act as an agent and communicates with the orchestrator to construct a new dependency tree.

```
├── stdout.8 [⋯ not built]
│   ├── response_to_markdown.7 [✓ built]
│   │   ├── query.6 [✓ built]
│   │   │   ├── messages_to_query.5 [✓ built]
│   │   │   │   ├── prompt_to_messages.3 [✓ built]
│   │   │   │   │   ├── value.2 [✓ built] (Initial prompt)
│   │   │   │   ├── (param: credentials)
│   │   │   │   │   ├── credentials.4 [✓ built]
│   │   │   │   ├── (param: toolspecs)
│   │   │   │   │   ├── tool/get_user_name/spec.1 [✓ built]
│   ├── response_to_markdown.14 [⋯ not built]
│   │   ├── query.12 [⋯ not built]
│   │   │   ├── messages_to_query.11 [⋯ not built]
│   │   │   │   ├── prompt_to_messages.3 [✓ built]
│   │   │   │   │   ├── value.2 [✓ built] (Initial prompt)
│   │   │   │   ├── value.15 [✓ built] (Feed "tool_calls" from output to input)
│   │   │   │   ├── toolcall_to_messages.18 [⋯ not built]
│   │   │   │   │   ├── tool/get_user_name/call.17 [⋯ not built]
│   │   │   │   │   │   ├── value.16 [⋯ not built] (Tool call spec from llm)
│   │   │   │   │   ├── (param: llm_spec)
│   │   │   │   │   │   ├── value.16 [⋯ not built] (Tool call spec from llm)
│   │   │   │   ├── (param: credentials)
│   │   │   │   │   ├── credentials.4 [✓ built]
│   │   │   │   ├── (param: toolspecs)
│   │   │   │   │   ├── tool/get_user_name/spec.1 [✓ built]
```

The path to `response_to_markdown` is cloned and extended to call the tools before calling the llm.


## Streaming and orchestration

We should support streaming, allowing users to receive results incrementally as updates are generated during intermediate processing.

Therefore, instead of a traditional, step-by-step build system, we should implement a sophisticated orchestrator.


## Actor interface

Preliminary version.


*n_of_streams*

```
int n_of_streams(const char *param_name);
```

Return the number of input streams associated with a given parameter.

-  If `param_name` is `NULL`, the function assumes the default input parameter.
-  If the parameter name is unknown, the function returns -1 and sets `errno` to indicate the error.

The number of input streams associated with a parameter may change dynamically during program execution.


*open*

```
int open(const char *param_name, unsigned int idx);
```

Open the `idx`th stream associated with the parameter `param_name`.

Return a file descriptor on success, or `-1` on error.  In case of error, `errno` is set.


*read*

```
int read(int fd, voif buffer[count], int count)
```

Read up to `count` bytes from the file descriptor `fd` into the `buffer`.

Return the number of bytes read.  If the end of the file is encountered, `0` is returned.  On error, `-1` is returned, and `errno` is set appropriately.


*write*

```
int write(int fd, const void buffer[count], int count);
```

Writes up to `count` bytes from the `buffer` to the file descriptor `fd`.

Return the number of bytes written, which might be less than `count`.  On error, return `-1` and sets `errno` appropriately.

The following file descriptors are predefined:

- `STDOUT_FILENO = 1` (Standard output)
- `STDERR_FILENO = 2` (Standard error; conventionally used for logging)
- `METRICS_FD = 3` (Metrics output stream)
- `TRACE_FD = 4` (Traces output stream)


*errno, strerror*

```
int errno;
char *strerror(int errnum);
void perror(const char *s);
```

As seen in POSIX.


### Communication with the orchestrator

To be defined after collecting experience


## Content types

A helper library may be required to process formats:

1. json
2. markdown
3. rich text
4. link to a host resource

For (1): The format of the streamed json has yet to be defined. Let's use protobuf's [JSON mapping](https://protobuf.dev/programming-guides/proto3/#json) for inspiration.

For (3): The document format should to be based on (1) and [ProseMirror's document model](https://github.com/ProseMirror/prosemirror-model).

An example of a host resource is a WebSocket connection.


## Runtime library

As mentioned above, we may need a common functionality to handle content types.

Furthermore, if we want to minimize the size of ailets, we should provide basic functions such as string manipulation. The set of functions is based on the JavaScript specification.

Finally, we should orchestrate ailets to run together. There should be several levels of complexity, starting with the minimal glue and going towards an operating system.


## Far future: Ailets operating system for agents

If the future of AI is multi-agent systems, then we will have a special case of microservices. We have to adapt the knowledge and the tools to our special case.

- Observability
- kubernetes (not yet)

For observability, ailets should write to the "log", "metrics" and "traces" streams. We'll use [OpenTelemetry](https://opentelemetry.io/). To pass correlation IDs between agents we'll use message headers.

Kubernetes is the standard for microservices, and it should be the standard for multi-agent systems as well. There is [Krustlet](https://krustlet.dev/) to run WebAssembly loads in k8s, but: The project is dead, and it's not clear if we have the resources to build on top of Krustlet.

In my search for actor orchestration libraries, I've found [lunatic](https://github.com/lunatic-solutions/lunatic). Even if it's not exactly what we need, the system is small enough to be forked or be used for inspiration.

For debugging, the system should remember the contents of streams for later inspection. I am thinking of a key-value storage. Or even better: use a key-stream storage with chrooted key prefixes. Ailets would send messages through named streams "stdin", "stdout", "log" etc that refer to the entries in the key-stream storage.

The key-value storage can be thought of as a virtual file system.

There should be shell commands for the user to list ailets, stop, restart and kill them, to list files, to show the contents of the streams.
