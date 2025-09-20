
use clap::Parser;
use server::cli_args::CLIArgs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    server::run(CLIArgs::parse(), None, None).await //separate method for easier testing
}