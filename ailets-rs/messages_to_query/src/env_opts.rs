use serde::Deserialize;
use std::collections::HashMap;

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
    pub fn envopts_from_reader(mut reader: impl embedded_io::Read) -> Result<EnvOpts, String> {
        // Read all data into a Vec<u8>
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            match embedded_io::Read::read(&mut reader, &mut chunk) {
                Ok(0) => break,
                Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                Err(e) => return Err(format!("Failed to read env opts: {e:?}")),
            }
        }

        // Deserialize from the string
        let opts_map: HashMap<String, serde_json::Value> = serde_json::from_slice(&buffer)
            .map_err(|e| format!("Failed to parse env opts JSON: {e}"))?;
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
