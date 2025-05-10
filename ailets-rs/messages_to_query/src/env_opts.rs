use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;

#[derive(Deserialize, Debug)]
pub struct EnvOpts {
    opts: HashMap<String, serde_json::Value>,
}

impl EnvOpts {
    /// Creates a new `EnvOpts` instance by reading JSON from a reader.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The JSON input is invalid or malformed
    /// - There are I/O errors reading from the provided reader
    pub fn envopts_from_reader(reader: impl Read) -> Result<EnvOpts, Box<dyn std::error::Error>> {
        let mut de = serde_json::Deserializer::from_reader(reader);
        let opts_map = HashMap::<String, serde_json::Value>::deserialize(&mut de)?;
        Ok(EnvOpts { opts: opts_map })
    }

    #[must_use]
    pub fn from_map(opts: HashMap<String, serde_json::Value>) -> EnvOpts {
        EnvOpts { opts }
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.opts.get(key)
    }

    #[must_use]
    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, String, serde_json::Value> {
        self.opts.iter()
    }
}

impl<'a> IntoIterator for &'a EnvOpts {
    type Item = (&'a String, &'a serde_json::Value);
    type IntoIter = std::collections::hash_map::Iter<'a, String, serde_json::Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.opts.iter()
    }
}
