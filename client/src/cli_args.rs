use clap::Parser;
use log::LevelFilter;

/// Command-line args for the load-test client
#[derive(Parser, Debug, PartialEq, Clone)]
pub struct CLIArgs {
    /// Log Level
    #[arg(short = 'l', long = "log_level", value_enum, default_value = "INFO")]
    pub log_level: LevelFilter,

    /// Server IP to connect to
    #[arg(long = "server-ip", default_value = "127.0.0.1")]
    pub server_ip: String,

    /// Server port to connect to
    #[arg(long = "server-port", default_value_t = 9000,
          value_parser = clap::value_parser!(u16).range(1..)
    )]
    pub server_port: u16,

    /// Number of parallel clients to spawn
    #[arg(long = "num-clients", default_value_t = 1)]
    pub num_clients: usize,

    /// Message payload to echo
    #[arg(long = "message", default_value = "hello world")]
    pub message: String,

    /// Maximum number of echoes across all clients before shutdown
    #[arg(long = "max-count", default_value_t = 1,
          value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub max_count: u64,
}