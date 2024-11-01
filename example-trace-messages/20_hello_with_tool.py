from cons import mkenv, prompt_to_md, build_plan_writing_trace
from cons.nodes.tool_get_user_name import get_spec_for_get_user_name


env = mkenv()

tool_get_user_name = env.add_node("tool/get_user_name", get_spec_for_get_user_name)
prompt_to_md(env, tools=[tool_get_user_name])

#build_plan_writing_trace(env, "messages_to_query", "traces/hello_with_tool")
build_plan_writing_trace(env, "query", "traces/hello_with_tool")
