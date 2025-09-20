use server::run;
use tokio::sync::oneshot;
use tokio_util::codec::{Framed, LinesCodec};
use futures::{SinkExt, StreamExt};
use log::info;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use server::cli_args::CLIArgs;

/// A full multi-round echo test against the server running via `main_internal`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_echo_three_round_trips() {
    // prepare shutdown channel
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    // atomic to hold the ephemeral bound port
    let port_holder = Arc::new(AtomicU16::new(0));

    // CLI arguments for test isolation
    let cli_args = CLIArgs {
        log_level: log::LevelFilter::Debug,
        ip: "127.0.0.1".into(),
        port: 0, // 👈 use ephemeral port
        num_workers: 1,
        channel_capacity: 10,
    };

    // start server as background task
    let server_task = {
        let port_holder_clone = port_holder.clone();
        tokio::spawn(run(cli_args.clone(), Some(shutdown_rx), Some(port_holder_clone)))
    };

    let bound_port = loop {
        let p = port_holder.load(Ordering::Relaxed);
        if p != 0 {
            break p;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };


    let addr = format!("{}:{}", cli_args.ip, bound_port);

    info!("Connecting to server at {addr}");
    let stream = tokio::net::TcpStream::connect(&addr)
        .await
        .expect("connect to echo server");

    let mut framed = Framed::new(stream, LinesCodec::new());

    // messages we want to round-trip
    let messages = vec![
        "hello world #1",
        "hello world #2",
        "hello world #3",
    ];

    for msg in &messages {
        info!("Sending test line: {msg}");
        framed
            .send(msg.to_string())
            .await
            .expect("send test line");

        let response = framed
            .next()
            .await
            .expect("response should arrive")
            .expect("valid codec frame");

        assert_eq!(response, *msg, "echoed line should match input");
    }

    info!("Closing client stream...");
    drop(framed);

    info!("Sending shutdown signal...");
    let _ = shutdown_tx.send(());

    info!("Awaiting server shutdown...");
    server_task
        .await
        .expect("server task join")
        .expect("server result ok");
}