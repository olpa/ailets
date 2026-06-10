use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, LazyBuffers, NextTimeout, Transport,
};

#[derive(Debug)]
struct FakeTransport {
    buffers: LazyBuffers,
    response: Vec<u8>,
    pos: usize,
}

impl Transport for FakeTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(
        &mut self,
        _amount: usize,
        _timeout: NextTimeout,
    ) -> Result<(), ureq::Error> {
        Ok(())
    }

    fn await_input(&mut self, _timeout: NextTimeout) -> Result<bool, ureq::Error> {
        if self.pos >= self.response.len() {
            return Ok(false);
        }
        let input = self.buffers.input_append_buf();
        let remaining = &self.response[self.pos..];
        let n = remaining.len().min(input.len());
        input[..n].copy_from_slice(&remaining[..n]);
        self.buffers.input_appended(n);
        self.pos += n;
        Ok(n > 0)
    }

    fn is_open(&mut self) -> bool {
        true
    }
}

#[derive(Debug)]
struct FakeConnector {
    response: Vec<u8>,
}

impl<In: Transport> Connector<In> for FakeConnector {
    type Out = FakeTransport;

    fn connect(
        &self,
        details: &ConnectionDetails,
        _chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(Some(FakeTransport {
            buffers: LazyBuffers::new(
                details.config.input_buffer_size(),
                details.config.output_buffer_size(),
            ),
            response: self.response.clone(),
            pos: 0,
        }))
    }
}

#[test]
fn happy_path() {
    let fake_response = b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, world!".to_vec();
    let agent = ureq::Agent::with_parts(
        ureq::config::Config::default(),
        FakeConnector {
            response: fake_response,
        },
        DefaultResolver::default(),
    );

    let spec = serde_json::json!({
        "url": "http://127.0.0.1/v1/chat",
        "method": "POST",
        "headers": {
            "Content-Type": "application/json",
            "Authorization": "Bearer test-token"
        },
        "body": { "model": "test-model", "messages": [] }
    });

    let reader = spec.to_string();
    let mut output = Vec::new();

    query::execute_impl(reader.as_bytes(), &mut output, &agent).expect("execute should succeed");

    assert_eq!(String::from_utf8(output).unwrap(), "Hello, world!");
}

#[test]
fn http_error_status() {
    let fake_response =
        b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 12\r\n\r\nAccess denied".to_vec();
    let agent = ureq::Agent::with_parts(
        ureq::config::Config::default(),
        FakeConnector {
            response: fake_response,
        },
        DefaultResolver::default(),
    );

    let spec = serde_json::json!({
        "url": "http://127.0.0.1/v1/chat",
        "method": "POST",
        "headers": {},
        "body": {}
    });

    let reader = spec.to_string();
    let mut output = Vec::new();

    let result = query::execute_impl(reader.as_bytes(), &mut output, &agent);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("401"),
        "error should mention status code, got: {err}"
    );
}
