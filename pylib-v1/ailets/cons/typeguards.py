from typing import Any, Sequence, TypeGuard

from .typing import (
    ChatMessage,
    ChatMessageAssistant,
    ChatMessageContent,
    ChatMessageContentImageUrl,
    ChatMessageContentInputAudio,
    ChatMessageContentRefusal,
    ChatMessageContentText,
    ChatMessageStructuredContent,
    ChatMessageSystem,
    ChatMessageToolCall,
    ChatMessageUser,
)


def is_chat_message_content_text(obj: Any) -> TypeGuard[ChatMessageContentText]:
    return isinstance(obj, dict) and obj.get("type") == "text" and "text" in obj


def is_chat_message_content_image_url(
    obj: Any,
) -> TypeGuard[ChatMessageContentImageUrl]:
    if not isinstance(obj, dict):
        return False
    if obj.get("type") != "image_url":
        return False
    if "image_url" not in obj:
        return False
    image_url = obj["image_url"]
    if not isinstance(image_url, dict):
        return False
    return "url" in image_url


def is_chat_message_content_input_audio(
    obj: Any,
) -> TypeGuard[ChatMessageContentInputAudio]:
    return (
        isinstance(obj, dict)
        and obj.get("type") == "input_audio"
        and "input_audio" in obj
        and isinstance(obj["input_audio"], dict)
        and "data" in obj["input_audio"]
        and "format" in obj["input_audio"]
    )


def is_chat_message_content_refusal(obj: Any) -> TypeGuard[ChatMessageContentRefusal]:
    return isinstance(obj, dict) and obj.get("type") == "refusal" and "refusal" in obj


def is_chat_message_structured_content(
    obj: Any,
) -> TypeGuard[ChatMessageStructuredContent]:
    return isinstance(obj, Sequence) and all(
        is_chat_message_content_text(item)
        or is_chat_message_content_image_url(item)
        or is_chat_message_content_input_audio(item)
        or is_chat_message_content_refusal(item)
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
