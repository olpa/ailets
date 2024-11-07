import json
import sys


(_, in_file, out_file) = sys.argv

url = "https://api.openai.com/v1/chat/completions"


with open(in_file) as h:
    prompt = json.load(h)

auth_key = {
    "actor": "auth-key",
    "outputStreamName": f"auth-{url}",
    "inputStream": {
        "url": url,
        "outputPrefix": "Bearer ",
    },
}

query = {
    "$deps": [auth_key],
    "url": url,
    "method": "POST",
    "headers": {
        "Content-type": "application/json",
        "Authorization": {"$stream": f"auth-{url}"},
    },
    "body": {"model": "gpt-3.5-turbo", "messages": prompt},
}

with open(out_file, "w") as h:
    json.dump(query, h, indent=2)
