import re
from typing import Any, Mapping

ALIASES = {
    "gpt4o": "gpt-4o-mini",
    "gpt": "gpt-4.1-nano",
    "chatgpt": "chatgpt-4o-latest",
    "gemini": "gemini-2.0-flash-lite",
}

OPENAI_GPT_DEFAULTS = {
    "http.url": "https://api.openai.com/v1/chat/completions",
    "http.header.Authorization": "Bearer {{secret}}",
    "ailets.model": "gpt",
}

OPENAI_GPT_MODELS = ["gpt-4o-mini", "gpt-4.1-nano", "o3", "o3-mini", "o4-mini", "chatgpt-4o-latest"]

LOCAL_DEFAULTS = {
    "http.url": "http://localhost:8000/v1/chat/completions",
    "ailets.model": "gpt",
}

LOCAL_MODELS = ["local"]

GOOGLE_DEFAULTS = {
    "http.url": "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
    "ailets.model": "gpt",
}

GOOGLE_MODELS = ["gemini-2.0-flash", "gemini-2.0-flash-lite", "gemini-1.5", "gemini-2.5"]

KEY_TO_DEFAULTS = {
    "gpt": OPENAI_GPT_DEFAULTS,
    "local": LOCAL_DEFAULTS,
    "google": GOOGLE_DEFAULTS,
}

MODEL_TO_KEY = {
    **{model: "gpt" for model in OPENAI_GPT_MODELS},
    **{model: "local" for model in LOCAL_MODELS},
    **{model: "google" for model in GOOGLE_MODELS},
}

def get_model_opts(model: str) -> Mapping[str, Any]:
    opts = None
    model = ALIASES.get(model, model)

    # Try progressively shorter versions of the model name
    try_name = model
    while try_name:
        if try_name in MODEL_TO_KEY:
            key = MODEL_TO_KEY[try_name]
            opts = KEY_TO_DEFAULTS[key].copy()
            break
        try_name = try_name[:-1]

    if opts is None:
        raise KeyError(f"No defaults found for model: {model}")
    
    opts["llm.model"] = model
    return opts

def get_wellknown_models() -> list[str]:
    return list(MODEL_TO_KEY.keys())

def get_wellknown_aliases() -> dict[str, str]:
    return ALIASES.copy()
