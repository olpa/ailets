import os
from wasmer import Store, Module, Instance, ImportObject, Memory
import json

class MessagesToMarkdown:
    def __init__(self):
        store = Store()
        
        # Load the WASM module
        current_dir = os.path.dirname(os.path.abspath(__file__))
        wasm_path = os.path.join(current_dir, "ailets_stdlib.wasm")
        
        with open(wasm_path, "rb") as f:
            wasm_bytes = f.read()
            
        module = Module(store, wasm_bytes)
        
        # Create an import object
        import_object = ImportObject()
        
        # Instantiate the module
        self.instance = Instance(module, import_object)

    def convert(self, messages):
        # Convert Python object to JSON string
        messages_json = json.dumps(messages)
        
        # Call the WASM function
        result = self.instance.exports.messages_to_markdown(messages_json)
        
        return result

# Example usage:
# converter = MessagesToMarkdown()
# markdown = converter.convert([{
#     "role": "user",
#     "content": [{"type": "text", "text": "Hello"}]
# }]) 