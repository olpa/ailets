# Errors

## actor-to-files

When an actor fails, the runtime closes all its open files with an error. Files owned by the failed actor are closed with `EOWNERDEAD`. Files closed due to a clean actor cancellation use `ECANCELED`.

## writer-to-reader

When a file is closed with an error, readers of that file receive `EPIPE`.

## reader-to-actor

When an actor receives `EPIPE` reading its input, the actor fails with `EPIPE` and the runtime closes its output files with `EPIPE`.

## backward-propagation

When at least one reader of a file has been created and all of them have since closed, the writer receives `EPIPE` on its next write to that file.
