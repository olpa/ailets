import json
from ailets.cons.typing import INodeRuntime


def credentials(runtime: INodeRuntime) -> None:
    value = {
        "Authorization": "Bearer ##OPENAI_API_KEY##",
        # "OpenAI-Organization": "",
    }

    output = runtime.open_write(None)
    output.write(json.dumps(value).encode("utf-8"))
    runtime.close_write(None)
