#!/bin/sh

set -eu

div='\n\n\n'

##

echo ======== gpt4o ======== Hello

../command-line-tool/ailets0.py gpt4o --prompt "Hello!"

##

echo $div======== gpt4o ======== With a system message

../command-line-tool/ailets0.py gpt4o --prompt '''role="system"
---
You are a helpful assistant who answers in Spanish''' \
  --prompt "Hello!"

##

echo $div======== gpt4o ======== Hello with a tool

../command-line-tool/ailets0.py gpt4o --prompt "Hello!" --tool get_user_name

##

echo $div======== gpt4o ======== Embed an image

../command-line-tool/ailets0.py gpt4o --prompt "Describe the image." --prompt "@{image/png}tux.png"

##

echo $div======== gpt4o ======== Link to an image

../command-line-tool/ailets0.py gpt4o --prompt "Describe the image." --prompt "@https://gewova.com/assets/ui-blurred.png"

##

echo $div======== dalle ======== Generate an image, get as a link

../command-line-tool/ailets0.py dalle --prompt 'linux logo'


##

echo $div======== dalle ======== Variate an image, get it back

../command-line-tool/ailets0.py dalle --prompt '''dalle_task="variations"
response_format="b64_json"
n=3
---
''' --prompt @../test/tux.png

