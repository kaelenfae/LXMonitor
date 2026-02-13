// Sniffer module - Promiscuous mode packet capture for Art-Net and sACN
//
// This module provides optional packet capture functionality using pcap/Npcap
// to see traffic destined for other IPs on the network (requires port mirroring).
//
// The sniffer feature requires the Npcap SDK to be installed for building.
// When the feature is disabled, stub implementations are provided.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[cfg(feature = "sniffer")]
use crate::network::artnet::{parse_artnet_packet, ARTNET_PORT};
#[cfg(feature = "sniffer")]
use crate::network::listener::{DmxData, DmxStoreHandle, ListenerEvent};
#[cfg(feature = "sniffer")]
use crate::network::sacn::{parse_sacn_packet, SACN_PORT};
#[cfg(feature = "sniffer")]
use crate::network::source::{SourceDirection, SourceManagerHandle};

#[cfg(feature = "sniffer")]
use pcap::{Capture, Device};
#[cfg(feature = "sniffer")]
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
#[cfg(feature = "sniffer")]
use tokio::sync::broadcast;

// Re-export types needed by lib.rs even without feature
#[cfg(not(feature = "sniffer"))]
use crate::network::listener::{DmxStoreHandle, ListenerEvent};
#[cfg(not(feature = "sniffer"))]
use crate::network::source::SourceManagerHandle;
#[cfg(not(feature = "sniffer"))]
use tokio::sync::broadcast;

/// Capture interface info for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureInterface {
    pub name: String,
    pub description: Option<String>,
}

/// Sniffer status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnifferStatus {
    pub enabled: bool,
    pub interface: Option<String>,
    pub npcap_available: bool,
    pub packets_captured: u64,
    pub error: Option<String>,
}

/// Sniffer state
pub struct SnifferState {
    pub enabled: Mutex<bool>,
    pub interface: Mutex<Option<String>>,
    pub packets_captured: Mutex<u64>,
    pub error: Mutex<Option<String>>,
    pub stop_flag: Mutex<bool>,
}

impl SnifferState {
    pub fn new() -> Self {
        Self {
            enabled: Mutex::new(false),
            interface: Mutex::new(None),
            packets_captured: Mutex::new(0),
            error: Mutex::new(None),
            stop_flag: Mutex::new(false),
        }
    }

    pub fn get_status(&self) -> SnifferStatus {
        SnifferStatus {
            enabled: *self.enabled.lock(),
            interface: self.interface.lock().clone(),
            npcap_available: is_npcap_available(),
            packets_captured: *self.packets_captured.lock(),
            error: self.error.lock().clone(),
        }
    }
}

impl Default for SnifferState {
    fn default() -> Self {
        Self::new()
    }
}

pub type SnifferStateHandle = Arc<SnifferState>;

// ============================================================================
// With sniffer feature enabled
// ============================================================================

#[cfg(feature = "sniffer")]
pub fn is_npcap_available() -> bool {
    Device::list().is_ok()
}

