// Network Listener - UDP socket management for Art-Net and sACN

use crate::network::artnet::{parse_artnet_packet, ArtNetPacket, ARTNET_PORT};
use crate::network::sacn::{parse_sacn_packet, SacnPacket, SACN_PORT};
use crate::network::source::{SourceDirection, SourceManagerHandle};

use parking_lot::RwLock;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;

/// DMX data for a universe
#[derive(Debug, Clone)]
pub struct DmxData {
    pub universe: u16,
    pub data: Vec<u8>,
    pub source_ip: IpAddr,
    pub timestamp: u64,
}

/// Event types emitted by the listener
#[derive(Debug, Clone)]
pub enum ListenerEvent {
    SourcesUpdated,
    DmxData(DmxData),
}

/// DMX data storage for all universes
pub struct DmxStore {
    data: RwLock<HashMap<u16, Vec<u8>>>,
}

impl DmxStore {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }

    pub fn update(&self, universe: u16, data: Vec<u8>) {
        let mut store = self.data.write();
        store.insert(universe, data);
    }

    pub fn get(&self, universe: u16) -> Option<Vec<u8>> {
        let store = self.data.read();
        store.get(&universe).cloned()
    }

    pub fn get_all(&self) -> HashMap<u16, Vec<u8>> {
        self.data.read().clone()
    }
}

impl Default for DmxStore {
    fn default() -> Self {
        Self::new()
    }
}

pub type DmxStoreHandle = Arc<DmxStore>;

/// Network listener configuration
#[derive(Debug, Clone)]
pub struct ListenerConfig {
    pub listen_artnet: bool,
    pub listen_sacn: bool,
    pub bind_address: Ipv4Addr,
}

impl Default for ListenerConfig {
    fn default() -> Self {
        Self {
            listen_artnet: true,
            listen_sacn: true,
            bind_address: Ipv4Addr::UNSPECIFIED,
        }
    }
}

/// Start the Art-Net listener
pub async fn start_artnet_listener(
    source_manager: SourceManagerHandle,
    dmx_store: DmxStoreHandle,
    event_tx: broadcast::Sender<ListenerEvent>,
    bind_addr: Ipv4Addr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::new(IpAddr::V4(bind_addr), ARTNET_PORT);
    let socket = UdpSocket::bind(addr).await?;

    // Enable broadcast receiving
    socket.set_broadcast(true)?;

    println!("[Art-Net] Listening on {}", addr);

    let mut buf = vec![0u8; 1500];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, src)) => {
                if let Some(packet) = parse_artnet_packet(&buf[..len], src) {
                    match packet {
                        ArtNetPacket::PollReply(reply) => {
                            let ip = IpAddr::V4(Ipv4Addr::new(
                                reply.ip_address[0],
                                reply.ip_address[1],
                                reply.ip_address[2],
                                reply.ip_address[3],
                            ));

                            // Calculate universes from sw_out
                            let mut universes = Vec::new();
                            for i in 0..reply.num_ports.min(4) as usize {
                                if reply.port_types[i] & 0x80 != 0 {
                                    // Output port
                                    let uni = crate::network::artnet::calculate_artnet_universe(
                                        reply.net_switch,
                                        reply.sub_switch,
                                        reply.sw_out[i],
                                    );
                                    universes.push(uni);
                                }
                            }

                            source_manager.update_artnet_source(
                                ip,
                                &reply.short_name,
                                &reply.long_name,
                                Some(reply.mac_address),
                                Some(universes),
                                None, // No sequence number for PollReply
                            );

                            let _ = event_tx.send(ListenerEvent::SourcesUpdated);
                        }
                        ArtNetPacket::Dmx(dmx) => {
                            // Get source IP and update as Art-Net source (sending DMX)
                            let ip = src.ip();
                            source_manager.update_artnet_source_with_direction(
                                ip,
                                "",
                                "",
                                None,
                                Some(vec![dmx.universe]),
                                SourceDirection::Sending,
                                Some(dmx.sequence),
                            );

                            // Store DMX data
                            dmx_store.update(dmx.universe, dmx.data.clone());

                            let _ = event_tx.send(ListenerEvent::DmxData(DmxData {
                                universe: dmx.universe,
                                data: dmx.data,
                                source_ip: ip,
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64,
                            }));
                        }
                        ArtNetPacket::Poll => {
                            // We don't respond to polls in monitor mode
                        }
                        ArtNetPacket::Other(_) => {
                            // Ignore other packet types for now
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[Art-Net] Receive error: {}", e);
            }
        }
    }
}

