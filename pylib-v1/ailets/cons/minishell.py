import cmd
import asyncio
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
        self.env.notification_queue.notify(self.env.processes.get_progress_handle(), -1)
        return True

    def do_pipes(self, arg: str) -> None:
        """List pipes."""
        for pipe in self.env.piper.pipes.values():
            print(pipe)

    def do_waits(self, arg: str) -> None:
        """List waits."""
        for handle, clients in self.env.notification_queue.get_waits():
            print(f"{handle}: {clients}")

    def do_tasks(self, arg: str) -> None:
        """List tasks."""
        tasks = asyncio.all_tasks()
        for task in tasks:
            print(
                f"Task {task.get_name()} - "
                f"Cancelled: {task.cancelled()}, "
                f"Done: {task.done()}"
            )

    # Aliases
    do_quit = do_exit
