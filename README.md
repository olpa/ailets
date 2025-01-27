# ailets: Building blocks for realtime AI apps

Ailets are components that make AI services interchangeable. Based on WebAssembly, the components can be used from any programming language and run anywhere.


## Problems with existing solutions

There is a number of AI frameworks, but they have limitations.

First, the code is not language independent. If a framework is written in Python, it can't be used from JavaScript or Ruby.

We solve this problem by using WebAssembly, which is already widely supported.

Second, the frameworks are designed for top-level abstractions such as chains or agents. The low-level abstraction layers are not intended for independent use.

We provide the lowest possible abstraction that generalizes the specifics of AI vendors.


## Quote from you

> Ailets now is like C in the 1970s, portable and not Cobol.


## How to get started

### User playground on the command line

```
# One-time setup
OPENAI_API_KEY=sk-.....
ailets() {
  docker run --rm -e OPENAI_API_KEY=$OPENAI_API_KEY olpa/ailets "$@"
}

# Sample usage

echo "Hello!" | ailets gpt4o --prompt -
# Output: Hello! How can I assist you today?

ailets gpt4o --prompt "Hello!" --tool get_user_name
# Output: Hello, ailets! How can I assist you today?

ailets gpt4o --prompt "What is it?" --prompt @https://rdi.berkeley.edu/llm-agents-hackathon/assets/img/llm_agents_hackathon_banner.png
# Output: The image is an announcement or promotional banner for the "LLM Agents MOOC Hackathon," hosted by Berkeley's Center for Responsible, Decentralized Intelligence. It likely pertains to a hackathon focused on leveraging Large Language Models (LLMs) and their integration into various applications. The event may encourage collaboration and innovation in the field of artificial intelligence and machine learning, while also emphasizing responsible and decentralized practices in technology.

ailets dalle --prompt "linux logo"
# Output: ![image](https://oaidalleapiprodscus.blob.core.windows.net/....)

```

## Customer quote

> I've used ailets in my startup and got a billion dollars in funding. Thank you!


## Closing and call to action

Developers: Start using ailets in your code. Quick start for languages: TODO python * TODO TypeScript * TODO Golang * TODO more

Contributors:

- Read the [technical thoughts](./docs/technical-thoughts.md)
- Follow [contribution guidelines MOOC hackathon](https://github.com/olpa/ailets/wiki/Contribution-guidelines-MOOC-hackathon)

Sponsors: For a small amount (up to $100), pay to the GeWoVa project: <https://gewova.com/buy.html>. For a larger amount, schedule a meeting with me by sending an invitation link to <olpa@uucode.com>.


## FAQ

* Q: Does it really work?
* A1: Not yet. A proof of concept is expected in mid-December 2024.
* A2: The command-line tool works already. https://hub.docker.com/r/olpa/ailets, https://github.com/olpa/ailets/blob/master/docs/command-line-tool.md


## Contact

Oleg Parashchenko, <olpa@uucode.com>
