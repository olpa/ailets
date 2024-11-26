from typing import Literal, TypedDict, Union, NotRequired

from ailets.cons.typing import ChatMessageStructuredContentItem


class Gpt4oImageUrl(TypedDict):
    url: str
    detail: NotRequired[str]


class Gpt4oImage(TypedDict):
    type: Literal["image_url"]
    image_url: Gpt4oImageUrl


Gpt4oChatMessageContentItem = Union[
    ChatMessageStructuredContentItem,
    Gpt4oImage,
]
