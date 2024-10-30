from nodes import prompt_to_md
from cons import mkenv

env = mkenv()
node = prompt_to_md(env)
env.dump_nodes()
