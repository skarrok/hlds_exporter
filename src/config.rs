use std::net::{SocketAddr, ToSocketAddrs};

use anyhow::anyhow;
use clap::{Parser, ValueEnum};
use serde::Serialize;
use serde_json::to_value;
use tracing::level_filters::LevelFilter;

/// HLDS metrics exporter in prometheus format
#[derive(Debug, Parser, Serialize)]
#[command(author, version, about, long_about = None)]
pub struct Config {
    /// Verbosity of logging
    #[arg(long, value_enum, env, default_value_t = LogLevel::Debug)]
    pub log_level: LogLevel,

    /// Format of logs
    #[arg(long, value_enum, env, default_value_t = LogFormat::Console)]
    pub log_format: LogFormat,

    /// Address for exporting metrics
    #[arg(long, env, value_parser = socketaddr_value_parser, default_value = "127.0.0.1:9000")]
    pub metrics_addr: SocketAddr,

    /// HLDS Server Addresses
    #[arg(long, env, value_parser = socketaddr_value_parser, num_args = 1.., default_value = "127.0.0.1:27015")]
    pub server_addr: Vec<SocketAddr>,

    /// UDP Bind Address
    #[arg(long, env, default_value = "0.0.0.0:0")]
    pub listen_addr: SocketAddr,
}

#[derive(ValueEnum, Debug, Clone, Copy, Serialize)]
pub enum LogFormat {
    /// Pretty logs for debugging
    Console,
    /// JSON logs
    Json,
}

#[derive(ValueEnum, Debug, Clone, Copy, Serialize)]
pub enum LogLevel {
    Off,
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for LevelFilter {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Off => Self::OFF,
            LogLevel::Trace => Self::TRACE,
            LogLevel::Debug => Self::DEBUG,
            LogLevel::Info => Self::INFO,
            LogLevel::Warn => Self::WARN,
            LogLevel::Error => Self::ERROR,
        }
    }
}

fn socketaddr_value_parser(value: &str) -> anyhow::Result<SocketAddr> {
    match value.to_socket_addrs() {
        Ok(mut iter) => iter.next().ok_or_else(|| anyhow!(value.to_string())),
        Err(e) => Err(e.into()),
    }
}

impl Config {
    pub fn log(&self) {
        if let Ok(json_obj) = to_value(self) {
            if let Ok(json_obj) =
                json_obj.as_object().ok_or_else(|| anyhow!("WTF"))
            {
                for (key, value) in json_obj {
                    log::debug!("Config {}={}", key, value);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::ToSocketAddrs;

    use super::socketaddr_value_parser;

    #[test]
    fn parse_socketaddr_with_default() {
        let expected =
            "localhost:9000".to_socket_addrs().unwrap().next().unwrap();
        let test_values = ["localhost:9000", "127.0.0.1:9000", "127.1:9000"];
        for value in test_values {
            assert_eq!(
                socketaddr_value_parser(value).expect("value_parser failed"),
                expected
            );
        }
    }

    #[test]
    fn verify_cli() {
        use super::Config;
        use clap::CommandFactory;
        Config::command().debug_assert();
    }
}
