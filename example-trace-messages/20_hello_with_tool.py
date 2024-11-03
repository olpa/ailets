import argparse
from cons import mkenv, prompt_to_md, build_plan_writing_trace
from cons.nodes.tool_get_user_name import get_spec_for_get_user_name

TARGET = "query"

def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("-n", "--dry-run", action="store_true", 
                       help="Show what would be built without building")
    parser.add_argument("--print-plan", action="store_true",
                       help="Print the build plan")
    parser.add_argument("--load-plan-from-trace", type=str,
                       help="Load build plan from a trace file")
    return parser.parse_args()

def main():
    args = parse_args()
    env = mkenv()

    if args.load_plan_from_trace:
        # TODO: Implement plan loading from trace
        pass
    else:
        tool_get_user_name = env.add_node("tool/get_user_name", get_spec_for_get_user_name)
        prompt_to_md(env, tools=[tool_get_user_name])
    
    if args.print_plan or args.dry_run:
        env.print_dependency_tree(TARGET)
        
    if not args.dry_run:
        build_plan_writing_trace(env, TARGET, "traces/hello_with_tool")

if __name__ == "__main__":
    main()
