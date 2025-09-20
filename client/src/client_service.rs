use std::net::SocketAddr;
use std::num::NonZero;
use log::{debug, error, info, trace, warn};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use futures::{SinkExt, StreamExt};
use tokio::signal::ctrl_c;
use tokio_util::codec::{Framed, LinesCodec};
use crate::reporting_service::ReportUpdate;

/// A single load‑test echo client worker
pub struct ClientService {
    id: usize,
    server_addr: SocketAddr,
    message: String,
    max_count: NonZero<u64>,
    shutdown_rx: Option<oneshot::Receiver<()>>,
    report_tx: mpsc::Sender<ReportUpdate>,
}

impl ClientService {
    pub fn new(
        id: usize,
        server_addr: SocketAddr,
        message: String,
        max_count: NonZero<u64>,
        report_tx: mpsc::Sender<ReportUpdate>,
    ) -> Self {
        Self {
            id,
            server_addr,
            message,
            max_count,
            shutdown_rx: None,
            report_tx,
        }
    }

    /// for testing a oneshot can be used instead of ctrl-c
    pub fn with_shutdown(mut self, rx: oneshot::Receiver<()>) -> Self {
        self.shutdown_rx = Some(rx);
        self
    }

    /// entry point for our actor
    pub fn run(
        self,
        handle: tokio::runtime::Handle,
    ) -> tokio::task::JoinHandle<Result<u64, Box<dyn std::error::Error + Send + Sync>>> {
        handle.spawn(self.run_inner())
    }

    async fn run_inner(
        mut self,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        trace!("Client {} starting run_inner...", self.id);

        let stream = TcpStream::connect(self.server_addr).await?;
        let peer = stream.peer_addr().unwrap_or(self.server_addr);
        debug!("Client {} connected to {}", self.id, peer);

        let mut shutdown_rx = self.shutdown_rx.take();
        let mut framed = Framed::new(stream, LinesCodec::new());
        let mut sent: u64 = 0;

        loop {
            trace!("Client {} loop top; sent so far={}", self.id, sent);

            // Construct a future that sends one message and yields `framed` back.
            // This extra step is required because `framed` must be moved into
            // the async block and returned — we can't just inline it in select!
            // since async closures aren't stable yet.
            let io_step = {
                let msg = self.message.clone();
                Box::pin(async move {
                    framed.send(msg.clone()).await?;
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(framed)
                })
            };

            tokio::select! {
                _ = async {
                    if let Some(rx) = &mut shutdown_rx {
                        let _ = rx.await;
                    }
                }, if shutdown_rx.is_some() => {
                    info!("Client {} received shutdown signal", self.id);
                    break;
                }

                _ = ctrl_c(), if shutdown_rx.is_none() => {
                    info!("Client {} shutting down on Ctrl+C", self.id);
                    break;
                }

                result = io_step => {
                    // io_step only handled the send, now handle receive inline
                    match result {
                        Ok(mut fr) => {
                            sent += 1;
                            debug!("Client {} send #{} succeeded", self.id, sent);

                            match fr.next().await {
                                Some(Ok(reply)) => {
                                    if reply != self.message {
                                        error!("Client {} mismatch: sent '{:?}', got '{:?}'", self.id, self.message, reply);
                                        break; //exit early
                                    }
                                    debug!("Client {} received echo for message #{}: {:?}", self.id, sent, reply);
                                        match self.report_tx.send(ReportUpdate { count: 1 }).await {
                                            Ok(_) => {}
                                            Err(_) => {
                                                trace!("Client {} reporter channel closed → shutting down", self.id);
                                                break;
                                            }
                                        }
                                }
                                Some(Err(e)) => {
                                    error!("Client {} receive error on message #{}: {}", self.id, sent, e);
                                    break;
                                }
                                None => {
                                    warn!("Client {} server closed connection after message #{}", self.id, sent);
                                    break;
                                }
                            }

                            if sent >= self.max_count.get() {
                                info!("Client {} reached max count {}", self.id, self.max_count);
                                break;
                            }

                            framed = fr; // carry forward
                        }
                        Err(e) => {
                            error!("Client {} send error: {}", self.id, e);
                            break;
                        }
                    }
                }
            }
        }

        debug!("Client {} terminating, total sent={}", self.id, sent);
        trace!("Client {} exiting run_inner...", self.id);
        Ok(sent)
    }
}