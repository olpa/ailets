from typing import Any, Literal, Sequence, TypedDict, Union
from typing_extensions import NotRequired

from ailets.atyping import ContentItemFunction, ContentItemRefusal, ContentItemText


class GptImageUrl(TypedDict):
    url: str
    detail: NotRequired[str]


class GptImage(TypedDict):
    type: Literal["image_url"]
    image_url: GptImageUrl


GptContentItem = Union[GptImage, ContentItemText, ContentItemRefusal]


class GptMessage(TypedDict):
    role: str
    content: Any
    tool_calls: NotRequired[Sequence[ContentItemFunction]]


def is_gpt_image(obj: Any) -> bool:
    if not isinstance(obj, dict):
        return False
    if obj.get("type") != "image_url":
        return False
    if not isinstance(obj.get("image_url"), dict):
        return False
    if not isinstance(obj["image_url"].get("url"), str):
        return False
    return True
