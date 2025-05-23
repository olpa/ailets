import json
import aiohttp
import os
import re
from urllib.parse import urlparse
from ailets.atyping import INodeRuntime, StdHandles
from ailets.cons.util import write_all
from ailets.io.input_reader import read_all


MAX_RUNS = 3  # Maximum number of runs allowed
_run_count = 0  # Track number of runs

secret_pattern = re.compile(r"""{{\s*secret\s*}}""")


def resolve_secrets(value: str, url: str) -> str:
    """Replace {{secret}} with actual secret value."""

    provider = ""
    parsed = urlparse(url)
    domain_parts = parsed.netloc.split(".")
    if len(domain_parts) >= 2:
        provider = domain_parts[-2]
    else:
        provider = domain_parts[0].split(":")[0]

    def get_secret(match: re.Match[str]) -> str:
        envvar1 = f"{provider.upper()}_API_KEY"
        envvar2 = "LLM_API_KEY"
        secret = os.environ.get(envvar1)
        if secret is None:
            secret = os.environ.get(envvar2)
        if secret is None:
            raise ValueError(f"Secret not found: {envvar1} or {envvar2}")
        return secret

    return secret_pattern.sub(get_secret, value)


async def raise_for_status(response: aiohttp.ClientResponse) -> None:
    """Raise an exception if the HTTP response status indicates an error."""
    if response.ok:
        return

    body = bytearray()
    while len(body) < 1024:
        chunk = await response.content.read(1024 - len(body))
        if not chunk:
            break
        body.extend(chunk)

    body_text = body.decode("utf-8", errors="replace")
    raise ValueError(f"HTTP {response.status} {body_text}")


async def query(runtime: INodeRuntime) -> None:
    """Perform the HTTP request to the API."""
    global _run_count
    _run_count += 1

    if _run_count > MAX_RUNS:
        raise RuntimeError(f"Exceeded maximum number of runs ({MAX_RUNS})")

    params = json.loads((await read_all(runtime, StdHandles.stdin)).decode("utf-8"))

    try:
        # Resolve secrets in headers and url
        url = params["url"]
        headers = {k: resolve_secrets(v, url) for k, v in params["headers"].items()}
        url = resolve_secrets(url, url)

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
                await raise_for_status(response)
                async for chunk in response.content.iter_any():
                    await write_all(runtime, StdHandles.stdout, chunk)

    except aiohttp.ClientError as e:
        print(f"HTTP Request failed: {str(e)}")
        raise
    except json.JSONDecodeError as e:
        print(f"Failed to decode JSON response: {str(e)}")
        raise
