pub mod client_service;
pub mod reporting_service;
pub mod cli_args;

use std::net::SocketAddr;
use std::num::NonZero;
use log::*;
use flexi_logger::{opt_format, Logger, WriteMode};
use tokio::sync::mpsc;

pub use client_service::ClientService;
pub use reporting_service::{ReportUpdate, ReportingService};
pub use crate::cli_args::CLIArgs;

/// Main entry point and graph construction logic.
pub async fn run(
    cli_args: CLIArgs
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    info!("Starting echo load-test client...");
    info!(
        "num_clients={} server={} port={} message='{}' max_count={:?}",
        cli_args.num_clients,
        cli_args.server_ip,
        cli_args.server_port,
        cli_args.message,
        cli_args.max_count,
    );

    // Parse server address
    let server_addr: SocketAddr = format!("{}:{}", cli_args.server_ip, cli_args.server_port).parse()?;


    // initialize logging (logger must only be started once)
    if let Err(e) = Logger::with(cli_args.log_level)
        .format(opt_format) // includes timestamp, level, logger name
        .write_mode(WriteMode::Direct)
        .start() {
        eprintln!("Error initializing logger: {}", e);
    }

    info!("Starting up...");
    let runtime_handle = tokio::runtime::Handle::current();

    // Channel for reporting updates
    let (report_tx, report_rx) = mpsc::channel::<ReportUpdate>(100);

    // Spawn reporting service
    let report_hdl = ReportingService::new(report_rx, cli_args.max_count*cli_args.num_clients as u64).run(runtime_handle.clone());

    // Spawn N clients
    info!("Connecting to server at {}", server_addr);
    let mut handles = Vec::new();
    for id in 0..cli_args.num_clients {
        let client = ClientService::new(
            id,
            server_addr,
            cli_args.message.clone(),
            NonZero::new(cli_args.max_count).expect("must be nonzero"),
            report_tx.clone(), // multiple producers
        );
        handles.push(client.run(runtime_handle.clone()));
    }
    drop(report_tx); // close channel when clients finish

    info!(
        "Started {} clients sending '{}' to {}:{}",
        cli_args.num_clients,
        cli_args.message,
        cli_args.server_ip,
        cli_args.server_port
    );

    // gather totals from client join handles
    let mut total: u64 = 0;
    for h in handles {
        match h.await {
            Ok(Ok(count)) => total += count,
            Ok(Err(e)) => error!("Client task error: {}", e),
            Err(join_err) => error!("Join error: {}", join_err),
        }
    }

    // exit when reporting_service finishes
    let final_total = report_hdl.await??;

    println!("Final total echoes sent = {}", final_total.max(total) );

    if final_total >= cli_args.max_count*cli_args.num_clients as u64 {
        Ok(())
    } else {
        //happens when we exit early due to echo mismatches
        Err("Client task error: not all clients reached max_count".into())
    }
}