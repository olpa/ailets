# Technical thoughts

## Actors

Follow the [actor model](https://en.wikipedia.org/wiki/Actor_model) as closely as possible:

> In response to a message it receives, an actor can: make local decisions, create more actors, send more messages, and determine how to respond to the next message received. Actors may modify their own private state, but can only affect each other indirectly through messaging


## Format of messages

Like an HTTP request:

- Message name
- Message headers: map from strings to strings
- Body


## Content types

A helper library may be required to process formats:

1. plain text
2. json
3. streamed json or parsed json
4. streamed rich text
5. host stream
6. chat history

For (3): The format of the streamed json has yet to be defined. Let's use protobuf's [JSON mapping](https://protobuf.dev/programming-guides/proto3/#json) for inspiration.

For (4): The document format should to be based on (3) and [ProseMirror's document model](https://github.com/ProseMirror/prosemirror-model).

An example of (5) is a WebSocket connection.


## Runtime library

As mentioned above, we may need a common functionality to handle content types.

Furthermore, if we want to minimize the size of ailets, we should provide basic functions such as string manipulation. The set of functions is based on the JavaScript specification.

Finally, we should orchestrate ailets to run together. There should be several levels of complexity, starting with the minimal glue and going towards an operating system.


## Ailets operating system for agents

If the future of AI is multi-agent systems, then we will have a special case of microservices. We have to adapt the knowledge and the tools to our special case.

- Observability
- kubernetes (not yet)

For observability, ailets should write to the "log", "metrics" and "traces" streams. We'll use [OpenTelemetry](https://opentelemetry.io/). To pass correlation IDs between agents we'll use message headers.

Kubernetes is the standard for microservices, and it should be the standard for multi-agent systems as well. There is [Krustlet](https://krustlet.dev/) to run WebAssembly loads in k8s, but: The project is dead, and it's not clear if we have the resources to build on top of Krustlet.

In my search for actor orchestration libraries, I've found [lunatic](https://github.com/lunatic-solutions/lunatic). Even if it's not exactly what we need, the system is small enough to be forked or be used for inspiration.

For debugging, the system should remember the contents of streams for later inspection. I am thinking of a key-value storage. Or even better: use a key-stream storage with chrooted key prefixes. Ailets would send messages through named streams "stdin", "stdout", "log" etc that refer to the entries in the key-stream storage.

The key-value storage can be thought of as a virtual file system.

There should be shell commands for the user to list ailets, stop, restart and kill them, to list files, to show the contents of the streams.
