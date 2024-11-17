import json
from ailets.cons.typing import INodeRuntime


def prompt_to_messages(runtime: INodeRuntime) -> None:
    n_prompts = runtime.n_of_streams(None)
    n_types = runtime.n_of_streams("type")

    if n_prompts != n_types:
        raise ValueError("Inputs and type streams have different lengths")

    def to_llm_item(runtime: INodeRuntime, i: int) -> dict:
        content = runtime.open_read(None, i).getvalue()
        content_type = runtime.open_read("type", i).getvalue()

        if content_type == "text":
            return {"role": "user", "content": content}
        elif content_type == "image_url":
            return {
                "role": "user",
                "content": [{"type": "image_url", "image_url": {"url": content}}],
            }
        else:
            raise ValueError(f"Unsupported content type: {content_type}")

    messages = [to_llm_item(runtime, i) for i in range(n_prompts)]

    # Write output
    output = runtime.open_write(None)
    output.write(json.dumps(messages))
    runtime.close_write(None)
