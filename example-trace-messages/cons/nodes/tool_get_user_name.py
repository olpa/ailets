def get_spec_for_get_user_name():
    """Return the specification for the get_user_name tool."""
    return {
        "name": "get_user_name",
        "description": (
            "Get the user's name. Call this whenever you need to know the name "
            "of the user."
        ),
        "parameters": {"type": "object", "additionalProperties": False},
    }
