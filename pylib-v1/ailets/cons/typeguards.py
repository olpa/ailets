from typing import Any, Sequence, TypeGuard

from .typing import (
    ChatMessage,
    ChatMessageAssistant,
    ChatMessageContent,
    ContentItemRefusal,
    ContentItemText,
    Content,
    ChatMessageSystem,
    ChatMessageToolCall,
    ChatMessageUser,
    ContentItemImage,
)


def is_chat_message_content_text(obj: Any) -> TypeGuard[ContentItemText]:
    return isinstance(obj, dict) and obj.get("type") == "text" and "text" in obj


def is_chat_message_content_refusal(obj: Any) -> TypeGuard[ContentItemRefusal]:
    return isinstance(obj, dict) and obj.get("type") == "refusal" and "refusal" in obj


def is_chat_message_content_image(obj: Any) -> TypeGuard[ContentItemImage]:
    return (
        isinstance(obj, dict)
        and obj.get("type") == "image"
        and isinstance(obj.get("content_type"), str)
        and ("url" in obj or "stream" in obj)
    )


def is_chat_message_structured_content(
    obj: Any,
) -> TypeGuard[Content]:
    return isinstance(obj, Sequence) and all(
        is_chat_message_content_text(item)
        or is_chat_message_content_refusal(item)
        or is_chat_message_content_image(item)
        for item in obj
    )


def is_chat_message_content(obj: Any) -> TypeGuard[ChatMessageContent]:
    return isinstance(obj, str) or is_chat_message_structured_content(obj)


def is_chat_message_system(obj: Any) -> TypeGuard[ChatMessageSystem]:
    return (
        isinstance(obj, dict)
        and obj.get("role") == "system"
        and isinstance(obj.get("content"), str)
    )


def is_chat_message_user(obj: Any) -> TypeGuard[ChatMessageUser]:
    return (
        isinstance(obj, dict)
        and obj.get("role") == "user"
        and is_chat_message_content(obj.get("content"))
    )


def is_chat_message_assistant(obj: Any) -> TypeGuard[ChatMessageAssistant]:
    return (
        isinstance(obj, dict)
        and obj.get("role") == "assistant"
        and (obj.get("content") is None or is_chat_message_content(obj.get("content")))
        and (obj.get("refusal") is None or isinstance(obj.get("refusal"), str))
    )


def is_chat_message_tool_call(obj: Any) -> TypeGuard[ChatMessageToolCall]:
    return (
        isinstance(obj, dict)
        and obj.get("role") == "tool"
        and isinstance(obj.get("tool_call_id"), str)
        and is_chat_message_content(obj.get("content"))
    )


def is_chat_message(obj: Any) -> TypeGuard[ChatMessage]:
    return (
        is_chat_message_system(obj)
        or is_chat_message_user(obj)
        or is_chat_message_assistant(obj)
        or is_chat_message_tool_call(obj)
    )
