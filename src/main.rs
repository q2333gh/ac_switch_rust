use ac_switch_rust::{run_cli, Cli};
use clap::Parser;

#[cfg(target_os = "windows")]
fn main() {
    if let Err(error) = run_cli(Cli::parse()) {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("ac_switch_rust only supports Windows.");
    std::process::exit(1);
}