/// Start the sACN listener
pub async fn start_sacn_listener(
    source_manager: SourceManagerHandle,
    dmx_store: DmxStoreHandle,
    event_tx: broadcast::Sender<ListenerEvent>,
    bind_addr: Ipv4Addr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::new(IpAddr::V4(bind_addr), SACN_PORT);

    // Create socket with socket2 for multicast support
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;

    socket.set_reuse_address(true)?;
    #[cfg(not(windows))]
    socket.set_reuse_port(true)?;

    socket.bind(&addr.into())?;
    socket.set_nonblocking(true)?;

    // Join multicast groups for universes 1-63999
    // For efficiency, we join a few common universes initially
    let multicast_interface = bind_addr;
    let mut joined_count = 0;
    let mut failed_count = 0;

    for universe in 1..=100 {
        let multicast_addr = crate::network::sacn::sacn_multicast_address(universe);
        match socket.join_multicast_v4(&multicast_addr, &multicast_interface) {
            Ok(_) => {
                joined_count += 1;
                if universe <= 20 {
                    println!(
                        "[sACN] Joined multicast group for universe {} ({})",
                        universe, multicast_addr
                    );
                }
            }
            Err(e) => {
                failed_count += 1;
                if universe <= 20 {
                    eprintln!(
                        "[sACN] Failed to join multicast for universe {}: {}",
                        universe, e
                    );
                }
            }
        }
    }

    println!(
        "[sACN] Multicast groups: {} joined, {} failed",
        joined_count, failed_count
    );

    let socket: std::net::UdpSocket = socket.into();
    let socket = UdpSocket::from_std(socket)?;

    println!("[sACN] Listening on {} (multicast)", addr);

    let mut buf = vec![0u8; 1500];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, src)) => {
                // Debug: Log raw packet info before parsing
                // Universe is at bytes 113-114 in the packet
                if len >= 115 {
                    let raw_universe = u16::from_be_bytes([buf[113], buf[114]]);
                    let raw_start_code = if len > 125 { buf[125] } else { 255 };
                    println!(
                        "[sACN RAW] Packet from {} - universe: {}, start_code: {}, len: {}",
                        src.ip(),
                        raw_universe,
                        raw_start_code,
                        len
                    );
                }

                if let Some(packet) = parse_sacn_packet(&buf[..len], src) {
                    match packet {
                        SacnPacket::Dmx(dmx) => {
                            source_manager.update_sacn_source_with_direction(
                                src.ip(),
                                &dmx.source.source_name,
                                &dmx.source.cid,
                                dmx.source.priority,
                                dmx.source.universe,
                                SourceDirection::Sending,
                                Some(dmx.source.sequence),
                            );

                            // Store DMX data
                            dmx_store.update(dmx.source.universe, dmx.data.clone());

                            let _ = event_tx.send(ListenerEvent::DmxData(DmxData {
                                universe: dmx.source.universe,
                                data: dmx.data,
                                source_ip: src.ip(),
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64,
                            }));
                        }
                        SacnPacket::Discovery(discovery) => {
                            // Update source with discovered universes
                            for universe in &discovery.universes {
                                source_manager.update_sacn_source(
                                    src.ip(),
                                    &discovery.source_name,
                                    &discovery.cid,
                                    100, // Default priority for discovery
                                    *universe,
                                    None, // No sequence number for Discovery
                                );
                            }
                            let _ = event_tx.send(ListenerEvent::SourcesUpdated);
                        }
                        SacnPacket::Sync { .. } => {
                            // Sync packets are handled elsewhere if needed
                        }
                        SacnPacket::Unknown => {}
                    }
                }
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::WouldBlock {
                    eprintln!("[sACN] Receive error: {}", e);
                }
            }
        }
    }
}

/// Start the status update loop
pub async fn start_status_updater(
    source_manager: SourceManagerHandle,
    event_tx: broadcast::Sender<ListenerEvent>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));

    loop {
        interval.tick().await;
        source_manager.update_statuses();
        source_manager.cleanup_stale_sources();
        let _ = event_tx.send(ListenerEvent::SourcesUpdated);
    }
}
