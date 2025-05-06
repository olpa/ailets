use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;

#[derive(Deserialize, Debug)]
pub struct EnvOpts {
    opts: HashMap<String, serde_json::Value>,
}

impl EnvOpts {
    pub fn envopts_from_reader(reader: impl Read) -> Result<EnvOpts, Box<dyn std::error::Error>> {
        let mut de = serde_json::Deserializer::from_reader(reader);
        let opts = EnvOpts::deserialize(&mut de)?;
        Ok(opts)
    }
}
