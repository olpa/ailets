# A267 – Fix `run_one_step_multialias_does_not_hang` under parallel tests

## Status

Deferred.  The test passes alone and with `--test-threads=1` but hangs when the
full suite runs in parallel.

## Root cause

`cmd_run --one-step $alias` computes `wait_targets` after calling
`executor.submit()`.  Under high parallelism the executor thread can run cat_a
to completion before `wait_targets` is evaluated.  `find(is_not_started)` then
skips cat_a (now Terminated) and returns cat_b (NotStarted, never submitted).
`executor.join(cat_b)` registers a waiter that never fires → hang.

**Partial fix already applied:** `wait_targets` is now computed *before*
`executor.submit()` so cat_a is guaranteed NotStarted at evaluation time.
Whether this fully closes the window under all OS schedulers still needs a
stress run to confirm.

## Next step

Run the full test suite (not just the one test) several times in parallel and
confirm the hang is gone.  If it still surfaces, investigate whether the
executor's internal mpsc channel introduces additional latency between `submit`
and the job being picked up.
