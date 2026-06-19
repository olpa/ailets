use dagsh::shell_ui::{parse_args, PromptArg};

fn args(v: &[&str]) -> Vec<String> {
    std::iter::once("dagsh")
        .chain(v.iter().copied())
        .map(str::to_string)
        .collect()
}

// test 1: plain text arg → PromptArg::Text with correct string
#[test]
fn test_plain_text_arg() {
    let result = parse_args(&args(&["hello"])).unwrap();
    assert_eq!(
        result.prompt_items,
        vec![PromptArg::Text("hello".to_string())]
    );
}

// test 2: @file.txt → PromptArg::File with path "file.txt" (prefix stripped)
#[test]
fn test_at_file_arg() {
    let result = parse_args(&args(&["@notes.txt"])).unwrap();
    assert_eq!(
        result.prompt_items,
        vec![PromptArg::File {
            path: "notes.txt".to_string(),
            attrs: vec![]
        }]
    );
}

// test 3: - and @- both → PromptArg::Stdin
#[test]
fn test_stdin_dash() {
    let result = parse_args(&args(&["-"])).unwrap();
    assert_eq!(result.prompt_items, vec![PromptArg::Stdin]);
}

#[test]
fn test_stdin_at_dash() {
    let result = parse_args(&args(&["@-"])).unwrap();
    assert_eq!(result.prompt_items, vec![PromptArg::Stdin]);
}

// test 4: --system-prompt "S" → PromptArg::SystemPrompt("S")
#[test]
fn test_system_prompt() {
    let result = parse_args(&args(&["--system-prompt", "Be concise"])).unwrap();
    assert_eq!(
        result.prompt_items,
        vec![PromptArg::SystemPrompt("Be concise".to_string())]
    );
}

// test 5: mixed args preserve order: --system-prompt "S" "hello" @f.txt → [SystemPrompt, Text, File]
#[test]
fn test_mixed_order_preserved() {
    let result = parse_args(&args(&["--system-prompt", "S", "hello", "@f.txt"])).unwrap();
    assert_eq!(
        result.prompt_items,
        vec![
            PromptArg::SystemPrompt("S".to_string()),
            PromptArg::Text("hello".to_string()),
            PromptArg::File {
                path: "f.txt".to_string(),
                attrs: vec![]
            },
        ]
    );
}

// test 6: --system-prompt with no following value → error
#[test]
fn test_system_prompt_missing_value() {
    let result = parse_args(&args(&["--system-prompt"]));
    assert!(result.is_err());
}

// test 7: -l script.tcl coexists with prompt args; multiple --load flags accumulate
#[test]
fn test_load_script_coexists_with_prompt_items() {
    let result = parse_args(&args(&["-l", "run.tcl", "hello"])).unwrap();
    assert_eq!(result.load_scripts, vec!["run.tcl".to_string()]);
    assert_eq!(
        result.prompt_items,
        vec![PromptArg::Text("hello".to_string())]
    );
}

#[test]
fn test_multiple_load_scripts() {
    let result = parse_args(&args(&["--load", "a.tcl", "--load", "b.tcl"])).unwrap();
    assert_eq!(
        result.load_scripts,
        vec!["a.tcl".to_string(), "b.tcl".to_string()]
    );
}

// @type=text,file=x.tcl → File with path "x.tcl" and attrs [("type","text")]
#[test]
fn test_at_arg_with_attrs() {
    let result = parse_args(&args(&["@type=text,file=x.tcl"])).unwrap();
    assert_eq!(
        result.prompt_items,
        vec![PromptArg::File {
            path: "x.tcl".to_string(),
            attrs: vec![("type".to_string(), "text".to_string())],
        }]
    );
}

// @type=image,content_type=image/png,file=photo.dat → two attrs, correct path
#[test]
fn test_at_arg_with_multiple_attrs() {
    let result = parse_args(&args(&[
        "@type=image,content_type=image/png,file=photo.dat",
    ]))
    .unwrap();
    assert_eq!(
        result.prompt_items,
        vec![PromptArg::File {
            path: "photo.dat".to_string(),
            attrs: vec![
                ("type".to_string(), "image".to_string()),
                ("content_type".to_string(), "image/png".to_string()),
            ],
        }]
    );
}

// missing file= key → error
#[test]
fn test_at_arg_missing_file_key() {
    let result = parse_args(&args(&["@type=text"]));
    assert!(result.is_err());
}
