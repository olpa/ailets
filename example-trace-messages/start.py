from nodes import prompt_to_md
from cons import mkenv

env = mkenv()
result = prompt_to_md(env)  # Now returns the final markdown
print(result)  # Let's see the output
env.dump_nodes()
