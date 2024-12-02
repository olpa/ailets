import json
import requests
import os
import re
from ailets.cons.atyping import INodeRuntime
from ailets.cons.util import read_all, write_all

MAX_RUNS = 3  # Maximum number of runs allowed
_run_count = 0  # Track number of runs

secret_pattern = re.compile(
    r"""{{\s*secret\(\s*['"]([^'"]+)['"]\s*,\s*['"]([^'"]+)['"]\s*\)\s*}}"""
)


def resolve_secrets(value: str) -> str:
    """Replace {{secret('service','key')}} with actual secret value."""

    def get_secret(match: re.Match[str]) -> str:
        service = match.group(1)
        envvar = f"{service.upper()}_API_KEY"
        secret = os.environ.get(envvar)
        if secret is None:
            raise ValueError(f"Secret not found: {envvar}")
        return secret

    return secret_pattern.sub(get_secret, value)


async def query(runtime: INodeRuntime) -> None:
    """Perform the HTTP request to the API."""
    global _run_count
    _run_count += 1

    if _run_count > MAX_RUNS:
        raise RuntimeError(f"Exceeded maximum number of runs ({MAX_RUNS})")

    assert runtime.n_of_streams(None) == 1, "Expected exactly one query params dict"
    fd = await runtime.open_read(None, 0)  # Get the single params dict
    params = json.loads((await read_all(runtime, fd)).decode("utf-8"))
    await runtime.close(fd)

    try:
        # Resolve secrets in headers and url
        headers = {k: resolve_secrets(v) for k, v in params["headers"].items()}
        url = resolve_secrets(params["url"])

        if "body" in params:
            body_kwargs = {"json": params["body"]}
        elif "body_stream" in params:
            stream_name = params["body_stream"]
            n_streams = runtime.n_of_streams(stream_name)
            assert (
                n_streams == 1
            ), f"Expected exactly one stream '{stream_name}', got {n_streams}"
            fd = await runtime.open_read(stream_name, 0)
            data = await read_all(runtime, fd)
            await runtime.close(fd)
            body_kwargs = {"data": data}
            headers["Content-length"] = str(len(data))
        else:
            raise ValueError("Invalid body type")

        response = requests.request(
            method=params["method"],
            url=url,
            headers=headers,
            **body_kwargs,
        )
        response.raise_for_status()  # Raise an exception for bad status codes

        value = response.json()
        fd = await runtime.open_write(None)
        await write_all(runtime, fd, json.dumps(value).encode("utf-8"))
        await runtime.close(fd)

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
