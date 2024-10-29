all: final-messaging-file

final-messaging-file: messages/15_messages.json

lint:
	flake8 code/*py

fix:
	black code/*y

messages/15_messages.json: code/10_stdin_to_prompt.py messages/05_stdin.txt
	python $^ $@
