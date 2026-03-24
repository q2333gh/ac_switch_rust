use ac_switch_rust::render_cli_markdown;
use std::env;
use std::fs;
use std::io::{self, Write};

fn main() {
    let content = render_cli_markdown();
    if let Some(path) = env::args().nth(1) {
        fs::write(path, content).expect("failed to write generated CLI markdown");
    } else {
        let mut stdout = io::stdout().lock();
        stdout
            .write_all(content.as_bytes())
            .expect("failed to write generated CLI markdown to stdout");
    }
}
