import re
from typing import Any, Mapping

OPENAI_GPT_DEFAULTS = {
    "http.url": "https://api.openai.com/v1/chat/completions",
    "http.header.Authorization": "Bearer {{secret}}",
    "ailets.model": "gpt4o",
}

OPENAI_GPT_MODELS = ["gpt-4.1-nano"]

KEY_TO_DEFAULTS = {
    "gpt": OPENAI_GPT_DEFAULTS,
}

MODEL_TO_KEY = {
    **{model: "gpt" for model in OPENAI_GPT_MODELS},
}

def get_model_opts(model: str) -> Mapping[str, Any]:
    opts = None

    # Try progressively shorter versions of the model name
    try_name = model
    while try_name:
        if try_name in MODEL_TO_KEY:
            key = MODEL_TO_KEY[try_name]
            opts = KEY_TO_DEFAULTS[key].copy()
            break
        try_name = try_name[:-1]

    if opts is None:
        raise ValueError(f"No defaults found for model: {model}")
    
    opts["llm.model"] = model
    return opts
