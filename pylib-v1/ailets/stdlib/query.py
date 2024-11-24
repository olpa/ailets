import json
import requests
import os
import re
from ailets.cons.typing import INodeRuntime

MAX_RUNS = 3  # Maximum number of runs allowed
_run_count = 0  # Track number of runs

secret_pattern = re.compile(
    r"""{{\s*secret\(\s*['"]([^'"]+)['"]\s*,\s*['"]([^'"]+)['"]\s*\)\s*}}"""
)


def resolve_secrets(value: str) -> str:
    """Replace {{secret('service','key')}} with actual secret value."""

    def get_secret(match):
        service = match.group(1)
        envvar = f"{service.upper()}_API_KEY"
        secret = os.environ.get(envvar)
        if secret is None:
            raise ValueError(f"Secret not found: {envvar}")
        return secret

    return secret_pattern.sub(get_secret, value)


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
        # Resolve secrets in headers and url
        headers = {k: resolve_secrets(v) for k, v in params["headers"].items()}
        url = resolve_secrets(params["url"])

        response = requests.request(
            method=params["method"],
            url=url,
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
