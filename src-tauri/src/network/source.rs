// Source Tracking - Manages discovered network sources

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Protocol type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    ArtNet,
    #[serde(rename = "sACN")]
    Sacn,
}

/// Source status based on last activity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceStatus {
    Active,   // Received data within last 3 seconds
    Idle,     // No data for 3-10 seconds
    Stale,    // No data for 10+ seconds
}

/// Represents a discovered network source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSource {
    pub id: String,
    pub ip: String,
    pub hostname: Option<String>,
    pub name: String,
    pub protocol: Protocol,
    pub universes: Vec<u16>,
    pub status: SourceStatus,
    pub fps: f32,
    
    // Statistics
    pub packet_count: u64,
    pub first_seen: u64,     // Unix timestamp ms
    pub last_seen: u64,      // Unix timestamp ms
    
    // Art-Net specific
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artnet_short_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artnet_long_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
    
    // sACN specific
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sacn_cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sacn_priority: Option<u8>,
}

impl NetworkSource {
    /// Create a new source from Art-Net discovery
    pub fn from_artnet(ip: IpAddr, short_name: &str, long_name: &str, mac: Option<[u8; 6]>) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        
        let mac_string = mac.map(|m| {
            format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                m[0], m[1], m[2], m[3], m[4], m[5])
        });
        
        let name = if !long_name.is_empty() {
            long_name.to_string()
        } else if !short_name.is_empty() {
            short_name.to_string()
        } else {
            format!("ArtNet @ {}", ip)
        };
        
        Self {
            id: format!("artnet-{}", ip),
            ip: ip.to_string(),
            hostname: None,
            name,
            protocol: Protocol::ArtNet,
            universes: Vec::new(),
            status: SourceStatus::Active,
            fps: 0.0,
            packet_count: 0,
            first_seen: now_ms,
            last_seen: now_ms,
            artnet_short_name: Some(short_name.to_string()),
            artnet_long_name: Some(long_name.to_string()),
            mac_address: mac_string,
            sacn_cid: None,
            sacn_priority: None,
        }
    }
    
    /// Create a new source from sACN discovery
    pub fn from_sacn(ip: IpAddr, source_name: &str, cid: &[u8; 16], priority: u8) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        
        let cid_string = crate::network::sacn::cid_to_string(cid);
        
        let name = if !source_name.is_empty() {
            source_name.to_string()
        } else {
            format!("sACN @ {}", ip)
        };
        
        Self {
            id: format!("sacn-{}", cid_string),
            ip: ip.to_string(),
            hostname: None,
            name,
            protocol: Protocol::Sacn,
            universes: Vec::new(),
            status: SourceStatus::Active,
            fps: 0.0,
            packet_count: 0,
            first_seen: now_ms,
            last_seen: now_ms,
            artnet_short_name: None,
            artnet_long_name: None,
            mac_address: None,
            sacn_cid: Some(cid_string),
            sacn_priority: Some(priority),
        }
    }
    
    /// Update source status based on time since last seen
    pub fn update_status(&mut self, now: Instant, last_packet: Instant) {
        let elapsed = now.duration_since(last_packet);
        self.status = if elapsed < Duration::from_secs(3) {
            SourceStatus::Active
        } else if elapsed < Duration::from_secs(10) {
            SourceStatus::Idle
        } else {
            SourceStatus::Stale
        };
    }
}

/// FPS calculator for a single universe
#[derive(Debug, Clone)]
pub struct FpsCounter {
    packet_times: Vec<Instant>,
    window_size: Duration,
}

impl FpsCounter {
    pub fn new() -> Self {
        Self {
            packet_times: Vec::new(),
            window_size: Duration::from_secs(1),
        }
    }
    
    pub fn record_packet(&mut self) {
        let now = Instant::now();
        // Remove old packets outside the window
        self.packet_times.retain(|&t| now.duration_since(t) < self.window_size);
        self.packet_times.push(now);
    }
    
