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
    Active, // Received data within last 3 seconds
    Idle,   // No data for 3-10 seconds
    Stale,  // No data for 10+ seconds
}

/// Source direction - whether the device is sending or receiving DMX
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceDirection {
    Sending,   // Device is sending DMX data (controller/console)
    Receiving, // Device is receiving DMX data (node/fixture)
    Both,      // Device is both sending and receiving
    Unknown,   // Direction not yet determined
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
    pub direction: SourceDirection,
    pub fps: f32,

    // Statistics
    pub packet_count: u64,
    pub first_seen: u64, // Unix timestamp ms
    pub last_seen: u64,  // Unix timestamp ms

    // Diagnostics - Phase 1
    #[serde(default)]
    pub packet_loss_percent: f32,
    #[serde(default)]
    pub fps_warning: Option<String>, // "low", "high", or None
    #[serde(default)]
    pub duplicate_universes: Vec<u16>, // Universes with multiple senders
    #[serde(default)]
    pub latency_jitter_ms: f32,

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
    pub fn from_artnet(
        ip: IpAddr,
        short_name: &str,
        long_name: &str,
        mac: Option<[u8; 6]>,
    ) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mac_string = mac.map(|m| {
            format!(
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                m[0], m[1], m[2], m[3], m[4], m[5]
            )
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
            direction: SourceDirection::Unknown,
            fps: 0.0,
            packet_count: 0,
            first_seen: now_ms,
            last_seen: now_ms,
            // Diagnostics
            packet_loss_percent: 0.0,
            fps_warning: None,
            duplicate_universes: Vec::new(),
            latency_jitter_ms: 0.0,
            // Art-Net specific
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
            direction: SourceDirection::Unknown,
            fps: 0.0,
            packet_count: 0,
            first_seen: now_ms,
            last_seen: now_ms,
            // Diagnostics
            packet_loss_percent: 0.0,
            fps_warning: None,
            duplicate_universes: Vec::new(),
            latency_jitter_ms: 0.0,
            // Art-Net specific
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
        self.packet_times
            .retain(|&t| now.duration_since(t) < self.window_size);
        self.packet_times.push(now);
    }

    pub fn fps(&self) -> f32 {
        let now = Instant::now();
        let count = self
            .packet_times
            .iter()
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

/// Sequence tracker for packet loss detection
#[derive(Debug, Clone)]
pub struct SequenceTracker {
    last_sequence: Option<u8>,
    expected_packets: u64,
    received_packets: u64,
    window_start: Instant,
}

impl SequenceTracker {
    pub fn new() -> Self {
        Self {
            last_sequence: None,
            expected_packets: 0,
            received_packets: 0,
            window_start: Instant::now(),
        }
    }

    /// Record a packet and return loss percentage
    pub fn record_packet(&mut self, sequence: u8) -> f32 {
        // Reset window every 5 seconds
        let now = Instant::now();
        if now.duration_since(self.window_start) > Duration::from_secs(5) {
            self.expected_packets = 0;
            self.received_packets = 0;
            self.window_start = now;
            self.last_sequence = Some(sequence);
            return 0.0;
        }

        self.received_packets += 1;

        if let Some(last) = self.last_sequence {
            // Calculate expected packets (handling wrap-around)
            let gap = if sequence >= last {
                sequence - last
            } else {
                255 - last + sequence + 1
            };
            self.expected_packets += gap as u64;
        } else {
            self.expected_packets += 1;
        }

        self.last_sequence = Some(sequence);

        if self.expected_packets == 0 {
            0.0
        } else {
            let loss = (self.expected_packets - self.received_packets) as f32
                / self.expected_packets as f32
                * 100.0;
            loss.max(0.0).min(100.0)
        }
    }
}

impl Default for SequenceTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Latency tracker for jitter calculation
#[derive(Debug, Clone)]
pub struct LatencyTracker {
    last_packet_time: Option<Instant>,
    intervals: Vec<Duration>,
    window_size: usize,
}

impl LatencyTracker {
    pub fn new() -> Self {
        Self {
            last_packet_time: None,
            intervals: Vec::new(),
            window_size: 100, // Track last 100 intervals
        }
    }

    /// Record packet arrival and return jitter in ms
    pub fn record_packet(&mut self) -> f32 {
        let now = Instant::now();

        if let Some(last) = self.last_packet_time {
            let interval = now.duration_since(last);
            self.intervals.push(interval);

            // Keep only recent intervals
            if self.intervals.len() > self.window_size {
                self.intervals.remove(0);
            }
        }

        self.last_packet_time = Some(now);
        self.calculate_jitter()
    }

    fn calculate_jitter(&self) -> f32 {
        if self.intervals.len() < 2 {
            return 0.0;
        }

        let mean: f64 = self.intervals.iter().map(|d| d.as_secs_f64()).sum::<f64>()
            / self.intervals.len() as f64;

        let variance: f64 = self
            .intervals
            .iter()
            .map(|d| {
                let diff = d.as_secs_f64() - mean;
                diff * diff
            })
            .sum::<f64>()
            / self.intervals.len() as f64;

        (variance.sqrt() * 1000.0) as f32 // Return jitter in ms
    }
}

impl Default for LatencyTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal source tracking with timing data
struct SourceEntry {
    source: NetworkSource,
    last_packet: Instant,
    fps_counter: FpsCounter,
    sequence_tracker: SequenceTracker,
    latency_tracker: LatencyTracker,
}

/// Central source manager
pub struct SourceManager {
    sources: RwLock<HashMap<String, SourceEntry>>,
    /// Track which sources are outputting to each universe (for duplicate detection)
    universe_sources: RwLock<HashMap<u16, Vec<String>>>,
    /// FPS warning thresholds
    fps_low_threshold: f32,
    fps_high_threshold: f32,
}

impl SourceManager {
    pub fn new() -> Self {
        Self {
            sources: RwLock::new(HashMap::new()),
            universe_sources: RwLock::new(HashMap::new()),
            fps_low_threshold: 20.0,
            fps_high_threshold: 44.0,
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
        sequence: Option<u8>,
    ) {
        let id = format!("artnet-{}", ip);
        let mut sources = self.sources.write();

        let entry = sources.entry(id.clone()).or_insert_with(|| SourceEntry {
            source: NetworkSource::from_artnet(ip, short_name, long_name, mac),
            last_packet: Instant::now(),
            fps_counter: FpsCounter::new(),
            sequence_tracker: SequenceTracker::new(),
            latency_tracker: LatencyTracker::new(),
        });

        entry.last_packet = Instant::now();
        entry.fps_counter.record_packet();

        // Track sequence number for packet loss
        if let Some(seq) = sequence {
            entry.source.packet_loss_percent = entry.sequence_tracker.record_packet(seq);
        }

        // Track jitter
        entry.source.latency_jitter_ms = entry.latency_tracker.record_packet();

        entry.source.packet_count += 1;
        entry.source.fps = entry.fps_counter.fps();
        entry.source.last_seen = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        entry
            .source
            .update_status(Instant::now(), entry.last_packet);

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
        sequence: Option<u8>,
    ) {
        let cid_string = crate::network::sacn::cid_to_string(cid);
        let id = format!("sacn-{}", cid_string);
        let mut sources = self.sources.write();

        let entry = sources.entry(id.clone()).or_insert_with(|| SourceEntry {
            source: NetworkSource::from_sacn(ip, source_name, cid, priority),
            last_packet: Instant::now(),
            fps_counter: FpsCounter::new(),
            sequence_tracker: SequenceTracker::new(),
            latency_tracker: LatencyTracker::new(),
        });

        entry.last_packet = Instant::now();
        entry.fps_counter.record_packet();

        // Track sequence number for packet loss
        if let Some(seq) = sequence {
            entry.source.packet_loss_percent = entry.sequence_tracker.record_packet(seq);
        }

        // Track jitter
        entry.source.latency_jitter_ms = entry.latency_tracker.record_packet();

        entry.source.packet_count += 1;
        entry.source.fps = entry.fps_counter.fps();
        entry.source.last_seen = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        entry
            .source
            .update_status(Instant::now(), entry.last_packet);
        entry.source.sacn_priority = Some(priority);

        // Add universe
        if !entry.source.universes.contains(&universe) {
            entry.source.universes.push(universe);
            entry.source.universes.sort();
        }
    }

    /// Update or add an Art-Net source with direction info (for sniffer mode)
    pub fn update_artnet_source_with_direction(
        &self,
        ip: IpAddr,
        short_name: &str,
        long_name: &str,
        mac: Option<[u8; 6]>,
        universes: Option<Vec<u16>>,
        direction: SourceDirection,
        sequence: Option<u8>,
    ) {
        let id = format!("artnet-{}", ip);
        let mut sources = self.sources.write();

        let entry = sources.entry(id.clone()).or_insert_with(|| SourceEntry {
            source: NetworkSource::from_artnet(ip, short_name, long_name, mac),
            last_packet: Instant::now(),
            fps_counter: FpsCounter::new(),
            sequence_tracker: SequenceTracker::new(),
            latency_tracker: LatencyTracker::new(),
        });

        entry.last_packet = Instant::now();
        entry.fps_counter.record_packet();

        // Track sequence number for packet loss
        if let Some(seq) = sequence {
            entry.source.packet_loss_percent = entry.sequence_tracker.record_packet(seq);
        }

        // Track jitter
        entry.source.latency_jitter_ms = entry.latency_tracker.record_packet();

        entry.source.packet_count += 1;
        entry.source.fps = entry.fps_counter.fps();
        entry.source.last_seen = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        entry
            .source
            .update_status(Instant::now(), entry.last_packet);

        // Update direction - upgrade Unknown to specific, or to Both if conflicting
        entry.source.direction = match (entry.source.direction, direction) {
            (SourceDirection::Unknown, d) => d,
            (SourceDirection::Sending, SourceDirection::Receiving) => SourceDirection::Both,
            (SourceDirection::Receiving, SourceDirection::Sending) => SourceDirection::Both,
            (current, _) => current,
        };

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

    /// Update or add an sACN source with direction info (for sniffer mode)
    pub fn update_sacn_source_with_direction(
        &self,
        ip: IpAddr,
        source_name: &str,
        cid: &[u8; 16],
        priority: u8,
        universe: u16,
        direction: SourceDirection,
        sequence: Option<u8>,
    ) {
        // For receiving-only devices without a real CID, use IP-based ID
        let id = if cid == &[0u8; 16] {
            format!("sacn-recv-{}", ip)
        } else {
            let cid_string = crate::network::sacn::cid_to_string(cid);
            format!("sacn-{}", cid_string)
        };
        let mut sources = self.sources.write();

        let entry = sources.entry(id.clone()).or_insert_with(|| SourceEntry {
            source: NetworkSource::from_sacn(ip, source_name, cid, priority),
            last_packet: Instant::now(),
            fps_counter: FpsCounter::new(),
            sequence_tracker: SequenceTracker::new(),
            latency_tracker: LatencyTracker::new(),
        });

        entry.last_packet = Instant::now();
        entry.fps_counter.record_packet();

        // Track sequence number for packet loss
        if let Some(seq) = sequence {
            entry.source.packet_loss_percent = entry.sequence_tracker.record_packet(seq);
        }

        // Track jitter
        entry.source.latency_jitter_ms = entry.latency_tracker.record_packet();

        entry.source.packet_count += 1;
        entry.source.fps = entry.fps_counter.fps();
        entry.source.last_seen = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        entry
            .source
            .update_status(Instant::now(), entry.last_packet);
        entry.source.sacn_priority = Some(priority);

        // Update direction
        entry.source.direction = match (entry.source.direction, direction) {
            (SourceDirection::Unknown, d) => d,
            (SourceDirection::Sending, SourceDirection::Receiving) => SourceDirection::Both,
            (SourceDirection::Receiving, SourceDirection::Sending) => SourceDirection::Both,
            (current, _) => current,
        };

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

    /// Update all source statuses, FPS warnings, and duplicate detection
    pub fn update_statuses(&self) {
        let now = Instant::now();
        let mut sources = self.sources.write();

        // Build universe -> source mapping for duplicate detection
        let mut universe_map: HashMap<u16, Vec<String>> = HashMap::new();

        for (id, entry) in sources.iter_mut() {
            entry.source.update_status(now, entry.last_packet);
            entry.source.fps = entry.fps_counter.fps();

            // FPS warnings
            let fps = entry.source.fps;
            entry.source.fps_warning = if fps > 0.0 && fps < self.fps_low_threshold {
                Some("low".to_string())
            } else if fps > self.fps_high_threshold {
                Some("high".to_string())
            } else {
                None
            };

            // Track universes for duplicate detection
            for universe in &entry.source.universes {
                universe_map.entry(*universe).or_default().push(id.clone());
            }
        }

        // Store universe mapping
        *self.universe_sources.write() = universe_map.clone();

        // Update duplicate warnings on sources
        for entry in sources.values_mut() {
            entry.source.duplicate_universes.clear();
            for universe in &entry.source.universes {
                if let Some(source_ids) = universe_map.get(universe) {
                    if source_ids.len() > 1 {
                        entry.source.duplicate_universes.push(*universe);
                    }
                }
            }
        }
    }

    /// Remove stale sources (inactive for more than 60 seconds)
    pub fn cleanup_stale_sources(&self) {
        let now = Instant::now();
        let mut sources = self.sources.write();
        sources.retain(|_, entry| now.duration_since(entry.last_packet) < Duration::from_secs(60));
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
