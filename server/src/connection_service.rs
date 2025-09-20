use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use log::{debug, error, info, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio::signal::ctrl_c;
use tokio::sync::{mpsc as tokio_mpsc, oneshot};
use tokio::time::interval;

/// Message from `ConnectionService` to `EchoService`
pub struct Connection {
    pub stream: TcpStream,
    pub peer_addr: std::io::Result<SocketAddr>,
}

/// Message sent from `EchoService` to `ConnectionService` to update load.
#[derive(Debug)]
pub struct LoadUpdate {
    pub worker_id: usize,
    pub total: i32, // authoritative total active connections
}

/// Centralized load‑balancing connection service.
pub struct ConnectionService {
    worker_txs: Vec<tokio_mpsc::Sender<Connection>>,
    load_rx: tokio_mpsc::Receiver<LoadUpdate>,
    addr: SocketAddr,
    shutdown_rx: Option<oneshot::Receiver<()>>, // for tests
    bound_port: Option<Arc<AtomicU16>>,
}

impl ConnectionService {
    pub fn new(
        worker_channels: Vec<tokio_mpsc::Sender<Connection>>,
        load_receiver: tokio_mpsc::Receiver<LoadUpdate>,
        ip: &str,
        port: u16,
        bound_port: Option<Arc<AtomicU16>>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            worker_txs: worker_channels,
            load_rx: load_receiver,
            addr: format!("{}:{}", ip, port).parse()?,
            shutdown_rx: None,
            bound_port,
        })
    }

    pub fn with_shutdown(mut self, rx: oneshot::Receiver<()>) -> Self {
        debug!("ConnectionService: added shutdown channel");
        self.shutdown_rx = Some(rx);
        self
    }

    pub fn run(
        mut self,
        handle: tokio::runtime::Handle,
    ) -> tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        handle.spawn(async move { self.run_inner().await })
    }

    async fn run_inner(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(self.addr).await?;
        self.addr = listener.local_addr()?; // update addr for port=0
        info!("ConnectionService: listening on {}", self.addr);

        if let Some(port_ref) = &mut self.bound_port {
            port_ref.store(self.addr.port(), Ordering::Relaxed);
        }

        let mut load_per_worker = vec![0i32; self.worker_txs.len()];
        let mut log_interval = interval(Duration::from_secs(5));
        let mut shutdown_rx = self.shutdown_rx.take();

        loop {
            let any_capacity = self.worker_txs.iter().any(|tx| tx.capacity() > 0);

            tokio::select! {
                result = async {
                    Some(listener.accept().await)
                }, if any_capacity => {
                    if let Some(result) = result {
                        match result {
                            Ok((stream, peer_addr)) => {
                                if let Some(worker_id) = self.select_worker(&load_per_worker) {
                                    //real value will appear with time
                                    load_per_worker[worker_id] += 1; // optimistic increment
                                    let conn = Connection { stream, peer_addr: Ok(peer_addr) };
                                    if let Err(e) = self.worker_txs[worker_id].send(conn).await {
                                        error!("Send to worker {} failed: {}", worker_id, e);
                                    } else {
                                        debug!("Accepted {} → worker {}", peer_addr, worker_id);
                                    }
                                }
                            }
                            Err(e) => error!("Accept error: {}", e),
                        }
                    }
                }

                Some(update) = self.load_rx.recv() => {
                    if update.worker_id < load_per_worker.len() {
                        load_per_worker[update.worker_id] = update.total;
                        debug!("Worker {} total load = {}", update.worker_id, update.total);
                    } else {
                        error!("Invalid LoadUpdate: {:?}", update);
                    }
                }

                _ = log_interval.tick() => {
                    let loads: Vec<String> =
                        load_per_worker.iter().enumerate()
                            .map(|(id, l)| format!("w{}:{}", id, l))
                            .collect();
                    if any_capacity {
                        info!("Load status: [{}]", loads.join(", "));
                    } else {
                        warn!("SATURATION: all worker queues full. Load = [{}]", loads.join(", "));
                    }
                }

                _ = async {
                    if let Some(rx) = &mut shutdown_rx {
                        let _ = rx.await;
                    }
                }, if shutdown_rx.is_some() => {
                    info!("ConnectionService: shutdown signal received");
                    break;
                }

                _ = ctrl_c(), if shutdown_rx.is_none() => {
                    info!("ConnectionService: Ctrl+C, shutting down...");
                    break;
                }
            }
        }
        self.worker_txs.clear(); //ensure workers know we are done
        Ok(())
    }

    fn select_worker(&self, load_per_worker: &[i32]) -> Option<usize> {
        let mut target = None;
        let mut min_load = i32::MAX;
        for (id, tx) in self.worker_txs.iter().enumerate() {
            if tx.capacity() > 0 && load_per_worker[id] <= min_load {
                min_load = load_per_worker[id];
                target = Some(id);
            }
        }
        target
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }
}