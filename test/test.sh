#!/bin/sh

set -eu

div='\n\n\n'

##

echo ======== gpt4o ======== Hello

../command-line-tool/ailets0.py gpt4o --prompt "Hello!"

##

echo $div======== gpt4o ======== Embed an image

../command-line-tool/ailets0.py gpt4o --prompt "Describe the image." --prompt "@{image/png}tux.png"

##

echo $div======== gpt4o ======== Link to an image

../command-line-tool/ailets0.py gpt4o --prompt "Describe the image." --prompt "@https://gewova.com/assets/ui-blurred.png"

##
