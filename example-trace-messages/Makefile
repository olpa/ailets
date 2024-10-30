all: final-messaging-file

final-messaging-file: messages/25_query.json

lint:
	flake8 code/*py

fix:
	black code/*y

messages/15_messages.json: code/10_stdin_to_prompt.py messages/05_stdin.txt
	python $^ $@

messages/25_query.json: code/20_prompt_to_query.py messages/15_messages.json
	python $^ $@
