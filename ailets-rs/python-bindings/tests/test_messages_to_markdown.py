import pytest
from ailets_rs import MessagesToMarkdown

def test_basic_conversion():
    converter = MessagesToMarkdown()
    messages = [{
        "role": "user",
        "content": [{"type": "text", "text": "Hello"}]
    }]
    
    result = converter.convert(messages)
    
    assert "**user**" in result
    assert "Hello" in result 