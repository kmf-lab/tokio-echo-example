use clap::Parser;
use client::CLIArgs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Strong-typed command-line arguments
    client::run(CLIArgs::parse()).await //separate method for easier testing
}