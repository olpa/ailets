from typing import Any, TypeGuard

from ailets.atyping import (
    ContentItem,
    ContentItemCtl,
    ContentItemFunction,
    ContentItemImage,
    ContentItemRefusal,
    ContentItemText,
    ContentItemToolSpec,
)


def is_content_item_text(obj: Any) -> TypeGuard[ContentItemText]:
    if not isinstance(obj, list):
        return False
    if len(obj) != 2:
        return False
    (obj0, obj1) = obj
    if not isinstance(obj0, dict) or not isinstance(obj1, dict):
        return False
    if obj0.get("type") != "text":
        return False
    if not isinstance(obj1, dict):
        return False
    return obj1.get("text") is not None


def is_content_item_refusal(obj: Any) -> TypeGuard[ContentItemRefusal]:
    if not isinstance(obj, list):
        return False
    if len(obj) != 2:
        return False
    (obj0, obj1) = obj
    if not isinstance(obj0, dict) or not isinstance(obj1, dict):
        return False
    if obj0.get("type") != "refusal":
        return False
    return obj1.get("refusal") is not None


def is_content_item_image(obj: Any) -> TypeGuard[ContentItemImage]:
    if not isinstance(obj, list):
        return False
    if len(obj) != 2:
        return False
    (obj0, obj1) = obj
    if not isinstance(obj0, dict) or not isinstance(obj1, dict):
        return False
    if obj0.get("type") != "image":
        return False
    return "image_url" in obj1 or "image_key" in obj1


def is_content_item_function(obj: Any) -> TypeGuard[ContentItemFunction]:
    if not isinstance(obj, list):
        return False
    if len(obj) != 2:
        return False
    (obj0, obj1) = obj
    if not isinstance(obj0, dict) or not isinstance(obj1, dict):
        return False
    if obj0.get("type") != "function":
        return False
    return "id" in obj1 and "function" in obj1


def is_content_item_ctl(obj: Any) -> TypeGuard[ContentItemCtl]:
    if not isinstance(obj, list):
        return False
    if len(obj) != 2:
        return False
    (obj0, obj1) = obj
    if not isinstance(obj0, dict) or not isinstance(obj1, dict):
        return False
    if obj0.get("type") != "ctl":
        return False
    return isinstance(obj1.get("role"), str)


def is_content_item_toolspec(obj: Any) -> TypeGuard[ContentItemToolSpec]:
    if not isinstance(obj, list):
        return False
    if len(obj) != 2:
        return False
    (obj0, obj1) = obj
    if not isinstance(obj0, dict) or not isinstance(obj1, dict):
        return False
    if obj0.get("type") != "toolspec":
        return False
    return "toolspec_key" in obj1 or "toolspec" in obj1


def is_content_item(obj: Any) -> TypeGuard[ContentItem]:
    return (
        is_content_item_text(obj)
        or is_content_item_refusal(obj)
        or is_content_item_image(obj)
        or is_content_item_function(obj)
        or is_content_item_ctl(obj)
        or is_content_item_toolspec(obj)
    )
