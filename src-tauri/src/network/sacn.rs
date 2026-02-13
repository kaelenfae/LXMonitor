// sACN (E1.31) Protocol Implementation
// ANSI E1.31 - 2018 Streaming ACN Protocol

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// sACN constants
pub const SACN_PORT: u16 = 5568;
pub const ACN_PACKET_IDENTIFIER: &[u8] = &[
    0x41, 0x53, 0x43, 0x2d, 0x45, 0x31, 0x2e, 0x31, 0x37, 0x00, 0x00, 0x00,
]; // "ASC-E1.17\0\0\0"

/// sACN root layer vectors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum RootVector {
    Data = 0x00000004,     // E131_DATA_PACKET
    Extended = 0x00000008, // E131_EXTENDED_PACKET
    Unknown = 0xFFFFFFFF,
}

impl From<u32> for RootVector {
    fn from(value: u32) -> Self {
        match value {
            0x00000004 => RootVector::Data,
            0x00000008 => RootVector::Extended,
            _ => RootVector::Unknown,
        }
    }
}

// Framing layer vector constants (not an enum due to context-dependent values)
pub const FRAMING_VECTOR_DMP: u32 = 0x00000002;
pub const FRAMING_VECTOR_SYNC: u32 = 0x00000001;

/// Source information from sACN packets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SacnSource {
    pub cid: [u8; 16],       // Component Identifier (UUID)
    pub source_name: String, // Source name (UTF-8, 64 bytes max)
    pub priority: u8,        // Priority (0-200, default 100)
    pub sync_address: u16,   // Synchronization universe
    pub sequence: u8,        // Sequence number
    pub options: u8,         // Options flags
    pub universe: u16,       // Universe number
}

impl Default for SacnSource {
    fn default() -> Self {
        Self {
            cid: [0; 16],
            source_name: String::new(),
            priority: 100,
            sync_address: 0,
            sequence: 0,
            options: 0,
            universe: 1,
        }
    }
}

/// Parsed sACN DMX data
#[derive(Debug, Clone)]
pub struct SacnDmx {
    pub source: SacnSource,
    pub start_code: u8,
    pub data: Vec<u8>,
}

/// Parsed sACN Universe Discovery packet
#[derive(Debug, Clone)]
pub struct SacnDiscovery {
    pub cid: [u8; 16],
    pub source_name: String,
    pub universes: Vec<u16>,
}

/// Result of parsing an sACN packet
#[derive(Debug, Clone)]
pub enum SacnPacket {
    Dmx(SacnDmx),
    Sync { sync_address: u16 },
    Discovery(SacnDiscovery),
    Unknown,
}

/// Parse an sACN packet from raw bytes
pub fn parse_sacn_packet(data: &[u8], _source: SocketAddr) -> Option<SacnPacket> {
    // Minimum packet size for root layer
    if data.len() < 38 {
        return None;
    }

    // Check for ACN packet identifier (bytes 4-15)
    if data.len() < 16 || &data[4..16] != ACN_PACKET_IDENTIFIER {
        return None;
    }

    // Root layer preamble size (bytes 0-1, should be 0x0010)
    let preamble = u16::from_be_bytes([data[0], data[1]]);
    if preamble != 0x0010 {
        return None;
    }

    // Post-amble size (bytes 2-3, should be 0x0000)
    let postamble = u16::from_be_bytes([data[2], data[3]]);
    if postamble != 0x0000 {
        return None;
    }

    // Root layer flags and length (bytes 16-17)
    // let flags_length = u16::from_be_bytes([data[16], data[17]]);

    // Root layer vector (bytes 18-21)
    let root_vector = u32::from_be_bytes([data[18], data[19], data[20], data[21]]);
    let root_vector = RootVector::from(root_vector);

    // CID (bytes 22-37)
    let mut cid = [0u8; 16];
    cid.copy_from_slice(&data[22..38]);

    match root_vector {
        RootVector::Data => parse_data_packet(data, cid),
        RootVector::Extended => parse_extended_packet(data, cid),
        RootVector::Unknown => Some(SacnPacket::Unknown),
    }
}

