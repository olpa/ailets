from typing import Any, TypeGuard

from ailets.atyping import (
    ContentItem,
    ContentItemFunction,
    ContentItemImage,
    ContentItemRefusal,
    ContentItemText,
)


def is_content_item_text(obj: Any) -> TypeGuard[ContentItemText]:
    if not isinstance(obj, list):
        return False
    if len(obj) != 2:
        return False
    if obj[0].get("type") != "text":
        return False
    return obj[1].get("text") is not None


def is_content_item_refusal(obj: Any) -> TypeGuard[ContentItemRefusal]:
    if not isinstance(obj, list):
        return False
    if len(obj) != 2:
        return False
    if obj[0].get("type") != "refusal":
        return False
    return obj[1].get("refusal") is not None


def is_content_item_image(obj: Any) -> TypeGuard[ContentItemImage]:
    if not isinstance(obj, list):
        return False
    if len(obj) != 2:
        return False
    if obj[0].get("type") != "image":
        return False
    return isinstance(obj[1].get("content_type"), str) and (
        "image_url" in obj[1] or "image_key" in obj[1]
    )


def is_content_item_function(obj: Any) -> TypeGuard[ContentItemFunction]:
    if not isinstance(obj, list):
        return False
    if len(obj) != 2:
        return False
    if obj[0].get("type") != "function":
        return False
    return "id" in obj[1] and "function" in obj[1]


def is_content_item(obj: Any) -> TypeGuard[ContentItem]:
    return (
        is_content_item_text(obj)
        or is_content_item_refusal(obj)
        or is_content_item_image(obj)
        or is_content_item_function(obj)
    )
