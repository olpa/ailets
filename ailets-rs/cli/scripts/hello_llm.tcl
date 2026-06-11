# Minimal "hello world" LLM workflow

set msgs [node value {[{"type": "ctl"}, {"role": "user"}]
[{"type": "text"}, {"text": "hello!"}]} "--explain=Seed chat messages"]

set toq [node add messages_to_query "--explain=gpt.messages_to_query"]
dep $toq $msgs

set q [node add query "--explain=HTTP query (stub)"]
dep $q $toq

set resp [node add gpt.response_to_messages "--explain=gpt.response_to_messages"]
dep $resp $q

set md [node add messages_to_markdown "--explain=messages_to_markdown"]
dep $md $resp

set end [node alias .end $md]

show
# run $end
