# Rust Echo Server and Client

This project implements a Tokio TCP echo server and client in Rust. It meets the functional requirements of a simple echo system and extends them with additional features for testing, logging, and graceful shutdown.

## Features

- Server listens on `127.0.0.1:9000` by default
- Multiple concurrent client connections are supported
- Each message received is echoed back to its originating client
- Client sends a message provided as a command-line argument (defaults to `"hello world"`)
- Client reads the echoed message and prints it to stdout
- Graceful shutdown on `Ctrl+C`
- Structured logging of connections, messages, and throughput
- Extended options for multiple clients, multiple messages, and throughput reporting

## Building

```bash
cargo build
```
or
```bash
cargo test
```

This produces two binaries:

- `server` – the echo server
- `client` – the echo client

## Running the Server

Start the server with defaults:

```bash
cargo run --bin server
```
The server listens on `127.0.0.1:9000`.

If that port is in use, try
```bash
cargo run --bin server -- --port 9123
```

We have many other options for configuring the server. For example, to run the server on a different IP address and port, and with four worker threads and a channel capacity of 200 messages:
```bash
cargo run --bin server -- --ip 127.0.0.1 --port 8000 --num-workers 4 --channel-capacity 200
```

### Server Parameters

- `--ip <IP>` – bind IP address (default: `127.0.0.1`)
- `--port <u16>` – listen port (default: `9000`)
- `--num-workers <usize>` – number of worker tasks (default: logical CPU count)
- `--channel-capacity <usize>` – capacity of worker channels (default: `100`)
- `--log_level <LEVEL>` – log level (DEBUG, INFO, WARN, etc.)

## Running the Client

### Single Message Example

```bash
cargo run --bin client -- --message "hello server"
```

With defaults (`--num-clients 1`, `--max-count 1`), the client sends a single message and prints the echoed reply. This satisfies the base assignment requirement.

### Multiple Clients

```bash
cargo run --bin client -- --message "ping" --num-clients 5
```

This spawns five logical clients concurrently, each sending one message.

### Multiple Messages per Client

```bash
cargo run --bin client -- --message "load" --num-clients 2 --max-count 10
```

Each of the two clients sends ten messages, for twenty total echoes.  
A reporting service aggregates totals and logs throughput periodically.

### Client Parameters

- `--server-ip <IP>` – server IP (default: `127.0.0.1`)
- `--server-port <u16>` – server port (default: `9000`)
- `--num-clients <usize>` – number of parallel client tasks (default: `1`)
- `--message <String>` – message payload (default: `"hello world"`)
- `--max-count <u64>` – messages per client (default: `1`)
- `--log_level <LEVEL>` – log level (DEBUG, INFO, WARN, etc.)

## Demonstration Across Multiple Terminals

**Terminal 1** – run the server

```bash
cargo run --bin server
```

**Terminal 2** – start a client

```bash
cargo run --bin client -- --message "foo"
```

**Terminal 3** – start another client

```bash
cargo run --bin client -- --message "bar"
```

The server logs both connections and shows each echoed message. Each client receives and prints its matching echo.

## Graceful Shutdown

Pressing `Ctrl+C` in the server terminal triggers a clean shutdown of the connection service and worker tasks. Clients also exit after reaching their configured message count.

## Summary

This project provides a correct, idiomatic implementation of a TCP echo server and client in Rust. It fulfills the requirements:

- A server that listens, accepts, and echoes to multiple clients concurrently
- A client that connects, sends a string message, and prints the echoed reply

It extends the base functionality with additional testing options, structured logging, safe error handling, and graceful shutdown. This makes it suitable both as a learning tool and as a foundation for further experimentation in asynchronous network programming with Rust.

Here’s a clean **README section** you can drop straight into your project documentation:

---

## Efficiency and Scaling

This project is designed with both **idleness efficiency** and **maximum throughput** in mind:

- **Asynchronous I/O**:  
  All networking is handled through Tokio’s async runtime, which uses the OS event system (`epoll/kqueue/IOCP`).
    - When no clients are connected or no messages are in flight, all tasks are suspended by the runtime and threads are put to sleep by the OS scheduler.
    - This means CPU usage at idle is effectively **near zero**, aside from occasional timer ticks used for load reporting.

- **Worker‑per‑Core Model**:  
  On startup, the server spawns one `EchoService` worker per logical CPU core (by default).
    - Each worker is an independent async task handling framed connections concurrently.
    - Incoming connections are distributed across the workers, allowing throughput to scale horizontally with hardware capabilities.
    - This approach mirrors the design of high‑performance servers (e.g., NGINX, Redis), ensuring no single thread or core becomes a bottleneck under load.

- **Optimistic Load Distribution + Reconciliation**:  
  New connections are distributed optimistically to workers to prevent connection bursts from piling up, while each worker periodically reports its authoritative connection count back to the acceptor.
    - This prevents counters from drifting, ensures connections are balanced in the long run, and allows bursts of traffic to be spread with minimal latency.

### Result

- **At rest**: negligible CPU usage and minimal power draw.
- **Under load**: efficient use of *all available cores*, scaling throughput close to linearly with hardware.
- **In practice**: the server behaves politely when idle, but can squeeze out maximum performance when stressed.
