# Minimal circular dependency example
# Demonstrates how the DAG dumper handles circular references

set a [value "aa"]
set b [value "bb"]

dep $b $a
dep $a $b

show
