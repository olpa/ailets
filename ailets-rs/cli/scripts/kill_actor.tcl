# Kill Actor
#
# Demonstrates killing a specific actor mid-run.
#
# Topology:  shell_input → dbg (collect 3B) → cat
#
# The dbg actor collects 3 bytes then suspends. We kill it while suspended
# and verify the node transitions to terminated.

set src  [node shell_input "--explain=Input source"]
set dbg1 [node dbg --bytes-before-pause=3 "--explain=Collects 3 B then pauses"]
set out  [node cat "--explain=Output sink"]
dep $dbg1 $src
dep $out $dbg1

show

run --bg
status
show

# Write 3 bytes — dbg1 suspends after collecting them
write $src "hel"
wait suspended $dbg1
status
show

# Kill dbg1 while it is suspended
kill $dbg1
wait terminated $dbg1
status
show

# Clean up: close input and wait for the run to finish
close $src
status
show

status
show
