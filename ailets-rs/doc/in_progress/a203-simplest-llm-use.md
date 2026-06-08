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

# Findings (current state of ailets-rs)

- `dagsh` (in `cli/`) keeps an in-process `actor_registry: HashMap<idname, fn(&dyn ActorRuntime) -> Result<(), String>>` (see `cli/src/lib.rs::make_env`). Today only three actors are registered: `cat`, `dbg`, `shell_input`.
- `cat` was migrated to this model by adding a plain native entry point `pub fn execute(runtime: &dyn ActorRuntime)` next to its old WASM/FFI entry point `execute_wasm` (`cat/src/lib.rs`). `execute` builds `AReader`/`AWriter` from `&dyn ActorRuntime` and calls the same business logic as the FFI version.
- `gpt`, `messages_to_query`, and `messages_to_markdown` were **not** migrated this way: they only expose FFI-style entry points (`_process_gpt`, `_process_messages`, `_messages_to_markdown` + `extern "C" execute_wasm`) built around `FfiActorRuntime`. They are not in `actor_registry` and `dagsh` cannot run them yet — this is the "actors might not be working" complication the initial task warned about.
- `gpt` additionally depends on a `DagOpsTrait` (create value/alias nodes, instantiate workflows, open pipes — see `gpt/src/dagops.rs`) whose only implementation, `DagOps`, is hard-wired to `&FfiActorRuntime`. A native, in-process `dagsh` build needs a `DagOpsTrait` impl backed by `ailetos::Environment` (the same object `cmd_node_inner` already uses for `add_node`/`add_value_node`/`add_aliases`).
- There is **no actor at all**, native or FFI, that performs the "query" step (the HTTP request to the LLM provider). The python pipeline's `.query.16` node has no Rust counterpart yet.
- `dagsh` scripts are read line-by-line (`cmd_source`, `content.lines()`), and `node value "<text>"` (`cmd_node_inner`/`parse_quoted_string`) does no escape processing — there is no way to embed a literal newline in a quoted value. The required seed value (`value.13`) is two JSON lines (JSONL), so we cannot paste it into a single `node value "..."` call as-is.

# Plan

The work breaks into three layers: (A) make the missing actors runnable inside `dagsh`, (B) get the seed chat-message value into the DAG, (C) wire it all together in `hello-llm.dagsh`. Layer (A) is the bulk of the effort and matches the "ask developers to implement a missing actor" warning in the initial task.

## Step 1 — Add native `execute` adapters for the simple actors

Mirror the `cat` migration (`cat/src/lib.rs::execute`) for the two actors that don't need `DagOpsTrait`:

