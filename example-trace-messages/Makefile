all: final-messaging-file

final-messaging-file: messages/25_query.json

lint:
	flake8 --max-line-length=88 code/*py cons/*py cons/nodes/*py
	mypy -m cons
	mypy -m cons.nodes

fix:
	black code/*y cons/*py cons/nodes/*py

messages/15_messages.json: code/10_stdin_to_prompt.py messages/05_stdin.txt
	python $^ $@

messages/25_query.json: code/20_prompt_to_query.py messages/15_messages.json
	python $^ $@
