# Introduction of a DAG


- Nodes have an idname (name which is like a type id)
- Nodes have a PID (process ID)
- The DAG refers to other nodes by PID not by a reference. It's ok to store all nodes in a vector and do a scan when searching.
- Nodes have a state: they should model a posix process, adding the state "not started yet" (I hope some posix extension has such state)
- Node has dependencies on other nodes and aliases
- DAG has aliases. An alias can name several nodes or other aliases. Maybe aliases can be implemented as special nodes, and the named nodes as dependencies
- There is a helper to get node (or alias) dependencies without aliases, which should be used by default.
