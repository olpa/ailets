from typing import Literal, Sequence, TypedDict, Union, NotRequired

from ailets.cons.typing import (
    ContentItemRefusal,
    ContentItemText,
)


class Gpt4oImageUrl(TypedDict):
    url: str
    detail: NotRequired[str]


class Gpt4oImage(TypedDict):
    type: Literal["image_url"]
    image_url: Gpt4oImageUrl


class ChatAssistantToolCall(TypedDict):
    type: Literal["function"]
    id: str
    function: dict[Literal["name", "arguments"], str]


Gpt4oChatMessageContentItem = Union[
    ContentItemText,
    ContentItemRefusal,
    Gpt4oImage,
]


Gpt4oChatMessageContent = Union[
    str,
    Sequence[Gpt4oChatMessageContentItem],
]
