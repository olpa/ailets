import os


def get_spec_for_get_user_name():
    """Return the specification for the get_user_name tool."""
    return {
        "name": "get_user_name",
        "description": (
            "Get the user's name. Call this whenever you need to know the name "
            "of the user."
        ),
        "parameters": {
            "type": "object",
            "properties": {},
            "additionalProperties": False,
        },
    }


def run_get_user_name() -> str:
    """Run the get_user_name tool."""
    return os.environ["USER"]
