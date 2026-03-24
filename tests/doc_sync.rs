use ac_switch_rust::render_cli_markdown;
use std::fs;
use std::path::Path;

#[test]
fn generated_cli_markdown_is_in_sync() {
    let expected = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("doc/cli.md"))
        .expect("tracked CLI reference should exist");
    let actual = render_cli_markdown();

    assert_eq!(normalize(&actual), normalize(&expected));
}

fn normalize(value: &str) -> String {
    value
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .trim()
        .to_owned()
}
