from typing import Literal, Sequence, TypedDict, Union, NotRequired

from ailets.cons.typing import (
    ChatMessageContentPlainText,
    ChatMessageContentRefusal,
    ChatMessageContentText,
)


class Gpt4oImageUrl(TypedDict):
    url: str
    detail: NotRequired[str]


class Gpt4oImage(TypedDict):
    type: Literal["image_url"]
    image_url: Gpt4oImageUrl


Gpt4oChatMessageContentItem = Union[
    ChatMessageContentText,
    ChatMessageContentRefusal,
    Gpt4oImage,
]

Gpt4oChatMessageContent = Union[
    ChatMessageContentPlainText,
    Sequence[Gpt4oChatMessageContentItem],
]
