import json
from ailets.cons.typing import INodeRuntime


def response_to_image(runtime: INodeRuntime) -> None:
    """Convert DALL-E response to image."""

    output = runtime.open_write(None)

    for i in range(runtime.n_of_streams(None)):
        response = json.loads(runtime.open_read(None, i).read())
        image_url = response["data"][0]["url"]
        revised_prompt = response["data"][0].get("revised_prompt", "")
        
        if revised_prompt:
            output.write(f"*Revised prompt: {revised_prompt}*\n\n")
        
        output.write(f"![DALL-E generated image]({image_url})")

    runtime.close_write(None) 