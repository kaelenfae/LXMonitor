// Art-Net Protocol Implementation
// Art-Net 4 Protocol: https://art-net.org.uk/

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Art-Net OpCodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ArtNetOpCode {
    OpPoll = 0x2000,
    OpPollReply = 0x2100,
    OpDmx = 0x5000,
    OpNzs = 0x5100,
    OpSync = 0x5200,
    OpAddress = 0x6000,
    OpInput = 0x7000,
    OpTodRequest = 0x8000,
    OpTodData = 0x8100,
    OpTodControl = 0x8200,
    OpRdm = 0x8300,
    OpRdmSub = 0x8400,
    OpIpProg = 0xf800,
    OpIpProgReply = 0xf900,
    Unknown = 0xFFFF,
}

impl From<u16> for ArtNetOpCode {
    fn from(value: u16) -> Self {
        match value {
            0x2000 => ArtNetOpCode::OpPoll,
            0x2100 => ArtNetOpCode::OpPollReply,
            0x5000 => ArtNetOpCode::OpDmx,
            0x5100 => ArtNetOpCode::OpNzs,
            0x5200 => ArtNetOpCode::OpSync,
            0x6000 => ArtNetOpCode::OpAddress,
            0x7000 => ArtNetOpCode::OpInput,
            0x8000 => ArtNetOpCode::OpTodRequest,
            0x8100 => ArtNetOpCode::OpTodData,
            0x8200 => ArtNetOpCode::OpTodControl,
            0x8300 => ArtNetOpCode::OpRdm,
            0x8400 => ArtNetOpCode::OpRdmSub,
            0xf800 => ArtNetOpCode::OpIpProg,
            0xf900 => ArtNetOpCode::OpIpProgReply,
            _ => ArtNetOpCode::Unknown,
        }
    }
}

/// Art-Net packet header (first 12 bytes)
pub const ARTNET_HEADER: &[u8] = b"Art-Net\0";
pub const ARTNET_PORT: u16 = 6454;

/// Parsed Art-Net Poll Reply containing source information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtPollReply {
    pub ip_address: [u8; 4],
    pub port: u16,
    pub version_info: u16,
    pub net_switch: u8,
    pub sub_switch: u8,
    pub oem: u16,
    pub ubea_version: u8,
    pub status1: u8,
    pub esta_manufacturer: u16,
    pub short_name: String,
    pub long_name: String,
    pub node_report: String,
    pub num_ports: u16,
    pub port_types: [u8; 4],
    pub good_input: [u8; 4],
    pub good_output: [u8; 4],
    pub sw_in: [u8; 4],
    pub sw_out: [u8; 4],
    pub style: u8,
    pub mac_address: [u8; 6],
    pub bind_ip: [u8; 4],
    pub bind_index: u8,
    pub status2: u8,
}

impl Default for ArtPollReply {
    fn default() -> Self {
        Self {
            ip_address: [0; 4],
            port: ARTNET_PORT,
            version_info: 0,
            net_switch: 0,
            sub_switch: 0,
            oem: 0,
            ubea_version: 0,
            status1: 0,
            esta_manufacturer: 0,
            short_name: String::new(),
            long_name: String::new(),
            node_report: String::new(),
            num_ports: 0,
            port_types: [0; 4],
            good_input: [0; 4],
            good_output: [0; 4],
            sw_in: [0; 4],
            sw_out: [0; 4],
            style: 0,
            mac_address: [0; 6],
            bind_ip: [0; 4],
            bind_index: 0,
            status2: 0,
        }
    }
}

/// Parsed Art-Net DMX packet
#[derive(Debug, Clone)]
pub struct ArtDmx {
    pub sequence: u8,
    pub physical: u8,
    pub universe: u16, // 15-bit universe (net:subnet:universe)
    pub length: u16,
    pub data: Vec<u8>,
}

/// Result of parsing an Art-Net packet
#[derive(Debug, Clone)]
pub enum ArtNetPacket {
    Poll,
    PollReply(ArtPollReply),
    Dmx(ArtDmx),
    Other(ArtNetOpCode),
}

/// Parse an Art-Net packet from raw bytes
pub fn parse_artnet_packet(data: &[u8], _source: SocketAddr) -> Option<ArtNetPacket> {
    // Minimum packet size check
    if data.len() < 12 {
        return None;
    }

    // Check Art-Net header
    if &data[0..8] != ARTNET_HEADER {
        return None;
    }

    // Get OpCode (little-endian)
    let opcode = u16::from_le_bytes([data[8], data[9]]);
    let opcode = ArtNetOpCode::from(opcode);

    match opcode {
        ArtNetOpCode::OpPoll => Some(ArtNetPacket::Poll),
        ArtNetOpCode::OpPollReply => parse_poll_reply(data),
        ArtNetOpCode::OpDmx => parse_dmx(data),
        other => Some(ArtNetPacket::Other(other)),
    }
}

