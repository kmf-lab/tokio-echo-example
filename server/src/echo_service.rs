use crate::connection_service::{Connection, LoadUpdate};
use futures::stream::{FuturesUnordered, StreamExt, StreamFuture};
use futures::SinkExt;
use log::{debug, info, error, trace};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::mpsc as tokio_mpsc;
use tokio_util::codec::{Framed, LinesCodec};
use ahash::AHashMap;

type FramedStream = Framed<TcpStream, LinesCodec>;

pub struct EchoService {
    worker_id: usize,
    connection_rx: tokio_mpsc::Receiver<Connection>,
    load_update_tx: tokio_mpsc::Sender<LoadUpdate>,
    connected_streams: FuturesUnordered<StreamFuture<FramedStream>>,
    addr_map: AHashMap<SocketAddr, ()>,
}

impl EchoService {
    pub fn new(
        worker_id: usize,
        connection_rx: tokio_mpsc::Receiver<Connection>,
        load_update_tx: tokio_mpsc::Sender<LoadUpdate>,
    ) -> Self {
        Self {
            worker_id,
            connection_rx,
            load_update_tx,
            connected_streams: FuturesUnordered::new(),
            addr_map: AHashMap::new(),
        }
    }

    // entry point to this actor
    pub fn run(
        self,
        handle: tokio::runtime::Handle,
    ) -> tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        handle.spawn(self.run_inner())
    }

    async fn run_inner(mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("EchoService {} started", self.worker_id);

        loop {
            tokio::select! {
                Some(conn) = self.connection_rx.recv() => {
                    self.on_new_connection(conn).await?;
                }
                Some((result, framed)) = self.connected_streams.next() => {
                    self.on_item_event(result, framed).await?;
                }
                else => {
                    if self.connection_rx.is_closed() {
                        info!("EchoService {} shutting down", self.worker_id);
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    async fn on_new_connection(
        &mut self,
        conn: Connection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let peer = match conn.peer_addr {
            Ok(a) => a,
            Err(e) => {
                debug!("Worker {} got conn without peer addr: {}", self.worker_id, e);
                return Ok(());
            }
        };

        self.addr_map.insert(peer, ());
        debug!("Worker {} accepted new connection from {}", self.worker_id, peer);

        // send authoritative total
        self.send_load_update().await;

        let framed = Framed::new(conn.stream, LinesCodec::new());
        self.connected_streams.push(framed.into_future());
        Ok(())
    }

    async fn on_item_event(
        &mut self,
        result: Option<Result<String, tokio_util::codec::LinesCodecError>>,
        mut framed: FramedStream,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let peer = framed
            .get_ref()
            .peer_addr()
            .unwrap_or_else(|_| SocketAddr::from(([0,0,0,0],0)));

        match result {
            Some(Ok(line)) => {
                trace!("Worker {} received line from {}: {:?}", self.worker_id, peer, line);
                match framed.send(line.clone()).await {
                    Ok(_) => {
                        debug!("Worker {} echoed back to {}: {:?}", self.worker_id, peer, line);
                        self.connected_streams.push(framed.into_future());
                    }
                    Err(e) => {
                        error!("Worker {} send error to {}: {}", self.worker_id, peer, e);
                        self.handle_disconnect(peer).await?;
                    }
                }
            }
            Some(Err(e)) => {
                error!("Worker {} codec error from {}: {}", self.worker_id, peer, e);
                self.handle_disconnect(peer).await?;
            }
            None => {
                debug!("Worker {}: stream from {} ended", self.worker_id, peer);
                self.handle_disconnect(peer).await?;
            }
        }
        Ok(())
    }

    async fn handle_disconnect(
        &mut self,
        peer: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.addr_map.remove(&peer);
        debug!("Worker {}: {} closed", self.worker_id, peer);

        // send authoritative total
        self.send_load_update().await;
        Ok(())
    }

    async fn send_load_update(&mut self) {
        let total = self.addr_map.len() as i32;
        if !self.load_update_tx.is_closed() {
            if let Err(e) = self.load_update_tx.send(LoadUpdate {
                worker_id: self.worker_id,
                total,
            }).await
            {
                trace!("Worker {} failed to send load update: {}", self.worker_id, e);
            }
        }
    }
}