    pub fn fps(&self) -> f32 {
        let now = Instant::now();
        let count = self.packet_times.iter()
            .filter(|&&t| now.duration_since(t) < self.window_size)
            .count();
        count as f32
    }
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal source tracking with timing data
struct SourceEntry {
    source: NetworkSource,
    last_packet: Instant,
    fps_counter: FpsCounter,
}

/// Central source manager
pub struct SourceManager {
    sources: RwLock<HashMap<String, SourceEntry>>,
}

impl SourceManager {
    pub fn new() -> Self {
        Self {
            sources: RwLock::new(HashMap::new()),
        }
    }
    
    /// Update or add an Art-Net source
    pub fn update_artnet_source(
        &self,
        ip: IpAddr,
        short_name: &str,
        long_name: &str,
        mac: Option<[u8; 6]>,
        universes: Option<Vec<u16>>,
    ) {
        let id = format!("artnet-{}", ip);
        let mut sources = self.sources.write();
        
        let entry = sources.entry(id.clone()).or_insert_with(|| {
            SourceEntry {
                source: NetworkSource::from_artnet(ip, short_name, long_name, mac),
                last_packet: Instant::now(),
                fps_counter: FpsCounter::new(),
            }
        });
        
        entry.last_packet = Instant::now();
        entry.fps_counter.record_packet();
        entry.source.packet_count += 1;
        entry.source.fps = entry.fps_counter.fps();
        entry.source.last_seen = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        entry.source.update_status(Instant::now(), entry.last_packet);
        
        // Update universes if provided
        if let Some(univs) = universes {
            for u in univs {
                if !entry.source.universes.contains(&u) {
                    entry.source.universes.push(u);
                    entry.source.universes.sort();
                }
            }
        }
    }
    
    /// Update or add an sACN source
    pub fn update_sacn_source(
        &self,
        ip: IpAddr,
        source_name: &str,
        cid: &[u8; 16],
        priority: u8,
        universe: u16,
    ) {
        let cid_string = crate::network::sacn::cid_to_string(cid);
        let id = format!("sacn-{}", cid_string);
        let mut sources = self.sources.write();
        
        let entry = sources.entry(id.clone()).or_insert_with(|| {
            SourceEntry {
                source: NetworkSource::from_sacn(ip, source_name, cid, priority),
                last_packet: Instant::now(),
                fps_counter: FpsCounter::new(),
            }
        });
        
        entry.last_packet = Instant::now();
        entry.fps_counter.record_packet();
        entry.source.packet_count += 1;
        entry.source.fps = entry.fps_counter.fps();
        entry.source.last_seen = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        entry.source.update_status(Instant::now(), entry.last_packet);
        entry.source.sacn_priority = Some(priority);
        
        // Add universe
        if !entry.source.universes.contains(&universe) {
            entry.source.universes.push(universe);
            entry.source.universes.sort();
        }
    }
    
    /// Get all sources as a vector
    pub fn get_all_sources(&self) -> Vec<NetworkSource> {
        let sources = self.sources.read();
        sources.values().map(|e| e.source.clone()).collect()
    }
    
    /// Update all source statuses
    pub fn update_statuses(&self) {
        let now = Instant::now();
        let mut sources = self.sources.write();
        for entry in sources.values_mut() {
            entry.source.update_status(now, entry.last_packet);
            entry.source.fps = entry.fps_counter.fps();
        }
    }
    
    /// Remove stale sources (inactive for more than 60 seconds)
    pub fn cleanup_stale_sources(&self) {
        let now = Instant::now();
        let mut sources = self.sources.write();
        sources.retain(|_, entry| {
            now.duration_since(entry.last_packet) < Duration::from_secs(60)
        });
    }
}

impl Default for SourceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe source manager handle
pub type SourceManagerHandle = Arc<SourceManager>;

/// Create a new source manager handle
pub fn create_source_manager() -> SourceManagerHandle {
    Arc::new(SourceManager::new())
}
