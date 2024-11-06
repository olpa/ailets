import json
import requests
import os
from typing import Dict, Any, List

MAX_RUNS = 3  # Maximum number of runs allowed
_run_count = 0  # Track number of runs


def query(query_params: List[Dict[str, Any]]) -> Any:
    """Perform the HTTP request to the API."""
    global _run_count
    _run_count += 1

    if _run_count > MAX_RUNS:
        raise RuntimeError(f"Exceeded maximum number of runs ({MAX_RUNS})")

    assert len(query_params) == 1, "Expected exactly one query params dict"
    params = query_params[0]  # Get the single params dict

    try:
        # Replace placeholder in Authorization header if it exists
        headers = params["headers"].copy()  # Make a copy to avoid modifying original
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
        return response.json()
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