/// Parse ArtPollReply packet
fn parse_poll_reply(data: &[u8]) -> Option<ArtNetPacket> {
    if data.len() < 207 {
        return None;
    }

    let mut reply = ArtPollReply::default();

    // IP Address (bytes 10-13)
    reply.ip_address.copy_from_slice(&data[10..14]);

    // Port (bytes 14-15, little-endian)
    reply.port = u16::from_le_bytes([data[14], data[15]]);

    // Version (bytes 16-17, high byte first)
    reply.version_info = u16::from_be_bytes([data[16], data[17]]);

    // Net/Sub switch (bytes 18-19)
    reply.net_switch = data[18];
    reply.sub_switch = data[19];

    // OEM (bytes 20-21)
    reply.oem = u16::from_be_bytes([data[20], data[21]]);

    // UBEA version (byte 22)
    reply.ubea_version = data[22];

    // Status1 (byte 23)
    reply.status1 = data[23];

    // ESTA Manufacturer (bytes 24-25)
    reply.esta_manufacturer = u16::from_le_bytes([data[24], data[25]]);

    // Short Name (bytes 26-43, 18 bytes, null terminated)
    reply.short_name = extract_string(&data[26..44]);

    // Long Name (bytes 44-107, 64 bytes, null terminated)
    reply.long_name = extract_string(&data[44..108]);

    // Node Report (bytes 108-171, 64 bytes)
    reply.node_report = extract_string(&data[108..172]);

    // NumPorts (bytes 172-173)
    reply.num_ports = u16::from_be_bytes([data[172], data[173]]);

    // Port Types (bytes 174-177)
    reply.port_types.copy_from_slice(&data[174..178]);

    // Good Input (bytes 178-181)
    reply.good_input.copy_from_slice(&data[178..182]);

    // Good Output (bytes 182-185)
    reply.good_output.copy_from_slice(&data[182..186]);

    // SwIn (bytes 186-189)
    reply.sw_in.copy_from_slice(&data[186..190]);

    // SwOut (bytes 190-193)
    reply.sw_out.copy_from_slice(&data[190..194]);

    // Style (byte 200)
    if data.len() > 200 {
        reply.style = data[200];
    }

    // MAC Address (bytes 201-206)
    if data.len() >= 207 {
        reply.mac_address.copy_from_slice(&data[201..207]);
    }

    // Bind IP (bytes 207-210)
    if data.len() >= 211 {
        reply.bind_ip.copy_from_slice(&data[207..211]);
    }

    // Bind Index (byte 211)
    if data.len() > 211 {
        reply.bind_index = data[211];
    }

    // Status2 (byte 212)
    if data.len() > 212 {
        reply.status2 = data[212];
    }

    Some(ArtNetPacket::PollReply(reply))
}

/// Parse ArtDmx packet
fn parse_dmx(data: &[u8]) -> Option<ArtNetPacket> {
    if data.len() < 18 {
        return None;
    }

    // Protocol version (bytes 10-11, should be 14)
    let _version = u16::from_be_bytes([data[10], data[11]]);

    // Sequence (byte 12)
    let sequence = data[12];

    // Physical port (byte 13)
    let physical = data[13];

    // Universe (bytes 14-15, little-endian) - SubUni in low byte, Net in high byte
    let sub_uni = data[14];
    let net = data[15];
    let universe = ((net as u16) << 8) | (sub_uni as u16);

    // Length (bytes 16-17, big-endian)
    let length = u16::from_be_bytes([data[16], data[17]]);

    // DMX data starts at byte 18
    let dmx_end = 18 + (length as usize).min(512);
    if data.len() < dmx_end {
        return None;
    }

    let dmx_data = data[18..dmx_end].to_vec();

    Some(ArtNetPacket::Dmx(ArtDmx {
        sequence,
        physical,
        universe,
        length,
        data: dmx_data,
    }))
}

/// Extract null-terminated string from bytes
fn extract_string(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).to_string()
}

/// Calculate the full 15-bit Art-Net universe from net, subnet, and universe
pub fn calculate_artnet_universe(net: u8, subnet: u8, universe: u8) -> u16 {
    ((net as u16 & 0x7F) << 8) | ((subnet as u16 & 0x0F) << 4) | (universe as u16 & 0x0F)
}

/// Create an ArtPoll packet for device discovery
pub fn create_artpoll_packet() -> Vec<u8> {
    let mut packet = Vec::with_capacity(14);

    // Art-Net header
    packet.extend_from_slice(ARTNET_HEADER);

    // OpCode (little-endian) - OpPoll = 0x2000
    packet.push(0x00);
    packet.push(0x20);

    // Protocol version (high byte first) - version 14
    packet.push(0x00);
    packet.push(0x0E);

    // Flags
    // Bit 1 = Send ArtPollReply when conditions change
    // Bit 0 = Deprecated, set to 0
    packet.push(0x02);

    // DiagPriority - Low priority diagnostics
    packet.push(0x10);

    packet
}
