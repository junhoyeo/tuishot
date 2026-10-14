use clap::Parser;
fn main() {
    if let Err(error) = ttyshot::execute(ttyshot::cli::Cli::parse()) {
        eprintln!("ttyshot: {error}");
        std::process::exit(1);
    }
}
