from ailets.cons.typing import INodeRuntime
from ailets.cons.util import read_all, write_all


def stdout(runtime: INodeRuntime) -> None:
    """Print each value to stdout and return them unchanged."""

    for i in range(runtime.n_of_streams(None)):
        fd = runtime.open_read(None, i)
        value = read_all(runtime, fd).decode("utf-8")
        runtime.close(fd)

        if value == "":
            continue
        print(value)

    fd = runtime.open_write(None)
    write_all(runtime, fd, "ok".encode("utf-8"))
    runtime.close(fd)
