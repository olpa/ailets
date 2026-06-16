### claude code

$ claude "say hello"

Wrong, only one positional argument is expected:
$ claude say hello

$ claude -p "query"
$ claude --print "query"
+ stdin in this case
Looks like "-p" is put before stdin

Claude: somehow handles file paths in "-p" prompt

Second positional argument: path to an image


--system-prompt
--system-prompt-file
--append-system-prompt
--append-system-prompt-file

# codex

$ codex "prompt as one argument"

non-interactive:

$ codex exec "some prompt"
$ codex e "some prompt"

Reads stdin, appends to the prompt

$ cat content.md | codex exec "summarize this document"

Using "-" as prompt forces stdin

"--image" or "-i"

- can be comma-separated
- can be repeated

System prompt override: in config or in the command line argument "-c"

### pi coding agent

```
$ pi [options] [@files...] [messages...]
```

First positional argument becomes the initial message
pre-submitted

```bash
pi "Read package.json" "What dependencies do we have?"
```

-p:
TTY check

```bash
cat README.md | pi -p "Summarize this text"
git diff | pi -p "Review these changes"
echo "context" | pi -p "Process this"
```

```bash
pi @prompt.md "Answer this"
pi -p @screenshot.png "What's in this image?"
pi @code.ts @test.ts "Review these files"
pi @context.md @diagram.png "Explain the architecture"
```

"-p"

### My plan

- Each positional argument: own item in the prompt
- Prefixed with @: read from file. Text items are supported
- --system prompt, --append-system-prompt. Several items in the prompt

