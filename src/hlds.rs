use anyhow::anyhow;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Cursor, Read};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::select;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::Interval;

use crate::metrics::Metrics;

pub const MAX_REPLY_SIZE: usize = 1400;

static A2S_INFO: &[u8] = b"\xFF\xFF\xFF\xFF\x54Source Engine Query\0";
const S2A_INFO: u8 = 0x49;

const S2C_CHALLENGE: u8 = 0x41;

static SPLIT_PACKET: &[u8] = b"\xFE\xFF\xFF\xFF";
static HEADER: &[u8] = b"\xFF\xFF\xFF\xFF";
const CHALLENGE_LENGHT: usize = 4;

#[derive(Debug)]
#[repr(u8)]
enum ServerType {
    Dedicated = b'd',
    Listen = b'i',
    Proxy = b'p',
}

impl TryFrom<u8> for ServerType {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            b'd' => Ok(Self::Dedicated),
            b'l' => Ok(Self::Listen),
            b'p' => Ok(Self::Proxy),
            _ => Err(anyhow!("Invalid server type")),
        }
    }
}

#[derive(Debug)]
#[repr(u8)]
enum EnvironmentType {
    Linux = b'l',
    Windows = b'w',
    Mac = b'm',
}

impl TryFrom<u8> for EnvironmentType {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            b'l' => Ok(Self::Linux),
            b'w' => Ok(Self::Windows),
            b'm' => Ok(Self::Mac),
            _ => Err(anyhow!("Invalid environment type")),
        }
    }
}

#[derive(Debug)]
#[repr(u8)]
enum Visibility {
    Public = 0,
    Private = 1,
}

impl TryFrom<u8> for Visibility {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Public),
            1 => Ok(Self::Private),
            _ => Err(anyhow!("Invalid visibility")),
        }
    }
}

#[derive(Debug)]
#[repr(u8)]
enum Vac {
    Unsecured = 0,
    Secured = 1,
}

impl TryFrom<u8> for Vac {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unsecured),
            1 => Ok(Self::Secured),
            _ => Err(anyhow!("Invalid VAC status")),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct ServerInfo {
    header: u8,
    protocol: u8,
    name: String,
    map: String,
    folder: String,
    game: String,
    id: i16,
    players: u8,
    max_players: u8,
    bots: u8,
    server_type: ServerType,
    environment: EnvironmentType,
    visibility: Visibility,
    vac: Vac,
    version: String,
}

pub struct GameServer {
    pub(crate) server_addr: SocketAddr,
    interval: Interval,
    rx_challenge: Receiver<Vec<u8>>,
    tx_challenge: Sender<Vec<u8>>,
    rx_packet: Receiver<Vec<u8>>,
    socket: Arc<UdpSocket>,

    last_update: Option<Instant>,
    challenge: Vec<u8>,
    metrics: Arc<Metrics>,
}

