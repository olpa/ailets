import argparse
import localsetup
from ailets.cons import mkenv, prompt_to_md, build_plan_writing_trace, load_state_from_trace
from ailets.cons.nodes.tool_get_user_name import get_spec_for_get_user_name, run_get_user_name
from ailets.cons.pipelines import get_func_map

TARGET = "stdout"

def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("-n", "--dry-run", action="store_true", 
                       help="Show what would be built without building")
    parser.add_argument("--print-plan", action="store_true",
                       help="Print the build plan")
    parser.add_argument("--load-plan-from-trace", type=str,
                       help="Load build plan from a trace file")
    parser.add_argument("--one-step", action="store_true",
                       help="Build only one step at a time")
    return parser.parse_args()

def main():
    args = parse_args()
    env = mkenv()
    env.add_tool("get_user_name", (get_spec_for_get_user_name, run_get_user_name))

    if args.load_plan_from_trace:
        load_state_from_trace(env, args.load_plan_from_trace, get_func_map())
        state_num = int(args.load_plan_from_trace.split('/')[-1].split('_')[0])
        initial_counter = (state_num // 10) + 1
    else:
        prompt_to_md(env, tools=['get_user_name'])
        initial_counter = 1

    target_node_name = env.get_node_by_base_name("stdout").name
    
    if args.print_plan or args.dry_run:
        env.print_dependency_tree(target_node_name)
        
    if not args.dry_run:
        build_plan_writing_trace(
            env, 
            target_node_name, 
            "traces/hello_with_tool", 
            one_step=args.one_step,
            initial_counter=initial_counter
        )

if __name__ == "__main__":
    main()