#[cfg(feature = "sniffer")]
pub fn list_capture_interfaces() -> Vec<CaptureInterface> {
    match Device::list() {
        Ok(devices) => devices
            .into_iter()
            .map(|d| CaptureInterface {
                name: d.name.clone(),
                description: d.desc.clone(),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(feature = "sniffer")]
pub fn start_sniffer_blocking(
    interface_name: &str,
    source_manager: SourceManagerHandle,
    dmx_store: DmxStoreHandle,
    event_tx: broadcast::Sender<ListenerEvent>,
    sniffer_state: SnifferStateHandle,
) {
    // Find the device
    let devices = match Device::list() {
        Ok(d) => d,
        Err(e) => {
            *sniffer_state.error.lock() = Some(format!("Failed to list devices: {}", e));
            return;
        }
    };

    let device = match devices.into_iter().find(|d| d.name == interface_name) {
        Some(d) => d,
        None => {
            *sniffer_state.error.lock() = Some(format!("Interface not found: {}", interface_name));
            return;
        }
    };

    // Open the capture
    let mut cap = match Capture::from_device(device) {
        Ok(c) => c,
        Err(e) => {
            *sniffer_state.error.lock() = Some(format!("Failed to open device: {}", e));
            return;
        }
    };

    // Configure capture
    let cap = cap.promisc(true).snaplen(1500).timeout(100); // 100ms timeout for checking stop flag

    let mut cap = match cap.open() {
        Ok(c) => c,
        Err(e) => {
            *sniffer_state.error.lock() = Some(format!("Failed to start capture: {}", e));
            return;
        }
    };

    // Set BPF filter for Art-Net and sACN ports
    let filter = format!("udp port {} or udp port {}", ARTNET_PORT, SACN_PORT);
    if let Err(e) = cap.filter(&filter, true) {
        *sniffer_state.error.lock() = Some(format!("Failed to set filter: {}", e));
        return;
    }

    println!(
        "[Sniffer] Started on interface {} with filter: {}",
        interface_name, filter
    );
    *sniffer_state.error.lock() = None;

    // Capture loop
    loop {
        // Check stop flag
        if *sniffer_state.stop_flag.lock() {
            println!("[Sniffer] Stopped by user");
            break;
        }

        // Try to get next packet
        match cap.next_packet() {
            Ok(packet) => {
                // Increment packet count
                *sniffer_state.packets_captured.lock() += 1;

                // Parse the packet - we need to extract IP header info
                if let Some((src_ip, dst_ip, src_port, dst_port, payload)) =
                    parse_ip_udp_packet(packet.data)
                {
                    let src_addr = SocketAddr::new(IpAddr::V4(src_ip), src_port);
                    let dst_addr = SocketAddr::new(IpAddr::V4(dst_ip), dst_port);

                    // Determine direction based on which port matches
                    let is_artnet = src_port == ARTNET_PORT || dst_port == ARTNET_PORT;
                    let is_sacn = src_port == SACN_PORT || dst_port == SACN_PORT;

                    if is_artnet {
                        if let Some(packet) = parse_artnet_packet(payload, src_addr) {
                            match packet {
                                crate::network::artnet::ArtNetPacket::Dmx(dmx) => {
                                    // Source is sending
                                    source_manager.update_artnet_source_with_direction(
                                        src_addr.ip(),
                                        "",
                                        "",
                                        None,
                                        Some(vec![dmx.universe]),
                                        SourceDirection::Sending,
                                        Some(dmx.sequence),
                                    );

                                    // Destination is receiving (if not broadcast)
                                    if !dst_ip.is_broadcast()
                                        && dst_ip != Ipv4Addr::new(255, 255, 255, 255)
                                    {
                                        source_manager.update_artnet_source_with_direction(
                                            dst_addr.ip(),
                                            "",
                                            "",
                                            None,
                                            Some(vec![dmx.universe]),
                                            SourceDirection::Receiving,
                                            None, // No sequence available/relevant for destination inference
                                        );
                                    }

                                    // Store DMX data
                                    dmx_store.update(dmx.universe, dmx.data.clone());

                                    let _ = event_tx.send(ListenerEvent::DmxData(DmxData {
                                        universe: dmx.universe,
                                        data: dmx.data,
                                        source_ip: src_addr.ip(),
                                        timestamp: std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis()
                                            as u64,
                                    }));
                                }
                                crate::network::artnet::ArtNetPacket::PollReply(reply) => {
                                    let ip = IpAddr::V4(Ipv4Addr::new(
                                        reply.ip_address[0],
                                        reply.ip_address[1],
                                        reply.ip_address[2],
                                        reply.ip_address[3],
                                    ));

                                    let mut universes = Vec::new();
                                    for i in 0..reply.num_ports.min(4) as usize {
                                        if reply.port_types[i] & 0x80 != 0 {
                                            let uni =
                                                crate::network::artnet::calculate_artnet_universe(
                                                    reply.net_switch,
                                                    reply.sub_switch,
                                                    reply.sw_out[i],
                                                );
                                            universes.push(uni);
                                        }
                                    }

                                    source_manager.update_artnet_source_with_direction(
                                        ip,
                                        &reply.short_name,
                                        &reply.long_name,
                                        Some(reply.mac_address),
                                        Some(universes),
                                        SourceDirection::Receiving,
                                        None, // No sequence for PollReply
                                    );

                                    let _ = event_tx.send(ListenerEvent::SourcesUpdated);
                                }
                                _ => {}
                            }
                        }
                    } else if is_sacn {
                        if let Some(packet) = parse_sacn_packet(payload, src_addr) {
                            match packet {
                                crate::network::sacn::SacnPacket::Dmx(dmx) => {
                                    // Source is sending
                                    source_manager.update_sacn_source_with_direction(
                                        src_addr.ip(),
                                        &dmx.source.source_name,
                                        &dmx.source.cid,
                                        dmx.source.priority,
                                        dmx.source.universe,
                                        SourceDirection::Sending,
                                        Some(dmx.source.sequence),
                                    );

                                    // For unicast sACN, mark destination as receiving
                                    if !dst_ip.is_multicast() && !dst_ip.is_broadcast() {
                                        source_manager.update_sacn_source_with_direction(
                                            dst_addr.ip(),
                                            "",
                                            &[0u8; 16],
                                            0,
                                            dmx.source.universe,
                                            SourceDirection::Receiving,
                                            None, // No sequence for destination inference
                                        );
                                    }

                                    dmx_store.update(dmx.source.universe, dmx.data.clone());

                                    let _ = event_tx.send(ListenerEvent::DmxData(DmxData {
                                        universe: dmx.source.universe,
                                        data: dmx.data,
                                        source_ip: src_addr.ip(),
                                        timestamp: std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis()
                                            as u64,
                                    }));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            Err(pcap::Error::TimeoutExpired) => {
                continue;
            }
            Err(e) => {
                eprintln!("[Sniffer] Capture error: {}", e);
                *sniffer_state.error.lock() = Some(format!("Capture error: {}", e));
                break;
            }
        }
    }

    *sniffer_state.enabled.lock() = false;
}

#[cfg(feature = "sniffer")]
fn parse_ip_udp_packet(data: &[u8]) -> Option<(Ipv4Addr, Ipv4Addr, u16, u16, &[u8])> {
    if data.len() < 42 {
        return None;
    }

    let eth_type = u16::from_be_bytes([data[12], data[13]]);
    if eth_type != 0x0800 {
        return None;
    }

    let ip_start = 14;
    let ip_header = &data[ip_start..];

    let version = (ip_header[0] >> 4) & 0x0F;
    if version != 4 {
        return None;
    }

    let ihl = (ip_header[0] & 0x0F) as usize * 4;
    if ihl < 20 || ip_start + ihl > data.len() {
        return None;
    }

    let protocol = ip_header[9];
    if protocol != 17 {
        return None;
    }

    let src_ip = Ipv4Addr::new(ip_header[12], ip_header[13], ip_header[14], ip_header[15]);
    let dst_ip = Ipv4Addr::new(ip_header[16], ip_header[17], ip_header[18], ip_header[19]);

    let udp_start = ip_start + ihl;
    if udp_start + 8 > data.len() {
        return None;
    }

    let udp_header = &data[udp_start..];
    let src_port = u16::from_be_bytes([udp_header[0], udp_header[1]]);
    let dst_port = u16::from_be_bytes([udp_header[2], udp_header[3]]);

    let payload_start = udp_start + 8;
    if payload_start > data.len() {
        return None;
    }

    let payload = &data[payload_start..];
    Some((src_ip, dst_ip, src_port, dst_port, payload))
}

// ============================================================================
// Without sniffer feature - stub implementations
// ============================================================================

#[cfg(not(feature = "sniffer"))]
pub fn is_npcap_available() -> bool {
    false
}

#[cfg(not(feature = "sniffer"))]
pub fn list_capture_interfaces() -> Vec<CaptureInterface> {
    Vec::new()
}

#[cfg(not(feature = "sniffer"))]
pub fn start_sniffer_blocking(
    _interface_name: &str,
    _source_manager: SourceManagerHandle,
    _dmx_store: DmxStoreHandle,
    _event_tx: broadcast::Sender<ListenerEvent>,
    sniffer_state: SnifferStateHandle,
) {
    *sniffer_state.error.lock() =
        Some("Sniffer feature not compiled. Rebuild with --features sniffer".to_string());
    *sniffer_state.enabled.lock() = false;
}
