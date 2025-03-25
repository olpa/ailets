import cmd
from ailets.cons.environment import Environment


class MiniShell(cmd.Cmd):
    intro = "Welcome to MiniShell. Type help or ? to list commands.\n"
    prompt = "(ailets) "

    def __init__(self, env: Environment) -> None:
        super().__init__()
        self.env = env

    def do_echo(self, arg: str) -> None:
        """Echo the input arguments.
        Usage: echo [message]"""
        print(arg)

    def do_help(self, arg: str) -> None:
        """List available commands with "help" or detailed help with "help cmd"."""
        super().do_help(arg)

    def do_exit(self, arg: str) -> bool:
        """Exit the shell."""
        print("Goodbye!")
        return True

    def do_ps(self, arg: str) -> None:
        """List processes."""
        for task in self.env.processes.get_processes():
            print(task)

    def do_awake(self, arg: str) -> bool:
        """Awake a process."""
        self.env.processes.mark_node_started_writing()
        return True

    def do_streams(self, arg: str) -> None:
        """List streams."""
        for stream in self.env.streams._streams:
            print(stream)

    def do_waits(self, arg: str) -> None:
        """List waits."""
        for handle, clients in self.env.notification_queue.get_waits():
            print(f"{handle}: {clients}")

    # Aliases
    do_quit = do_exit
