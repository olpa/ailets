# Technical thoughts

## Architecture

Ailets are a mix of:

- the [actor model](https://en.wikipedia.org/wiki/Actor_model), and
- [everything is a file](https://en.wikipedia.org/wiki/Everything_is_a_file).

> In response to a message it receives, an actor can: make local decisions, create more actors, send more messages, and determine how to respond to the next message received. Actors may modify their own private state, but can only affect each other indirectly through messaging

For steps in llm pipelines, the communication can be simplified: use "stdin" for incoming messages and "stdout" for outgoing messages. Furthermore, instead of multiple messages, it's enough to have one message with a streaming body.


### Components

- ailets itself:
  - calls to llms
  - llm tools
- shared library
- orchestrator
- host support
  - to drive orchestrator
  - tools exposed as aitlets


## Build system

When using llm with tools, the workflow is no more a straight pipeline. Instead, there is branching, and it makes sense to have features of the build systems.

Sample dependency tree for a simple request to a llm:

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

The steps flow from the inners of the tree to outside.

- `prompt_to_messages` converts a user prompt from `value.1` to a llm json
- In parallel, `credentials.3` gets an api key from somewhere
- Then `messages_to_query.4` combines the user message and the credentials to a specification of an http request
- `query.5` executes the http request
- `response_to_markdown.6`
- `stdout.7` prints the result

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

The tree is similar to one of the basic llm use. The additions is:

- node `tool/get_user_name/spec`
- the parameter `toolspecs` of `messages_to_query`

Processing the result of `query`, the step `response_to_markdown` will notice that llm haven't generated content but want to use a tool instead. The step steps being an actor, and communicates with the orchestrator  to build a new dependency tree:

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

We should support streaming: User should get the result incrementally while updates are being generated in some intermediate step.

Therefore: Instead of a traditional build system with a step-by-step workflow, we should implement a sophisticated orchestrator.


## Actor interface

Preliminary version.


*n_of_streams*

```
int n_of_streams(const char *param_name);
```

Actors get input through named parameters, each named parameters is an array of streams. The function returns the number of them.

The `param_name` can be `NULL`, it means the default input parameter.

Return `-1` and set `errno` if the parameter name is unknown.

The number of input streams may change dynamically.


*open*

```
int open(const char *param_name, unsigned int idx);
```

Get a file descriptor to `idx`th stream of the parameter `param_name`.

Return `-1` and set `errno` in case of error.


*read*

```
int read(int fd, voif buffer[count], int count)
```

Read up to `count` bytes from the file descriptor `fd` into `buffer`.

Return `0` if the stream is ended.

Return `-1` and set `errno` in case of error.


*write*

```
int write(int fd, const void buffer[count], int count);
```

Write up to `count` bytes from `buffer`to the stream referred to by the file descriptor `fd`.

The number of bytes written may be less than `count`.

Return `-1` and set `errno` in case of error.

There is a fixed set of file descriptors:

- `stdout=1`
- `log=2`
- `metrics=3`
- `trace=4`


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
