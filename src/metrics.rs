use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::bail;
use prometheus_client::metrics::family::Family;
use prometheus_client::registry::Registry;
use prometheus_client::{encoding::text::encode, metrics::gauge::Gauge};
use tiny_http::{Response, Server};

pub struct Metrics {
    registry: Arc<Mutex<Registry>>,
    export_addr: String,
    players: Family<Vec<(String, String)>, Gauge>,
    bots: Family<Vec<(String, String)>, Gauge>,
    info: Family<Vec<(String, String)>, Gauge>,
    up: Family<Vec<(String, String)>, Gauge>,
}

impl Metrics {
    pub fn new(metrics_addr: String) -> Self {
        let metrics = Self {
            registry: Arc::new(Mutex::new(Registry::default())),
            export_addr: metrics_addr,
            players: Family::default(),
            bots: Family::default(),
            info: Family::default(),
            up: Family::default(),
        };
        let mut m = metrics.registry.lock().unwrap();
        m.register("hlds_info", "server info", metrics.info.clone());
        m.register(
            "hlds_players",
            "current number of players",
            metrics.players.clone(),
        );
        m.register(
            "hlds_bots",
            "current number of bots",
            metrics.bots.clone(),
        );
        m.register(
            "hlds_up",
            "server is up",
            metrics.up.clone(),
        );
        drop(m);
        metrics
    }

    pub fn observe_players(&self, addr: SocketAddr, players: u8, bots: u8) {
        self.players
            .get_or_create(&vec![("addr".to_string(), addr.to_string())])
            .set(i64::from(players));
        self.bots
            .get_or_create(&vec![("addr".to_string(), addr.to_string())])
            .set(i64::from(bots));
    }

    pub fn observe_info(
        &self,
        addr: SocketAddr,
        name: String,
        game: String,
        version: String,
    ) {
        self.info
            .get_or_create(&vec![
                ("name".to_string(), name),
                ("addr".to_string(), addr.to_string()),
                ("game".to_string(), game),
                ("version".to_string(), version),
            ])
            .set(1);
    }

    pub fn observe_up(&self, addr: SocketAddr, up: bool) {
        self.up
            .get_or_create(&vec![("addr".to_string(), addr.to_string())])
            .set(up.into());
    }

    pub fn listen(&self) -> anyhow::Result<()> {
        let server = match Server::http(&self.export_addr) {
            Ok(server) => server,
            Err(err) => bail!("Can't export metrics: {}", err),
        };

        let registry = Arc::clone(&self.registry);

        std::thread::spawn(move || Self::serve_metrics(&server, &registry));
        Ok(())
    }

    fn serve_metrics(server: &Server, registry: &Arc<Mutex<Registry>>) {
        for request in server.incoming_requests() {
            match request.url() {
                "/metrics" => Self::export_metrics(request, registry),
                _ => {
                    let response = Response::from_string("Not found")
                        .with_status_code(404);
                    if let Err(err) = request.respond(response) {
                        tracing::debug!("Can't send response: {}", err);
                    }
                },
            }
        }
    }

    fn export_metrics(
        request: tiny_http::Request,
        registry: &Arc<Mutex<Registry>>,
    ) {
        let mut buf = String::new();
        match registry.lock() {
            Ok(registry) => {
                if let Err(err) = encode(&mut buf, &registry) {
                    tracing::debug!("Can't encode metrics {}", err);
                }

                let response = Response::from_string(buf);
                if let Err(err) = request.respond(response) {
                    tracing::debug!("Can't send response {}", err);
                }
            },
            Err(err) => {
                tracing::debug!("Can't access registry {}", err);
            },
        }
    }
}