/// Parse sACN data packet (contains DMX or sync)
fn parse_data_packet(data: &[u8], cid: [u8; 16]) -> Option<SacnPacket> {
    // Minimum size for framing layer
    if data.len() < 115 {
        return None;
    }

    // Framing layer starts at byte 38
    // Framing flags and length (bytes 38-39)
    // let framing_flags_length = u16::from_be_bytes([data[38], data[39]]);

    // Framing vector (bytes 40-43)
    let framing_vector = u32::from_be_bytes([data[40], data[41], data[42], data[43]]);

    // Source name (bytes 44-107, 64 bytes, UTF-8)
    let source_name = extract_string(&data[44..108]);

    // Priority (byte 108)
    let priority = data[108];

    // Sync address (bytes 109-110)
    let sync_address = u16::from_be_bytes([data[109], data[110]]);

    // Sequence (byte 111)
    let sequence = data[111];

    // Options (byte 112)
    let options = data[112];

    // Universe (bytes 113-114)
    let universe = u16::from_be_bytes([data[113], data[114]]);

    if framing_vector == 0x00000001 {
        // Sync packet
        return Some(SacnPacket::Sync { sync_address });
    }

    // DMP layer starts at byte 115
    if data.len() < 126 {
        return None;
    }

    // DMP flags and length (bytes 115-116)
    // let dmp_flags_length = u16::from_be_bytes([data[115], data[116]]);

    // DMP vector (byte 117, should be 0x02 for SET_PROPERTY)
    let dmp_vector = data[117];
    if dmp_vector != 0x02 {
        return Some(SacnPacket::Unknown);
    }

    // Address type & data type (byte 118)
    // First address (bytes 119-120)
    // Address increment (bytes 121-122)
    // Property count (bytes 123-124)
    let property_count = u16::from_be_bytes([data[123], data[124]]) as usize;

    // Start code (byte 125)
    // Only process packets with start code 0 (standard DMX512 data)
    // Non-zero start codes indicate alternative data types (e.g., RDM, text packets)
    // Ignoring non-zero start codes fixes flashing issues with ETC Ion consoles
    let start_code = data[125];
    if start_code != 0 {
        println!(
            "[sACN DEBUG] Ignoring packet with non-zero start code: {} (priority: {}, universe: {})",
            start_code, priority, universe
        );
        return Some(SacnPacket::Unknown);
    }

    // DMX data starts at byte 126
    let dmx_length = (property_count.saturating_sub(1))
        .min(512)
        .min(data.len() - 126);
    let dmx_data = data[126..126 + dmx_length].to_vec();

    let source = SacnSource {
        cid,
        source_name,
        priority,
        sync_address,
        sequence,
        options,
        universe,
    };

    Some(SacnPacket::Dmx(SacnDmx {
        source,
        start_code,
        data: dmx_data,
    }))
}

/// Parse sACN extended packet (contains discovery)
fn parse_extended_packet(data: &[u8], cid: [u8; 16]) -> Option<SacnPacket> {
    // Extended packets contain universe discovery
    if data.len() < 120 {
        return None;
    }

    // Framing layer starts at byte 38
    // Framing vector (bytes 40-43)
    let framing_vector = u32::from_be_bytes([data[40], data[41], data[42], data[43]]);

    if framing_vector != 0x00000002 {
        // Not a discovery packet
        return Some(SacnPacket::Unknown);
    }

    // Source name (bytes 44-107)
    let source_name = extract_string(&data[44..108]);

    // Universe discovery layer starts at byte 112
    // Discovery flags and length (bytes 112-113)
    // let discovery_flags_length = u16::from_be_bytes([data[112], data[113]]);

    // Discovery vector (bytes 114-117)
    // let discovery_vector = u32::from_be_bytes([data[114], data[115], data[116], data[117]]);

    // Page (byte 118)
    // let page = data[118];

    // Last page (byte 119)
    // let last_page = data[119];

    // Universe list starts at byte 120
    let mut universes = Vec::new();
    let mut offset = 120;
    while offset + 1 < data.len() {
        let universe = u16::from_be_bytes([data[offset], data[offset + 1]]);
        if universe != 0 {
            universes.push(universe);
        }
        offset += 2;
    }

    Some(SacnPacket::Discovery(SacnDiscovery {
        cid,
        source_name,
        universes,
    }))
}

/// Extract null-terminated UTF-8 string from bytes
fn extract_string(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).to_string()
}

/// Calculate sACN multicast address for a universe
/// Format: 239.255.{high byte}.{low byte}
pub fn sacn_multicast_address(universe: u16) -> std::net::Ipv4Addr {
    std::net::Ipv4Addr::new(239, 255, (universe >> 8) as u8, (universe & 0xFF) as u8)
}

/// CID to string (UUID format)
pub fn cid_to_string(cid: &[u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        cid[0], cid[1], cid[2], cid[3],
        cid[4], cid[5],
        cid[6], cid[7],
        cid[8], cid[9],
        cid[10], cid[11], cid[12], cid[13], cid[14], cid[15]
    )
}
