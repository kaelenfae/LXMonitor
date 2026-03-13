# Changelog

## [0.1.1] - 2026-03-12

### Fixed
- **Event listener stability** — Tauri event listeners (`sources-updated`, `dmx-updated`) no longer tear down and re-register every time a source is selected, fixing potential missed events
- **Duplicate application state** — Removed a second `AppState` instance with a disconnected `is_listening` mutex that could cause state inconsistencies
- **sACN performance** — Removed debug logging that fired on every sACN packet, eliminating thousands of unnecessary console writes per second on active networks
- **Heatmap accuracy** — Fixed stale closure in DMX data fetching that caused channel activity comparisons to always use initial empty state instead of the previous frame
- **Latency tracking performance** — Switched `LatencyTracker` from `Vec` to `VecDeque` for O(1) interval pruning (was O(n) on every packet)
- **FPS calculation** — Simplified `FpsCounter::fps()` to avoid redundant re-filtering of already-pruned data

### Removed
- Unused `dns-lookup` dependency (reduces build time and binary size)
- Dead `trigger_artnet_poll()` function that would panic if called outside Tokio runtime
- Duplicated channel history/activity tracking logic (consolidated into the event-driven handler)

## [0.1.0] - Initial Release

### Features
- Art-Net 4 & sACN (E1.31) protocol support
- Real-time 512-channel DMX monitoring with grid, graph, and heatmap views
- Automatic source discovery with Art-Net polling
- Network diagnostics (FPS, jitter, packet loss, duplicate universe detection)
- Multi-universe dashboard
- Channel history graphing
- Optional sniffer mode (requires Npcap)
- 11 themes including accessibility options (CVD, dyslexic font, high contrast)
- CSV data export
