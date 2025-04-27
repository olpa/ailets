# Actor interface

The actor interface defines how ailets interact with their environment. Each ailet is an actor that receives input through streams and produces output through streams.


*get_name*

Name of the actor.


*open_read*

```
int open_read(const char *param_name, unsigned int idx);
```

Open the `idx`th stream associated with the parameter `param_name`.

Return a file descriptor on success, or `-1` on error.  In case of error, `errno` is set.


*open_write*

```
int open_write(const char *param_name);
```

Open a write stream associated with the parameter `param_name`.

Return a file descriptor on success, or `-1` on error.  In case of error, `errno` is set.


*read*

```
int read(int fd, voif buffer[count], int count)
```

Read up to `count` bytes from the file descriptor `fd` into the `buffer`.

Return the number of bytes read.  If the end of the file is encountered, `0` is returned.  On error, `-1` is returned, and `errno` is set appropriately.


*write*

```
int write(int fd, const void buffer[count], int count);
```

Writes up to `count` bytes from the `buffer` to the file descriptor `fd`.

Return the number of bytes written, which might be less than `count`.  On error, return `-1` and sets `errno` appropriately.

The following file descriptors are predefined:

- `STDOUT_FILENO = 1` (Standard output)
- `STDERR_FILENO = 2` (Standard error; conventionally used for logging)
- `METRICS_FD = 3` (Metrics output stream)
- `TRACE_FD = 4` (Traces output stream)


*close*

```
int close(int fd);
```

Close the file descriptor `fd`.


*get_next_name*

```
char *get_next_name(const char *base_name);
```

Return an unique name with the prefix `base_name`.


*get_errno*

```
int get_errno(void);
```

As seen in POSIX.


### Communication with the orchestrator

Based on the experience developing a tool for gpt4o, the following functions were sufficient:

- `add_value_node(value: bytes, explain: Optional[str])`: Creates a new value node.

- `instantiate_with_deps(target: str, aliases: dict[str, str])`: Creates a new instance of a plugin (either a tool or a model).

- `alias(alias: str, node_name: Optional[str])`: Creates or updates an alias pointing to a node.

- `detach_from_alias(alias: str)`: Freezes the dependencies of nodes associated with the alias, preventing them from being affected by subsequent changes to the alias.
