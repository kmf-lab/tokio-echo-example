use server::{ConnectionService, EchoService, LoadUpdate, Connection};
use client::{CLIArgs, run};
use tokio::sync::{mpsc, oneshot};
use std::sync::{
    Arc,
    atomic::{AtomicU16, Ordering},
};
use log::{warn, LevelFilter};
use std::time::Duration;

/// End-to-end integration test: spin up server and client
/// Requires server and client both members of workspace.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_end_to_end_echo() {

    // ===== Spawn echo server on ephemeral port =====
    let bound_port = Arc::new(AtomicU16::new(0));

    let num_workers = 2;
    let channel_capacity = 100;
    let (load_tx, load_rx) = mpsc::channel::<LoadUpdate>(channel_capacity);

    let mut worker_txs = Vec::new();
    let mut handles = Vec::new();
    let runtime_handle = tokio::runtime::Handle::current();

    println!("Spawning {} workers with channel cap {}", num_workers, channel_capacity);
    for i in 0..num_workers {
        let (conn_tx, conn_rx) = mpsc::channel::<Connection>(channel_capacity);
        worker_txs.push(conn_tx);
        handles.push(
            EchoService::new(i, conn_rx, load_tx.clone()).run(runtime_handle.clone())
        );
    }
    drop(load_tx); //done with new producers

    let mut conn_svc = ConnectionService::new(
        worker_txs,
        load_rx,
        "127.0.0.1",
        0,                        // ephemeral bind
        Some(bound_port.clone()), // actual port saved
    ).expect("connection service");

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    conn_svc = conn_svc.with_shutdown(shutdown_rx);
    handles.push(conn_svc.run(runtime_handle.clone()));

    // wait for server to bind to ephemeral port
    let server_port = loop {
        let p = bound_port.load(Ordering::Relaxed);
        if p != 0 {
            break p;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };

    // ===== Run client against this ephemeral server =====
    let args = CLIArgs {
        log_level: LevelFilter::Trace,
        server_ip: "127.0.0.1".into(),
        server_port, // use ephemeral bound port
        num_clients: 2, //our test will spawn 2 clients
        message: "ping".into(),
        max_count: 50 // our test will send 50 messages from each client
    };
    println!("Running client with args: {:?}", args);

    assert!(args.server_port>0, "Server port must be > 0");
    let client_result = run(args).await; //client main entry point
    assert!(client_result.is_ok(), "Client run failed: {:?}", client_result);

    // ===== oneshot shutdown server =====
    let _ = shutdown_tx.send(());

    // join all spawned server handles cleanly
    for h in handles {
        let _ = h.await;
    }
}