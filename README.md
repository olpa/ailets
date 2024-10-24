# ailets: Building blocks for realtime AI apps

It's components that make AI services interchangeable. Based on WebAssembly, the components can be used from any programming language and run anywhere.


## Problems with existing solutions

There is a number of AI frameworks, but they have limitations.

First, the code is not language independent. If a framework is written in Python, it can't be used from JavaScript or Ruby.

We solve this problem by using WebAssembly, which is already widely supported.

Second, the frameworks are designed for top-level abstractions such as chains or agents. The low-level abstraction layers are not intended for independent use.

We provide the lowest possible abstraction that generalizes the specifics of AI vendors.

Third, ailets are being designed for the upcoming project, a multi-agent orchestration system. This system will be the first to be built on modern DevOps principles for distributed systems.


## Quote from you

> Ailets now is like C in 1970s, portable and not Cobol.


## How to get started

### User playground on the command line

Start the ailets shell:

```
$ docker run -it olpa/ailets-shell

$ pass add openai
Enter password for openai: ****************
Retype password for openai: ****************
$ echo 'hello' \
    | text2query --curl gpt4 \
    | curl -K - \
    | response2text gpt4
Hello! How can I assist you today?
```

### User playground in the browser

Open the page TODO.

The shell in the browser is the same as in the Docker container. Run the same commands.

### Sample Python code

Here is the contents of `example.py`:

```
import sys, os, io, ailets
al = ailets.Env()

prompt = io.StringIO("hello")
t2q = al.get('text2query-gpt4').run(stdin=prompt)
q = al.get('query').run(stdin=t2q.stdout,
        env={'API_KEY': os.environ['OPENAI_API_KEY']})
r2t = al.get('response2text-gpt4', stdin=q.stdout)

while True:
        chunk = r2t.stdout.read(8)
        if not chunk:
            break
        sys.stdout.write(chunk)
```

Run the code:

```
$ docker run -it \
    -e OPENAI_API_KEY="$OPENAI_API_KEY" \
    -v $(pwd)/example.py:/code/example.py \
    olpa/ailets-shell \
    python /code/example.py
Hello! How can I assist you today?
```


## Customer quote

> I've used ailets in my startup and got a billion dollars in funding. Thank you!


## Closing and call to action

Developers: Start using ailets in your code. Quick start for languages: TODO python * TODO TypeScript * TODO Golang * TODO more

Contributors:

- Go to the [project tickets](https://github.com/olpa/ailets/issues) and filter by "good first issue"
- Read the [technical thoughts](./docs/technical-thoughts.md)

Sponsors: For a small amount (up to $100), pay to the GeWoVa project: <https://gewova.com/buy.html>. For a larger amount, schedule a meeting with me by sending an invitation link to <olpa@uucode.com>.


## FAQ

* Q: Does it really work?
* A: Not yet. A proof of concept is expected in mid-December 2024.


## Contact

Oleg Parashchenko, <olpa@uucode.com>
