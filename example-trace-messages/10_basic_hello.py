import setup
from cons import mkenv, prompt_to_md, build_plan_writing_trace


env = mkenv()
node = prompt_to_md(env)
build_plan_writing_trace(env, node.name, "traces/basic_hello")