- `messages_to_markdown`: add `pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String>` that wraps `_messages_to_markdown` with `AReader`/`AWriter` built `from_std`, alongside the existing `execute_wasm`.
- `messages_to_query`: same idea around `_process_messages`. This actor also reads an `EnvOpts` JSON blob (`env_opts.rs`) in addition to stdin — per the developer, use the same sample value the python version used (the `value.13` content given in the initial task is *the* input to feed through; don't invent a different prompt or reshape it). Confirm during implementation whether `_process_messages` needs a non-empty `EnvOpts` to process this exact input, and supply the smallest opts blob that makes it work if so.

Register both in `cli/src/lib.rs::make_env` (`reg.register("messages_to_markdown", messages_to_markdown::execute)`, etc.), and add `messages_to_markdown`/`messages_to_query` as path dependencies in `cli/Cargo.toml` (parallel to the existing `cat = { path = "../cat" }`).

## Step 2 — Add a stub `DagOpsTrait` implementation for native `gpt::execute`

Confirmed with the developer: in the "simplest llm use" workflow (no function/tool calls), `gpt`'s response handler never actually exercises `DagOpsTrait` — it's wired through generically but the no-tools code path doesn't call `value_node`/`alias`/`instantiate_with_deps`/etc. So Step 2 does **not** need a real `ailetos::Environment`-backed `DagOps`:

- Write a minimal stub struct that implements `DagOpsTrait` with each method either `unimplemented!()`/returning an error, or a harmless no-op — whichever keeps `_process_gpt`'s generic bounds happy without pretending to support capabilities this workflow doesn't use.
- Add `pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String>` to `gpt` that wires `AReader`/`AWriter` (mirroring `cat::execute`) and the stub `DagOps` into `_process_gpt`.
- Register `gpt` (e.g. as `gpt.response_to_messages`, matching the python plan's idname) in `actor_registry`.

A real `ailetos::Environment`-backed `DagOpsTrait` impl is deferred to whenever a workflow that actually needs tool/function calls is migrated — note that as follow-up scope, not blocking this task.

## Step 3 — Stub the "query" actor

No HTTP-request actor exists yet, and building a real one (HTTP client, auth, streaming SSE parsing of the LLM response) is out of scope for "simplest llm use". Add a minimal stub actor (e.g. crate `query` or a small module in `cli`) that:

- Reads its input (the query JSON produced by `messages_to_query`) and ignores it (or logs it via `--explain`),
- Writes a fixed, canned LLM-response JSON to stdout that `gpt`'s `response_to_messages` parser expects (mirror the shape the python `--dry-run` path would have produced for the prompt `"hello!"`).

Register it as `query` in `actor_registry`. Document in this file (or a follow-up task) that this stub must be replaced by a real HTTP-calling actor before the workflow can talk to an actual LLM provider — that is a separate, larger piece of work (auth/secrets, streaming parse, error handling, retries) that should get its own task.

## Step 4 — Get the seed chat-message value into the DAG

`value.13`'s content is two JSON lines:
```
[{"type": "ctl"}, {"role": "user"}]
[{"type": "text"}, {"text": "hello!"}]
```
`node value "..."` can't embed a literal `\n` today. Pick one:
- (a) Minimal CLI change: teach `parse_quoted_string` to unescape `\n` (and maybe `\t`, `\"`) inside quoted value strings — small, generally useful, low risk.
- (b) Avoid the CLI change: create the value via two `write` calls against a `shell_input`-style node instead of `node value`.
- (c) Add a `node value-file <path>` (or extend `source`) that loads literal file content as a value — heavier, probably overkill for this task.

Recommend (a): smallest change, and likely useful for future scripts that need multi-line seed data.

## Step 5 — Write `hello-llm.dagsh`

Modeled on `cli/scripts/sample.dagsh`, wire the chain that mirrors the python plan (innermost/upstream first):

```
set msgs  = node value "[{\"type\": \"ctl\"}, {\"role\": \"user\"}]\n[{\"type\": \"text\"}, {\"text\": \"hello!\"}]" --explain="Seed chat messages"
set toq   = node add messages_to_query --explain="gpt.messages_to_query"
dep $toq $msgs
set q     = node add query --explain="HTTP query (stub)"
dep $q $toq
set resp  = node add gpt.response_to_messages --explain="gpt.response_to_messages"
dep $resp $q
set md    = node add messages_to_markdown --explain="messages_to_markdown"
dep $md $resp
set end   = node alias .end $md

show
run $end
```
(exact actor idnames/registration strings to be finalized in Steps 1–3; adjust `dep` direction/`run` target to match `dagsh` conventions once the chain is buildable.)

## Step 6 — Verify

- `cargo build` the workspace; fix any trait/lifetime friction surfaced by the new native adapters. ✓ (clean build + clippy across `messages_to_query`, `messages_to_markdown`, `gpt`, `dagsh`)
- Run `dagsh`, `source cli/scripts/hello-llm.dagsh`, and `run` the terminal alias; confirm the DAG completes and prints a markdown rendering of the canned "hello" assistant response. ✓ — verified manually: `source cli/scripts/hello-llm.dagsh` followed by `run $end` builds the chain `msgs → messages_to_query → query (stub) → gpt.response_to_messages → messages_to_markdown` and prints `Hello! How can I help you today?`. (`run $end` is left commented out in the committed script, matching the convention of other sample scripts like `stdin_dag_flow.dagsh` — the user runs it manually.)
- ~~Add an integration test~~ — decided not needed for this task; skipped per developer.

# Status

All six steps implemented and verified; see commits tagged `A203` on this branch:
1. Native `execute` adapters for `messages_to_markdown`/`messages_to_query`, registered in `dagsh`.
2. `messages_to_query::execute` stubs `EnvOpts` to empty when the `Env` reader is unavailable (in-process `IoBridge` only materializes `Stdin` — see Findings) and warns on the actor's log stream.
3. `StubDagOps`/`StubWriter` + native `gpt::execute`, registered as `gpt.response_to_messages`.
4. Stub `query` actor (`cli/src/query_actor.rs`) emitting a fixed canned chat-completion SSE stream.
5. Heredoc syntax (`<<DELIM ... DELIM`) added to dagsh scripts (`find_heredoc_marker`, `execute_parts`, `cmd_source`) so the JSONL seed value can be embedded on one logical line.
6. `cli/scripts/hello-llm.dagsh` wires the full chain; verified end-to-end manually.

## Follow-up work (out of scope here, deserves its own task)

- A real, `ailetos::Environment`-backed `DagOpsTrait` implementation, needed once a tool-calling workflow is migrated (replaces `StubDagOps`).
- A real HTTP-calling "query" actor (auth/secrets, streaming SSE parsing, error handling, retries), replacing the canned stub.
- `ailetos::actor_syscall::io_bridge::IoBridge` support for materializing non-`Stdin` readers (specifically `Env`), so `messages_to_query` can receive real `EnvOpts` when run natively in `dagsh`.

# Decisions (resolved with the developer)

1. **`DagOpsTrait` for `gpt`**: don't build a real `ailetos::Environment`-backed impl — the no-tools "hello" workflow never exercises it. Use a stub impl (Step 2). A real impl is follow-up work for whenever a tool-calling workflow gets migrated.
2. **`EnvOpts` / seed input for `messages_to_query`**: use the exact sample value provided in the initial task (`value.13`'s JSONL chat-messages content) as the input fed through the pipeline — don't substitute a different prompt or format. Whether `messages_to_query` additionally needs non-empty `EnvOpts` to process it is to be discovered during implementation (Step 1).
3. **"query" actor**: a canned-response stub (Step 3) is acceptable for this task. Building the real HTTP-calling actor (auth, streaming SSE parsing, retries) is separate, larger follow-up work with its own task.
