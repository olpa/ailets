import json
from ..node_runtime import NodeRuntime


def credentials(runtime: NodeRuntime) -> None:
    value = {
        "Authorization": "Bearer ##OPENAI_API_KEY##",
        # "OpenAI-Organization": "",
    }

    output = runtime.open_write(None)
    output.write(json.dumps(value))
    runtime.close_write(None)
