use clap::Parser;
fn main() {
    if let Err(error) = tuishot::execute(tuishot::cli::Cli::parse()) {
        eprintln!("tuishot: {error}");
        std::process::exit(1);
    }
}
