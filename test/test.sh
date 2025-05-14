#!/bin/sh

set -eu

div='\n\n\n'

##

echo ======== gpt ======== Hello

../command-line-tool/ailets0.py gpt --prompt "Hello!"

##

echo $div======== gpt ======== With a system message

../command-line-tool/ailets0.py gpt --prompt '''role="system"
---
You are a helpful assistant who answers in Spanish''' \
  --prompt "Hello!"

##

echo $div======== gpt ======== Hello with a tool

../command-line-tool/ailets0.py gpt --prompt "Hello!" --tool get_user_name

##

echo $div======== gpt ======== Embed an image

../command-line-tool/ailets0.py gpt --prompt "Describe the image." --prompt "@{image/png}tux.png"

##

echo $div======== gpt ======== Link to an image

../command-line-tool/ailets0.py gpt --prompt "Describe the image." --prompt "@https://gewova.com/assets/ui-blurred.png"

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

