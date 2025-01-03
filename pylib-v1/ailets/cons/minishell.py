import cmd

class MiniShell(cmd.Cmd):
    intro = 'Welcome to MiniShell. Type help or ? to list commands.\n'
    prompt = '(ailets) '

    def do_echo(self, arg):
        """Echo the input arguments.
        Usage: echo [message]"""
        print(arg)

    def do_help(self, arg):
        """List available commands with "help" or detailed help with "help cmd"."""
        super().do_help(arg)

    def do_exit(self, arg):
        """Exit the shell."""
        print('Goodbye!')
        return True

    # Aliases
    do_quit = do_exit

def main():
    MiniShell().cmdloop()

if __name__ == '__main__':
    main()
