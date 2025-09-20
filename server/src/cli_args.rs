use clap::Parser;
use log::LevelFilter;

/// Command line args
#[derive(Parser, Debug, PartialEq, Clone)]
pub struct CLIArgs {
    #[arg(short = 'l', long = "log_level", value_enum, default_value = "INFO")]
    pub log_level: LevelFilter,

    #[arg(long = "ip", default_value = "127.0.0.1")]
    pub ip: String,

    #[arg(long = "port", default_value_t = 9000)]
    pub port: u16,

    /// Number of worker threads (defaults to number of logical CPUs)
    #[arg(long = "num-workers", default_value_t = num_cpus::get())]
    pub num_workers: usize,

    #[arg(long = "channel-capacity", default_value_t = 100)]
    pub channel_capacity: usize,
}