impl GameServer {
    pub(crate) fn new(
        server_addr: SocketAddr,
        interval: Interval,
        rx_challenge: Receiver<Vec<u8>>,
        tx_challenge: Sender<Vec<u8>>,
        rx_packet: Receiver<Vec<u8>>,
        socket: Arc<UdpSocket>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            server_addr,
            interval,
            rx_challenge,
            tx_challenge,
            rx_packet,
            socket,

            last_update: None,
            challenge: vec![],
            metrics,
        }
    }

    pub(crate) async fn process(&mut self) {
        loop {
            select! {
                _ = self.interval.tick() => {
                    self.get_info().await.unwrap_or_else(|e| tracing::debug!("Error requesting info: {}", e));
                    let up = self.last_update.map_or(false, |update| update.elapsed() < Duration::from_secs(5));
                    self.metrics.observe_up(self.server_addr, up);
                }
                Some(challenge) = self.rx_challenge.recv() => {
                    self.challenge = challenge;
                    self.get_info().await.unwrap_or_else(|e| tracing::debug!("Error requesting info: {}", e));
                }
                Some(packet) = self.rx_packet.recv() => {
                    self.parse_reply(&packet).await;
                    self.last_update = Some(Instant::now());
                }
            }
        }
    }

    pub(crate) async fn get_info(&self) -> anyhow::Result<()> {
        if self.challenge.is_empty() {
            self.socket.send_to(A2S_INFO, self.server_addr).await?;
        } else {
            let mut msg = Vec::from(A2S_INFO);
            msg.extend(&self.challenge);
            self.socket.send_to(&msg, self.server_addr).await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, reply), fields(server = %self.server_addr))]
    pub async fn parse_reply(&mut self, reply: &[u8]) {
        if reply.starts_with(SPLIT_PACKET) {
            tracing::warn!(server = %self.server_addr, "Split packet is not supported");
            return;
        }
        self.parse_packet(reply).await;
    }

    async fn parse_packet(&self, packet: &[u8]) {
        if !packet.starts_with(HEADER) {
            return;
        }

        let Some(type_) = packet.get(HEADER.len()) else {
            tracing::warn!(server = %self.server_addr, "Packet without type is received");
            return;
        };

        match *type_ {
            S2A_INFO => {
                self.parse_info(packet);
            },
            S2C_CHALLENGE => {
                if let Ok(challenge) = Self::parse_challenge(packet) {
                    let _ = self
                        .tx_challenge
                        .send(challenge)
                        .await
                        .inspect_err(|e| {
                            tracing::warn!("Failed to send challenge: {e}");
                        });
                }
            },
            _ => {},
        }
    }

    fn parse_info(&self, packet: &[u8]) {
        let buf = Cursor::new(packet);
        let info = ServerInfo::try_from(buf);
        if let Ok(info) = info {
            tracing::trace!("{:?}", &info);
            self.metrics.observe_players(
                self.server_addr,
                info.players,
                info.bots,
            );
            self.metrics.observe_info(
                self.server_addr,
                info.name,
                info.game,
                info.version,
            );
        }
    }

    fn parse_challenge(packet: &[u8]) -> anyhow::Result<Vec<u8>> {
        let index = HEADER.len() + 1;

        let challenge = &packet
            .get(index..index + CHALLENGE_LENGHT)
            .ok_or_else(|| anyhow!("Challenge is not long enough"))?;

        Ok(challenge.to_vec())
    }
}

impl TryFrom<Cursor<&[u8]>> for ServerInfo {
    type Error = anyhow::Error;

    fn try_from(value: Cursor<&[u8]>) -> Result<Self, Self::Error> {
        let mut value = value;
        let _packet_header = value.read_i32::<LittleEndian>()?;
        let header = value.read_u8()?;
        let protocol = value.read_u8()?;
        let name = read_cstring(&mut value)?;
        let map = read_cstring(&mut value)?;
        let folder = read_cstring(&mut value)?;
        let game = read_cstring(&mut value)?;
        let id = value.read_i16::<LittleEndian>()?;
        let players = value.read_u8()?;
        let max_players = value.read_u8()?;
        let bots = value.read_u8()?;
        let server_type = value.read_u8()?.try_into()?;
        let environment = value.read_u8()?.try_into()?;
        let visibility = value.read_u8()?.try_into()?;
        let vac = value.read_u8()?.try_into()?;
        let version = read_cstring(&mut value)?;

        Ok(Self {
            header,
            protocol,
            name,
            map,
            folder,
            game,
            id,
            players,
            max_players,
            bots,
            server_type,
            environment,
            visibility,
            vac,
            version,
        })
    }
}

fn read_cstring(buf: &mut Cursor<&[u8]>) -> anyhow::Result<String> {
    let end = buf.get_ref().len().try_into()?;
    let mut c = [0; 1];
    let mut str_vec = Vec::with_capacity(256);

    while buf.position() < end {
        buf.read_exact(&mut c)?;
        if c[0] == 0 {
            break;
        }
        str_vec.push(c[0]);
    }

    Ok(String::from_utf8_lossy(str_vec.as_slice()).to_string())
}
