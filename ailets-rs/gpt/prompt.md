State now: The code collects "tool calls" and processes them all together at the end
Task: Process "tool calls" in a streaming way, as they appear in input

To do the task, create and maintain a work plain "plan.md". Each step should be a logical human-reviewable step. Amount of changes should be less than 100 lines if possible. Stop after each step and let me review and commit changes.

Hints:

First, refactor the structure builder "structure_builder.rs" and its test. Only then continue with the actor ("lib.rs" and "handers.rs")

There are two modes of input. One is "all at once", see the fixture "funcall_response.txt", another one is "streaming", see "funcall_streaming.txt". In the "streaming" input mode, the "tool_calls" items are built using "delta"s.

The deltas of "argument" should be processed by "write_long_bytes".

The logic of "inject_tool_calls" is mostly the same, except that "detach" should be done only one, for the first tool call.

Assumptions for the input streaming mode:

Assume that only "arguments" can be spread over deltas. Other fields for a tool call are provided completely in the first delta.

The attribute "index" is the index of a tool call in tool calls. The value grows from 0, incrementing by 1 after a tool cal is finished. The value never decreases and never incremented by more than 1.

If assumptions are broken, the code should report an error. Add tests for the assumptions.
