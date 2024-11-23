import json
import requests
import os
from ailets.cons.typing import INodeRuntime

MAX_RUNS = 3  # Maximum number of runs allowed
_run_count = 0  # Track number of runs


def query(runtime: INodeRuntime) -> None:
    """Perform the HTTP request to the API."""
    global _run_count
    _run_count += 1

    if _run_count > MAX_RUNS:
        raise RuntimeError(f"Exceeded maximum number of runs ({MAX_RUNS})")

    assert runtime.n_of_streams(None) == 1, "Expected exactly one query params dict"
    hparams = runtime.open_read(None, 0)  # Get the single params dict
    params = json.loads(hparams.read())

    try:
        # Replace placeholder in Authorization header if it exists
        headers = params["headers"]
        if "Authorization" in headers:
            headers["Authorization"] = headers["Authorization"].replace(
                "##OPENAI_API_KEY##", os.environ["OPENAI_API_KEY"]
            )

        response = requests.request(
            method=params["method"],
            url=params["url"],
            headers=headers,
            json=params["body"],
        )
        response.raise_for_status()  # Raise an exception for bad status codes

        value = response.json()
        output = runtime.open_write(None)
        output.write(json.dumps(value).encode("utf-8"))
        runtime.close_write(None)

    except requests.exceptions.RequestException as e:
        print(f"HTTP Request failed: {str(e)}")
        if hasattr(e, "response") and e.response is not None:
            print(f"Response text: {e.response.text}")
        raise
    except json.JSONDecodeError as e:
        print(f"Failed to decode JSON response: {str(e)}")
        if response is not None:
            print(f"Raw response: {response.text}")
        raise
