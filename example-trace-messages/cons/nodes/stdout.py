def stdout(value: list[str]) -> list[str]:
    """Print each value to stdout and return them unchanged."""
    for v in value:
        print(v)
    return value
