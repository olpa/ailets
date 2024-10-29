import json
import sys


(_, in_file, out_file) = sys.argv


with open(in_file) as h:
    md_input = h.read().strip()

message = {
    "role": "user",
    "content": md_input,
}
messages = [message]

with open(out_file, "w") as h:
    json.dump(messages, h)
