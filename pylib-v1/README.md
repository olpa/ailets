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
