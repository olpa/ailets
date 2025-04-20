from typing import Any, Literal, Sequence, TypedDict, Union
from typing_extensions import NotRequired

from ailets.atyping import ContentItemFunction, ContentItemRefusal, ContentItemText


class Gpt4oImageUrl(TypedDict):
    url: str
    detail: NotRequired[str]


class Gpt4oImage(TypedDict):
    type: Literal["image_url"]
    image_url: Gpt4oImageUrl


Gpt4oContentItem = Union[Gpt4oImage, ContentItemText, ContentItemRefusal]


class Gpt4oMessage(TypedDict):
    role: str
    content: Any
    tool_calls: NotRequired[Sequence[ContentItemFunction]]


def is_gpt4o_image(obj: Any) -> bool:
    if not isinstance(obj, dict):
        return False
    if obj.get("type") != "image_url":
        return False
    if not isinstance(obj.get("image_url"), dict):
        return False
    if not isinstance(obj["image_url"].get("url"), str):
        return False
    return True
