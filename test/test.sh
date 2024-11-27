#!/bin/sh

set -eux

##

echo -- ======== gpt4o ======== Hello

../command-line-tool/ailets0.py gpt4o --prompt "Hello!"

##

echo -- ======== gpt4o ======== Embed an image

../command-line-tool/ailets0.py gpt4o --prompt "Describe the image." --prompt "@{image/png}tux.png"

##

echo -- ======== gpt4o ======== Link to an image

../command-line-tool/ailets0.py gpt4o --prompt "Describe the image." --prompt "@https://gewova.com/assets/ui-blurred.png"

##
