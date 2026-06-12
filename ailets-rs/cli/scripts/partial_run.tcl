# Partial run: demonstrate stop conditions
#
# Creates a linear pipeline: value.1 -- cat.2 -- cat.3 -- cat.4
#
# Try these commands after loading:
#   run --one-step           # Run only value.1
#   run --stop-before $cat3  # Run value.1, cat.2
#   run --stop-after $cat2   # Run value.1, cat.2
#   run                      # Run remaining nodes

set v1 [value "hello" "--explain=Input"]

set cat2 [node cat "--explain=Step 1"]
dep $cat2 $v1

set cat3 [node cat "--explain=Step 2"]
dep $cat3 $cat2

set cat4 [node cat "--explain=Step 3"]
dep $cat4 $cat3

show
status
