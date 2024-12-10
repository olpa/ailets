import os
from wasmer import Store, Module, Instance, Memory, ImportObject
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
        
        # Create memory and import object
        memory = Memory(store, min_pages=1)
        import_object = ImportObject()
        import_object.register(
            "env",
            {
                "memory": memory,
            }
        )
        
        # Instantiate the module
        self.instance = Instance(module, import_object)
        self.memory = memory

    def convert(self, messages):
        # Convert Python object to JSON string
        messages_json = json.dumps(messages)
        
        # Allocate memory for input
        input_bytes = messages_json.encode('utf-8')
        input_ptr = self._allocate(len(input_bytes))
        self._write_memory(input_ptr, input_bytes)
        
        # Call WASM function
        result_ptr = self.instance.exports.messages_to_markdown(input_ptr, len(input_bytes))
        
        # Read result
        result = self._read_string(result_ptr)
        
        return result

    def _allocate(self, size):
        return self.instance.exports.alloc(size)

    def _write_memory(self, ptr, data):
        self.memory.view[ptr:ptr + len(data)].write(data)

    def _read_string(self, ptr):
        # Read string length
        length = 0
        view = self.memory.view
        while view[ptr + length] != 0:
            length += 1
        
        # Read string data
        data = bytes(view[ptr:ptr + length])
        return data.decode('utf-8')

# Example usage:
# converter = MessagesToMarkdown()
# markdown = converter.convert([{
#     "role": "user",
#     "content": [{"type": "text", "text": "Hello"}]
# }]) 