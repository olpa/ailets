# Initial task

We have to build a minimal llm workflow without support of llm tools. More precisely, we want to migrate from Python ($repo/command-line-tool and $repo/pylib-v1) to Rust.

Acceptance criteria:

- Create a new dagsh script "hello-llm.dagsh", which creates a workflow, so that "run" command will execute it

Complications: I'm not sure if actors are still working for Rust version. Check "cat" (as a ailtets-rs subproject) actor how it was migrated. In some cases, you'll need to ask developers to implement a missing actor.

The plan of the python version is:

```
$ ./ailets0.py gpt --prompt "hello!" --dry-run
├── .messages_to_markdown.18 [⋯ not built]
│   ├── .gpt.response_to_messages.17 [⋯ not built]
│   │   ├── .query.16 [⋯ not built]
│   │   │   ├── .gpt.messages_to_query.15 [⋯ not built]
│   │   │   │   ├── value.13 [✓ built] (chat messages)
```

The content of "value.13" is (use it for the dagsh-script):

```
[{"type": "ctl"}, {"role": "user"}]
[{"type": "text"}, {"text": "hello!"}]
```

^^^

As an experienced product manager, use this "initial task" description to create a full task with steps. The result should be in this file itself.
