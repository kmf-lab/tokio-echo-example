
pub mod connection_service;
pub mod echo_service;
pub mod cli_args;

pub use connection_service::{Connection, ConnectionService, LoadUpdate};
pub use echo_service::EchoService;

#[allow(unused_imports)]
use log::*;
use tokio::sync::{mpsc, oneshot};
use flexi_logger::{opt_format, Logger, WriteMode};
use cli_args::CLIArgs;
use std::sync::Arc;
use std::sync::atomic::AtomicU16;

/// Testable internal entry point
pub async fn run(
    cli_args: CLIArgs,
    shutdown_rx: Option<oneshot::Receiver<()>>,
    bound_port: Option<Arc<AtomicU16>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    // initialize logging (logger must only be started once)
    if let Err(e) = Logger::with(cli_args.log_level)
        .format(opt_format) // includes timestamp, level, logger name
        .write_mode(WriteMode::Direct)
        .start() {
        eprintln!("Error initializing logger: {}", e);
    }

    info!("Starting up...");
    let runtime_handle = tokio::runtime::Handle::current();

    let mut all_handles = Vec::new();
    let (load_tx, load_rx) =
        mpsc::channel::<LoadUpdate>(cli_args.channel_capacity);

    // spawn workers
    let mut worker_txs = Vec::new();

    for i in 0..cli_args.num_workers {
        let (conn_tx, conn_rx) =
            mpsc::channel::<Connection>(cli_args.channel_capacity);
        worker_txs.push(conn_tx);
        all_handles.push(
            EchoService::new(i, conn_rx, load_tx.clone()).run(runtime_handle.clone()),
        );
    }
    drop(load_tx); // no more clones

    // spawn connection service
    let conn_svc = ConnectionService::new(
        worker_txs,
        load_rx,
        &cli_args.ip,
        cli_args.port,
        bound_port,
    )?;
    
    // added for testing to we can inject a shutdown channel
    let conn_svc = if let Some(rx) = shutdown_rx {
        conn_svc.with_shutdown(rx)
    } else {
        conn_svc
    };

    // spawn connection service
    all_handles.push(conn_svc.run(runtime_handle.clone()));

    info!(
        "Running {} workers with channel cap {} on {}:{} ...",
        cli_args.num_workers, cli_args.channel_capacity, cli_args.ip, cli_args.port
    );

    // wait for all services to finish
    for h in all_handles {
        h.await??;
    }

    trace!("All services shut down");
    Ok(())
}