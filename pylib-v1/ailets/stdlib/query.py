import json
import aiohttp
import os
import re
from ailets.cons.atyping import INodeRuntime, StdHandles
from ailets.cons.util import write_all
from ailets.cons.input_reader import read_all


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

    params = json.loads((await read_all(runtime, StdHandles.stdin)).decode("utf-8"))

    try:
        # Resolve secrets in headers and url
        headers = {k: resolve_secrets(v) for k, v in params["headers"].items()}
        url = resolve_secrets(params["url"])

        if "body" in params:
            body_kwargs = {"json": params["body"]}
        elif "body_key" in params:
            key = params["body_key"]
            fd = await runtime.open_read(key)
            data = await read_all(runtime, fd)
            await runtime.close(fd)
            body_kwargs = {"data": data}
            headers["Content-length"] = str(len(data))
        else:
            raise ValueError("Invalid body type")

        async with aiohttp.ClientSession() as session:
            async with session.request(
                method=params["method"],
                url=url,
                headers=headers,
                **body_kwargs,
            ) as response:
                response.raise_for_status()
                async for chunk in response.content.iter_any():
                    await write_all(runtime, StdHandles.stdout, chunk)

    except aiohttp.ClientError as e:
        print(f"HTTP Request failed: {str(e)}")
        raise
    except json.JSONDecodeError as e:
        print(f"Failed to decode JSON response: {str(e)}")
        raise
