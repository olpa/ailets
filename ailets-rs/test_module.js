mod = require('./ailets-stdlib/pkg/ailets_stdlib');

mtm = mod.messages_to_markdown;

//console.log(mtm);
from = [{
         "role": "user",
         "content": [{"type": "text", "text": "Hello"}]
     }];
back = mtm(JSON.stringify(from));
console.log('back:', back);

console.log("ok so far");
