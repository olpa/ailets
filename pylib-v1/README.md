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
