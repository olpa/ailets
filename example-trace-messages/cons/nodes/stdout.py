def stdout(value: list[str]) -> list[str]:
    """Print each value to stdout and return them unchanged."""
    for v in value:
        if v is not None and v != "":
            print(v)
    return value
