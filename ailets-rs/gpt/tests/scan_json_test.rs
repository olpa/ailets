use gpt::rjiter::RJiter;
use gpt::scan_json::{scan_json, Matcher, Trigger};

#[test]
fn test_scan_json_empty_input() {
    let mut reader = std::io::empty();
    let mut buffer = vec![0u8; 32];
    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let triggers: Vec<Trigger<()>> = vec![];
    scan_json(&triggers, &mut rjiter, ());
}
