import json
import requests
import os
from typing import Dict, Any


def query(query_params: Dict[str, Any]) -> str:
    """Perform the HTTP request to the API."""
    try:
        # Replace placeholder in Authorization header if it exists
        headers = query_params[
            "headers"
        ].copy()  # Make a copy to avoid modifying original
        if "Authorization" in headers:
            headers["Authorization"] = headers["Authorization"].replace(
                "##OPENAI_API_KEY##", os.environ["OPENAI_API_KEY"]
            )

        response = requests.request(
            method=query_params["method"],
            url=query_params["url"],
            headers=headers,
            json=query_params["body"],
        )
        response.raise_for_status()  # Raise an exception for bad status codes
        return response.json()
    except requests.exceptions.RequestException as e:
        print(f"HTTP Request failed: {str(e)}")
        if (
            hasattr(e, "response") and e.response is not None
        ):  # Check if response exists
            print(f"Response text: {e.response.text}")
        raise
    except json.JSONDecodeError as e:
        print(f"Failed to decode JSON response: {str(e)}")
        if response is not None:  # Check if response exists
            print(f"Raw response: {response.text}")
        raise
