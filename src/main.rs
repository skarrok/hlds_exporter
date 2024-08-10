mod config;
mod hlds;
mod metrics;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::vec;

use clap::Parser;
use dotenvy::dotenv;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use config::Config;
use config::LogFormat;
use config::LogLevel;
use hlds::MAX_REPLY_SIZE;

fn setup_logger(log_level: LogLevel, log_format: LogFormat) {
    let log_level: LevelFilter = log_level.into();

    let with_color = supports_color::on(supports_color::Stream::Stderr)
        .filter(|s| s.has_basic)
        .is_some();
    let filter = EnvFilter::builder()
        .with_default_directive(
            format!(
                "{}={}",
                env!("CARGO_PKG_NAME").replace('-', "_"),
                log_level
            )
            .parse()
            .expect("Filter string shoud be correct"),
        )
        .from_env_lossy();
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(with_color);

    match log_format {
        LogFormat::Console => builder.init(),
        LogFormat::Json => builder.json().flatten_event(true).init(),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    let config = Config::parse();
    setup_logger(config.log_level, config.log_format);

    config.log();
    let m = metrics::Metrics::new(config.metrics_addr.to_string());
    m.listen()?;

    let socket = Arc::new(UdpSocket::bind(config.listen_addr).await?);

    let mut servers = vec![];
    let mut addr_to_channel = HashMap::new();
    let shared_metrics = Arc::new(m);
    for addr in config.server_addr {
        if addr_to_channel.contains_key(&addr) {
            tracing::warn!("Duplicate server address: {}. Skipping", addr);
            continue;
        }
        let (tx_challenge, rx_challenge) = mpsc::channel::<Vec<u8>>(1);
        let (tx_packet, rx_packet) = mpsc::channel::<Vec<u8>>(1);
        let mut interval = time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        let gs = hlds::GameServer::new(
            addr,
            interval,
            rx_challenge,
            tx_challenge,
            rx_packet,
            Arc::clone(&socket),
            Arc::clone(&shared_metrics),
        );
        servers.push(gs);
        addr_to_channel.insert(addr, tx_packet);
    }

    for server in servers {
        let mut server = server;
        tokio::spawn(async move {
            server.process().await;
        });
    }

    #[allow(clippy::infinite_loop)]
    let reader = tokio::spawn(async move {
        let mut buf = [0; MAX_REPLY_SIZE];
        loop {
            let Ok((amt, src)) = socket.recv_from(&mut buf).await else {
                tracing::warn!("Error reading from socket");
                continue;
            };
            let channel = addr_to_channel.get(&src);
            if let Some(c) = channel {
                let Some(buf) = buf.get(..amt) else {
                    tracing::warn!("Error slicing buffer");
                    continue;
                };
                c.send(Vec::from(buf)).await.unwrap_or_else(|e| {
                    tracing::warn!("Error sending packet to worker: {}", e);
                });
            }
        }
    });

    reader.await?;

    Ok(())
}
