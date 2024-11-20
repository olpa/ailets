import json
from ailets.cons.typing import INodeRuntime


def credentials(runtime: INodeRuntime) -> None:
    value = {
        "Authorization": "Bearer ##OPENAI_API_KEY##",
    }

    output = runtime.open_write(None)
    output.write(json.dumps(value))
    runtime.close_write(None) 