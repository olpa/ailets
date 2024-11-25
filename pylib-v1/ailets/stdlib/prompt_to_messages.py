from ailets.cons.typing import INodeRuntime
from ailets.cons.util import read_all, write_all


def prompt_to_messages(runtime: INodeRuntime) -> None:
    fd_out = runtime.open_write(None)
    write_all(runtime, fd_out, b'{"role":"user","content":[')

    i = 0
    while i < runtime.n_of_streams(None):
        fd_in = runtime.open_read(None, i)
        i += 1
        write_all(runtime, fd_out, read_all(runtime, fd_in))
        runtime.close(fd_in)

    write_all(runtime, fd_out, b"]}")
    runtime.close(fd_out)
