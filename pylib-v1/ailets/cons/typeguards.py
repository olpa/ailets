from typing import Any, Sequence, TypeGuard

from .atyping import (
    Content,
    ContentItem,
    ContentItemFunction,
    ContentItemImage,
    ContentItemRefusal,
    ContentItemText,
)


def is_content_item_text(obj: Any) -> TypeGuard[ContentItemText]:
    return isinstance(obj, dict) and obj.get("type") == "text" and "text" in obj


def is_content_item_refusal(obj: Any) -> TypeGuard[ContentItemRefusal]:
    return isinstance(obj, dict) and obj.get("type") == "refusal" and "refusal" in obj


def is_content_item_image(obj: Any) -> TypeGuard[ContentItemImage]:
    return (
        isinstance(obj, dict)
        and obj.get("type") == "image"
        and isinstance(obj.get("content_type"), str)
        and ("url" in obj or "key" in obj)
    )


def is_content_item_function(obj: Any) -> TypeGuard[ContentItemFunction]:
    return (
        isinstance(obj, dict)
        and obj.get("type") == "function"
        and "id" in obj
        and "function" in obj
    )


def is_content_item(obj: Any) -> TypeGuard[ContentItem]:
    return (
        is_content_item_text(obj)
        or is_content_item_refusal(obj)
        or is_content_item_image(obj)
        or is_content_item_function(obj)
    )


def is_chat_message_content(obj: Any) -> TypeGuard[Content]:
    return isinstance(obj, Sequence) and all(is_content_item(item) for item in obj)
