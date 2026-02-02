import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

// Available themes (matching LXLog)
const THEMES = [
  { id: 'dark', name: 'Dark', preview: '#0f0f12' },
  { id: 'light', name: 'Light', preview: '#fdf2f0' },
  { id: 'midnight', name: 'Midnight', preview: '#0a0e1a' },
  { id: 'forest', name: 'Forest', preview: '#0a120e' },
  { id: 'sunset', name: 'Sunset', preview: '#1a1410' },
  { id: 'ocean', name: 'Ocean', preview: '#0a1214' },
  { id: 'lavender', name: 'Lavender', preview: '#1a1420' },
  { id: 'snes', name: 'SNES', preview: '#2a2a34' },
  { id: 'colorblind', name: 'CVD', preview: '#0f1214' },
  { id: 'dyslexic', name: 'Dyslexic', preview: '#f5f5f0' },
  { id: 'hotpink', name: 'Hot Pink', preview: '#ff1493' },
];

// Channel color mode options
const COLOR_MODES = [
  { id: 'level', name: 'Level', description: 'Color by DMX value' },
  { id: 'source', name: 'Source', description: 'Color by source' },
  { id: 'lastUsed', name: 'Last Used', description: 'Fade based on activity' },
  { id: 'unused', name: 'Unused', description: 'Highlight unused channels' },
];

// View mode options
const VIEW_MODES = [
  { id: 'grid', name: 'Grid', icon: '⊞' },
  { id: 'graph', name: 'Graph', icon: '📈' },
];

// Graph colors for multiple channels
const GRAPH_COLORS = [
  '#6366f1', '#06b6d4', '#10b981', '#f59e0b', '#ef4444',
  '#8b5cf6', '#ec4899', '#14b8a6', '#f97316', '#84cc16',
];

// Source colors
const SOURCE_COLORS = [
  '#6366f1', '#06b6d4', '#10b981', '#f59e0b', '#ef4444',
  '#8b5cf6', '#ec4899', '#14b8a6', '#f97316', '#84cc16',
];

// Channel Graph Component
function ChannelGraph({ universe, channelData, trackedChannels, onRemoveChannel, timeWindow = 30 }) {
  const canvasRef = useRef(null);
  const dataHistoryRef = useRef({});
  const [currentTime, setCurrentTime] = useState(Date.now());

  // Update time every 100ms for smooth animation
  useEffect(() => {
    const interval = setInterval(() => {
      setCurrentTime(Date.now());
    }, 100);
    return () => clearInterval(interval);
  }, []);

  // Store data history
  useEffect(() => {
    if (!channelData) return;

    const now = Date.now();
    const cutoff = now - (timeWindow * 1000);

    trackedChannels.forEach(ch => {
      if (!dataHistoryRef.current[ch]) {
        dataHistoryRef.current[ch] = [];
      }

      // Add new data point
      dataHistoryRef.current[ch].push({
        time: now,
        value: channelData[ch - 1] || 0
      });

      // Remove old data points
      dataHistoryRef.current[ch] = dataHistoryRef.current[ch].filter(
        point => point.time > cutoff
      );
    });
  }, [channelData, trackedChannels, timeWindow]);

  // Draw graph
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    const rect = canvas.getBoundingClientRect();

    // Handle high DPI displays
    const dpr = window.devicePixelRatio || 1;
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    const width = rect.width;
    const height = rect.height;
    const padding = { top: 20, right: 20, bottom: 30, left: 50 };
    const graphWidth = width - padding.left - padding.right;
    const graphHeight = height - padding.top - padding.bottom;

    // Clear canvas
    ctx.clearRect(0, 0, width, height);

    // Draw background grid
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.1)';
    ctx.lineWidth = 1;

    // Horizontal grid lines (DMX values)
    for (let i = 0; i <= 4; i++) {
      const y = padding.top + (i / 4) * graphHeight;
      ctx.beginPath();
      ctx.moveTo(padding.left, y);
      ctx.lineTo(width - padding.right, y);
      ctx.stroke();

      // Value labels
      ctx.fillStyle = 'rgba(255, 255, 255, 0.5)';
      ctx.font = '11px monospace';
      ctx.textAlign = 'right';
      ctx.fillText(String(255 - (i * 64)), padding.left - 8, y + 4);
    }

    // Vertical grid lines (time)
    const now = currentTime;
    for (let i = 0; i <= timeWindow; i += 5) {
      const x = padding.left + ((timeWindow - i) / timeWindow) * graphWidth;
      ctx.beginPath();
      ctx.moveTo(x, padding.top);
      ctx.lineTo(x, height - padding.bottom);
      ctx.stroke();

      // Time labels
      if (i > 0) {
        ctx.fillStyle = 'rgba(255, 255, 255, 0.5)';
        ctx.font = '11px monospace';
        ctx.textAlign = 'center';
        ctx.fillText(`-${i}s`, x, height - padding.bottom + 15);
      }
    }

    // "Now" label
    ctx.fillText('now', width - padding.right, height - padding.bottom + 15);

    // Draw data lines for each tracked channel
    trackedChannels.forEach((ch, index) => {
      const history = dataHistoryRef.current[ch] || [];
      if (history.length < 2) return;

      const color = GRAPH_COLORS[index % GRAPH_COLORS.length];

      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.beginPath();

      let started = false;
      history.forEach((point, i) => {
        const x = padding.left + ((point.time - (now - timeWindow * 1000)) / (timeWindow * 1000)) * graphWidth;
        const y = padding.top + ((255 - point.value) / 255) * graphHeight;

        if (x < padding.left) return;

        if (!started) {
          ctx.moveTo(x, y);
          started = true;
        } else {
          ctx.lineTo(x, y);
        }
      });

      ctx.stroke();

      // Draw current value dot
      const lastPoint = history[history.length - 1];
      if (lastPoint) {
        const x = padding.left + ((lastPoint.time - (now - timeWindow * 1000)) / (timeWindow * 1000)) * graphWidth;
        const y = padding.top + ((255 - lastPoint.value) / 255) * graphHeight;

        ctx.fillStyle = color;
        ctx.beginPath();
        ctx.arc(x, y, 5, 0, Math.PI * 2);
        ctx.fill();
      }
    });

  }, [trackedChannels, currentTime, timeWindow]);

  return (
    <div className="channel-graph">
      <div className="graph-header">
        <h3>Channel History</h3>
        <div className="graph-legend">
          {trackedChannels.map((ch, index) => (
            <div key={ch} className="legend-item">
              <span
                className="legend-color"
                style={{ background: GRAPH_COLORS[index % GRAPH_COLORS.length] }}
              />
              <span className="legend-label">Ch {ch}</span>
              <span className="legend-value">
                {channelData ? channelData[ch - 1] || 0 : 0}
              </span>
              <button
                className="legend-remove"
                onClick={() => onRemoveChannel(ch)}
                title="Remove from graph"
              >
                ✕
              </button>
            </div>
          ))}
        </div>
      </div>
      <canvas ref={canvasRef} className="graph-canvas" />
      {trackedChannels.length === 0 && (
        <div className="graph-empty">
          <p>Click on channels in the grid to add them to the graph</p>
        </div>
      )}
    </div>
  );
}

// Settings Modal Component
function SettingsModal({
  isOpen,
  onClose,
  theme,
  setTheme,
  accessibility,
  setAccessibility,
  networkInterfaces,
  selectedInterface,
  onInterfaceChange,
  protocolFilter,
  onProtocolChange,
  // Sniffer props
  snifferEnabled,
  onSnifferToggle,
  npcapAvailable,
  captureInterfaces,
  selectedCaptureInterface,
  onCaptureInterfaceChange,
  snifferStatus
}) {
  if (!isOpen) return null;

  const handleThemeChange = (themeId) => {
    THEMES.forEach(t => {
      if (t.id !== 'dark') {
        document.documentElement.classList.remove(t.id);
      }
    });
    if (themeId !== 'dark') {
      document.documentElement.classList.add(themeId);
    }
    setTheme(themeId);
    localStorage.setItem('lxmonitor-theme', themeId);
  };

  const handleAccessibilityChange = (key, value) => {
    const newAccessibility = { ...accessibility, [key]: value };
    setAccessibility(newAccessibility);
    localStorage.setItem('lxmonitor-accessibility', JSON.stringify(newAccessibility));
    if (value) {
      document.documentElement.classList.add(key);
    } else {
      document.documentElement.classList.remove(key);
    }
  };

  return (
    <div className="settings-overlay" onClick={onClose}>
      <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header">
          <h2>Settings</h2>
          <button className="settings-close" onClick={onClose}>✕</button>
        </div>
        <div className="settings-content">
          {/* Network Interface Section */}
          <div className="settings-section">
            <h3>Network Interface</h3>
            <div className="interface-selector">
              <select
                value={selectedInterface}
                onChange={(e) => onInterfaceChange(e.target.value)}
                className="interface-select"
              >
                {networkInterfaces.map((iface) => (
                  <option key={iface.ip} value={iface.ip}>
                    {iface.name} ({iface.ip})
                  </option>
                ))}
              </select>
              <p className="interface-note">
                Select which network adapter to listen on. "All Interfaces" listens on all available adapters.
              </p>
            </div>
          </div>

          {/* Protocol Filter Section */}
          <div className="settings-section">
            <h3>Protocol</h3>
            <div className="protocol-filter">
              <div className="protocol-options">
                <label className={`protocol-option ${protocolFilter === 'both' ? 'active' : ''}`}>
                  <input
                    type="radio"
                    name="protocol"
                    value="both"
                    checked={protocolFilter === 'both'}
                    onChange={(e) => onProtocolChange(e.target.value)}
                  />
                  <span className="protocol-label">Both</span>
                </label>
                <label className={`protocol-option ${protocolFilter === 'artnet' ? 'active' : ''}`}>
                  <input
                    type="radio"
                    name="protocol"
                    value="artnet"
                    checked={protocolFilter === 'artnet'}
                    onChange={(e) => onProtocolChange(e.target.value)}
                  />
                  <span className="protocol-label">Art-Net</span>
                </label>
                <label className={`protocol-option ${protocolFilter === 'sacn' ? 'active' : ''}`}>
                  <input
                    type="radio"
                    name="protocol"
                    value="sacn"
                    checked={protocolFilter === 'sacn'}
                    onChange={(e) => onProtocolChange(e.target.value)}
                  />
                  <span className="protocol-label">sACN</span>
                </label>
              </div>
              <p className="interface-note">
                Filter which protocols to listen for. Use "Both" for full network monitoring.
              </p>
            </div>
          </div>

          {/* Sniffer Mode Section */}
          <div className="settings-section">
            <h3>Sniffer Mode (Advanced)</h3>
            <div className="sniffer-settings">
              {!npcapAvailable ? (
                <div className="sniffer-warning">
                  <span className="warning-icon">⚠️</span>
                  <div>
                    <p><strong>Npcap Not Installed</strong></p>
                    <p className="description">Sniffer mode requires Npcap to be installed. <a href="https://npcap.com/" target="_blank" rel="noopener noreferrer">Download Npcap</a></p>
                  </div>
                </div>
              ) : (
                <>
                  <div className="accessibility-toggle">
                    <div>
                      <label>Enable Sniffer Mode</label>
                      <div className="description">Capture all network traffic (requires admin)</div>
                    </div>
                    <label className="toggle-switch">
                      <input
                        type="checkbox"
                        checked={snifferEnabled}
                        onChange={(e) => onSnifferToggle(e.target.checked)}
                      />
                      <span className="toggle-slider"></span>
                    </label>
                  </div>
                  {snifferEnabled && (
                    <div className="interface-selector" style={{ marginTop: '12px' }}>
                      <label>Capture Interface</label>
                      <select
                        value={selectedCaptureInterface}
                        onChange={(e) => onCaptureInterfaceChange(e.target.value)}
                        className="interface-select"
                      >
                        {captureInterfaces.map((iface) => (
                          <option key={iface.name} value={iface.name}>
                            {iface.description || iface.name}
                          </option>
                        ))}
                      </select>
                    </div>
                  )}
                  {snifferStatus?.packets_captured > 0 && (
                    <p className="interface-note" style={{ marginTop: '8px' }}>
                      Packets captured: {snifferStatus.packets_captured.toLocaleString()}
                    </p>
                  )}
                  {snifferStatus?.error && (
                    <p className="interface-note" style={{ color: 'var(--error)', marginTop: '8px' }}>
                      Error: {snifferStatus.error}
                    </p>
                  )}
                </>
              )}
              <p className="interface-note" style={{ marginTop: '12px' }}>
                Sniffer mode allows seeing traffic to other IPs. Requires port mirroring on managed switches.
              </p>
            </div>
          </div>
          <div className="settings-section">
            <h3>Theme</h3>
            <div className="theme-grid">
              {THEMES.map((t) => (
                <div
                  key={t.id}
                  className={`theme-option ${theme === t.id ? 'active' : ''}`}
                  onClick={() => handleThemeChange(t.id)}
                >
                  <div className="preview" style={{ background: t.preview }}></div>
                  <span>{t.name}</span>
                </div>
              ))}
            </div>
          </div>

          {/* Accessibility Section */}
          <div className="settings-section">
            <h3>Accessibility</h3>
            <div className="accessibility-options">
              <div className="accessibility-toggle">
                <div>
                  <label>Dyslexic Font</label>
                  <div className="description">Use OpenDyslexic font</div>
                </div>
                <label className="toggle-switch">
                  <input
                    type="checkbox"
                    checked={accessibility['dyslexic-mode'] || false}
                    onChange={(e) => handleAccessibilityChange('dyslexic-mode', e.target.checked)}
                  />
                  <span className="toggle-slider"></span>
                </label>
              </div>
              <div className="accessibility-toggle">
                <div>
                  <label>Reduced Motion</label>
                  <div className="description">Disable animations</div>
                </div>
                <label className="toggle-switch">
                  <input
                    type="checkbox"
                    checked={accessibility['reduced-motion'] || false}
                    onChange={(e) => handleAccessibilityChange('reduced-motion', e.target.checked)}
                  />
                  <span className="toggle-slider"></span>
                </label>
              </div>
              <div className="accessibility-toggle">
                <div>
                  <label>High Contrast</label>
                  <div className="description">Increase visual contrast</div>
                </div>
                <label className="toggle-switch">
                  <input
                    type="checkbox"
                    checked={accessibility['high-contrast'] || false}
                    onChange={(e) => handleAccessibilityChange('high-contrast', e.target.checked)}
                  />
                  <span className="toggle-slider"></span>
                </label>
              </div>
              <div className="accessibility-toggle">
                <div>
                  <label>Large Text</label>
                  <div className="description">Increase font size 20%</div>
                </div>
                <label className="toggle-switch">
                  <input
                    type="checkbox"
                    checked={accessibility['large-text'] || false}
                    onChange={(e) => handleAccessibilityChange('large-text', e.target.checked)}
                  />
                  <span className="toggle-slider"></span>
                </label>
              </div>
            </div>
          </div>

          {/* About Section */}
          <div className="settings-section">
            <h3>About</h3>
            <div className="about-info">
              <p><strong>LXMonitor</strong> v0.1.0</p>
              <p className="description">Universal Art-Net / sACN Network Monitor</p>
              <p className="about-link">
                Part of the <a href="https://lxlog.netlify.app" target="_blank" rel="noopener noreferrer">LXLog</a> family
              </p>
              <p className="about-link" style={{ marginTop: '16px', fontSize: '11px', opacity: 0.7 }}>
                Created with the help of Google Antigravity
              </p>
              <p className="about-link" style={{ marginTop: '12px' }}>
                <a href="https://ko-fi.com/lxlog" target="_blank" rel="noopener noreferrer">Support on Ko-fi ☕</a>
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// Source Card Component
function SourceCard({ source, isActive, onClick }) {
  const getStatusClass = (status) => {
    switch (status) {
      case 'active': return 'active';
      case 'idle': return 'idle';
      default: return 'stale';
    }
  };

  return (
    <div
      className={`source-card ${isActive ? 'active' : ''}`}
      onClick={onClick}
    >
      <div className="source-card-header">
        <span className="source-name">{source.name || 'Unknown Source'}</span>
        <div className="source-status">
          <span className={`status-dot ${getStatusClass(source.status)}`}></span>
          <span>{source.status}</span>
        </div>
      </div>
      <div className="source-details">
        <div className="source-detail">
          <span className="label">IP</span>
          <span className="value">{source.ip}</span>
        </div>
        <div className="source-detail">
          <span className="label">Protocol</span>
          <span className={`source-protocol ${source.protocol.toLowerCase()}`}>
            {source.protocol}
          </span>
        </div>
        <div className="source-detail">
          <span className="label">Universes</span>
          <span className="value">
            {source.universes.length > 0 ? source.universes.slice(0, 8).join(', ') : 'None'}
            {source.universes.length > 8 ? ` (+${source.universes.length - 8})` : ''}
          </span>
        </div>
        {source.fps > 0 && (
          <div className="source-detail">
            <span className="label">Rate</span>
            <span className="value">{Math.round(source.fps)} fps</span>
          </div>
        )}
        {source.mac_address && (
          <div className="source-detail">
            <span className="label">MAC</span>
            <span className="value">{source.mac_address}</span>
          </div>
        )}
        {source.sacn_priority && (
          <div className="source-detail">
            <span className="label">Priority</span>
            <span className="value">{source.sacn_priority}</span>
          </div>
        )}
      </div>
    </div>
  );
}

// Channel Tooltip Component
function ChannelTooltip({ channel, value, source, position }) {
  if (!position) return null;

  return (
    <div
      className="channel-tooltip"
      style={{
        left: position.x,
        top: position.y,
      }}
    >
      <div className="tooltip-header">
        <span className="tooltip-channel">Channel {channel}</span>
        <span className="tooltip-value">{value}</span>
      </div>
      {source && (
        <div className="tooltip-source">
          <span className="label">Source:</span>
          <span className="value">{source.name || source.ip}</span>
        </div>
      )}
      {source && (
        <div className="tooltip-detail">
          <span className="label">IP:</span>
          <span className="value">{source.ip}</span>
        </div>
      )}
      {source && (
        <div className="tooltip-detail">
          <span className="label">Protocol:</span>
          <span className={`source-protocol ${source.protocol?.toLowerCase()}`}>
            {source.protocol}
          </span>
        </div>
      )}
      <div className="tooltip-hint">Click to add to graph</div>
    </div>
  );
}

// Universe Viewer Component
function UniverseViewer({
  universe,
  data,
  stats,
  allUniverses,
  onUniverseChange,
  colorMode,
  onColorModeChange,
  viewMode,
  onViewModeChange,
  sources,
  channelHistory,
  trackedChannels,
  onToggleChannel,
  theme
}) {
  const [hoveredChannel, setHoveredChannel] = useState(null);
  const [tooltipPos, setTooltipPos] = useState(null);
  const gridRef = useRef(null);

  const channels = Array.from({ length: 512 }, (_, i) => ({
    number: i + 1,
    value: data ? data[i] || 0 : 0
  }));

  const activeChannels = data ? data.filter(v => v > 0).length : 0;

  // Find source for this universe
  const universeSource = sources.find(s => s.universes.includes(universe));
  const sourceColorIndex = sources.indexOf(universeSource);

  // Get channel color based on mode
  const getChannelColor = (channel) => {
    const value = channel.value;

    switch (colorMode) {
      case 'source':
        if (value > 0 && universeSource) {
          return SOURCE_COLORS[sourceColorIndex % SOURCE_COLORS.length];
        }
        return null;

      case 'level':
        if (value > 0) {
          // Check for CVD mode
          if (theme === 'colorblind') {
            // Use single color opacity/intensity instead of hue sweep
            const alpha = 0.3 + (value / 255) * 0.7;
            return `rgba(238, 119, 51, ${alpha})`; // Orange (#ee7733)
          }

          const hue = 240 - (value / 255) * 240;
          return `hsl(${hue}, 80%, 50%)`;
        }
        return null;

      case 'lastUsed':
        const history = channelHistory[channel.number];
        if (history) {
          const elapsed = Date.now() - history.lastActive;
          const fadeTime = 5000;
          const opacity = Math.max(0.2, 1 - (elapsed / fadeTime));
          if (value > 0) {
            return `rgba(99, 102, 241, ${opacity})`;
          }
        }
        return value > 0 ? 'var(--accent-primary)' : null;

      case 'unused':
        if (value === 0) {
          // Check for CVD mode
          if (theme === 'colorblind') {
            return `var(--error)`; // Use theme error color (Magenta)
          }
          return 'rgba(239, 68, 68, 0.3)';
        }
        return 'var(--success)';

      default:
        return null;
    }
  };

  const handleMouseEnter = (e, channel) => {
    const rect = e.target.getBoundingClientRect();
    const gridRect = gridRef.current?.getBoundingClientRect() || { left: 0, top: 0 };

    setHoveredChannel(channel);
    setTooltipPos({
      x: rect.left - gridRect.left + rect.width / 2,
      y: rect.top - gridRect.top - 10
    });
  };

  const handleMouseLeave = () => {
    setHoveredChannel(null);
    setTooltipPos(null);
  };

  const handleChannelClick = (channel) => {
    onToggleChannel(channel.number);
  };

  return (
    <div className="universe-viewer">
      <div className="universe-header">
        <div className="universe-header-left">
          <h2 className="universe-title">Universe</h2>
          <select
            className="universe-select"
            value={universe}
            onChange={(e) => onUniverseChange(Number(e.target.value))}
          >
            {allUniverses.map(u => (
              <option key={u} value={u}>
                {u}
              </option>
            ))}
          </select>
        </div>

        <div className="universe-controls">
          {/* View mode toggle */}
          <div className="view-mode-toggle">
            {VIEW_MODES.map(mode => (
              <button
                key={mode.id}
                className={`view-mode-btn ${viewMode === mode.id ? 'active' : ''}`}
                onClick={() => onViewModeChange(mode.id)}
                title={mode.name}
              >
                {mode.icon}
              </button>
            ))}
          </div>

          {viewMode === 'grid' && (
            <div className="color-mode-selector">
              <label>Color:</label>
              <select
                value={colorMode}
                onChange={(e) => onColorModeChange(e.target.value)}
              >
                {COLOR_MODES.map(mode => (
                  <option key={mode.id} value={mode.id}>{mode.name}</option>
                ))}
              </select>
            </div>
          )}

          <div className="universe-stats">
            <div className="stat">
              <span className="stat-value">{stats?.fps || 0}</span>
              <span className="stat-label">fps</span>
            </div>
            <div className="stat">
              <span className="stat-value">{activeChannels}</span>
              <span className="stat-label">active</span>
            </div>
            <div className="stat">
              <span className="stat-value">{stats?.packets?.toLocaleString() || 0}</span>
              <span className="stat-label">packets</span>
            </div>
          </div>
        </div>
      </div>

      {/* Source info bar */}
      {universeSource && (
        <div className="universe-source-bar">
          <span className="source-indicator" style={{
            background: SOURCE_COLORS[sourceColorIndex % SOURCE_COLORS.length]
          }}></span>
          <span className="source-name">{universeSource.name || 'Unknown'}</span>
          <span className="source-ip">{universeSource.ip}</span>
          <span className={`source-protocol ${universeSource.protocol.toLowerCase()}`}>
            {universeSource.protocol}
          </span>
        </div>
      )}

      {/* Grid View */}
      {viewMode === 'grid' && (
        <div className="channel-grid" ref={gridRef}>
          {channels.map((channel) => {
            const customColor = getChannelColor(channel);
            const isTracked = trackedChannels.includes(channel.number);
            return (
              <div
                key={channel.number}
                className={`channel-cell ${channel.value > 0 ? 'has-value' : ''} ${isTracked ? 'tracked' : ''}`}
                onMouseEnter={(e) => handleMouseEnter(e, channel)}
                onMouseLeave={handleMouseLeave}
                onClick={() => handleChannelClick(channel)}
                style={customColor ? { '--channel-color': customColor } : {}}
              >
                <div
                  className="channel-value"
                  style={customColor ? {
                    background: customColor,
                    opacity: channel.value > 0 ? 0.3 + (channel.value / 255) * 0.7 : 1
                  } : {
                    opacity: channel.value > 0 ? 0.3 + (channel.value / 255) * 0.7 : 1
                  }}
                >
                  {channel.value > 0 ? channel.value : channel.number}
                </div>
              </div>
            );
          })}

          {hoveredChannel && (
            <ChannelTooltip
              channel={hoveredChannel.number}
              value={hoveredChannel.value}
              source={universeSource}
              position={tooltipPos}
            />
          )}
        </div>
      )}

      {/* Graph View */}
      {viewMode === 'graph' && (
        <ChannelGraph
          universe={universe}
          channelData={data}
          trackedChannels={trackedChannels}
          onRemoveChannel={(ch) => onToggleChannel(ch)}
        />
      )}
    </div>
  );
}

// Empty State Component
function EmptyState({ isListening }) {
  return (
    <div className="empty-state">
      <div className="empty-state-icon">📡</div>
      <h3>{isListening ? 'Listening...' : 'No Sources Detected'}</h3>
      <p>
        {isListening
          ? 'Waiting for Art-Net and sACN traffic on the network.'
          : 'Start listening to detect sources on the network.'
        }
      </p>
      {isListening && <div className="loading-spinner" style={{ margin: '20px auto' }}></div>}
    </div>
  );
}

// Main App Component
function App() {
  const [theme, setTheme] = useState('dark');
  const [accessibility, setAccessibility] = useState({});
  const [showSettings, setShowSettings] = useState(false);
  const [sources, setSources] = useState([]);
  const [selectedSource, setSelectedSource] = useState(null);
  const [selectedUniverse, setSelectedUniverse] = useState(null);
  const [dmxData, setDmxData] = useState({});
  const [isListening, setIsListening] = useState(true);
  const [universeStats, setUniverseStats] = useState({});
  const [colorMode, setColorMode] = useState('level');
  const [viewMode, setViewMode] = useState('grid');
  const [channelHistory, setChannelHistory] = useState({});
  const [trackedChannels, setTrackedChannels] = useState([]);
  const [networkInterfaces, setNetworkInterfaces] = useState([{ name: 'All Interfaces', ip: '0.0.0.0' }]);
  const [selectedInterface, setSelectedInterface] = useState('0.0.0.0');
  const [protocolFilter, setProtocolFilter] = useState('both');

  // Sniffer mode state
  const [snifferEnabled, setSnifferEnabled] = useState(false);
  const [snifferStatus, setSnifferStatus] = useState(null);
  const [captureInterfaces, setCaptureInterfaces] = useState([]);
  const [selectedCaptureInterface, setSelectedCaptureInterface] = useState('');
  const [npcapAvailable, setNpcapAvailable] = useState(false);

  // Device tab state (all, sending, receiving)
  const [deviceTab, setDeviceTab] = useState('all');

  // Get all universes from all sources
  const allUniverses = [...new Set(sources.flatMap(s => s.universes))].sort((a, b) => a - b);

  // Load saved settings on mount
  useEffect(() => {
    const savedTheme = localStorage.getItem('lxmonitor-theme') || 'dark';
    const savedAccessibility = JSON.parse(localStorage.getItem('lxmonitor-accessibility') || '{}');
    const savedColorMode = localStorage.getItem('lxmonitor-colormode') || 'level';
    const savedViewMode = localStorage.getItem('lxmonitor-viewmode') || 'grid';
    const savedTrackedChannels = JSON.parse(localStorage.getItem('lxmonitor-trackedchannels') || '[]');

    if (savedTheme !== 'dark') {
      document.documentElement.classList.add(savedTheme);
    }
    setTheme(savedTheme);

    Object.entries(savedAccessibility).forEach(([key, value]) => {
      if (value) {
        document.documentElement.classList.add(key);
      }
    });
    setAccessibility(savedAccessibility);
    setColorMode(savedColorMode);
    setViewMode(savedViewMode);
    setTrackedChannels(savedTrackedChannels);

    // Fetch network interfaces
    const fetchInterfaces = async () => {
      try {
        const interfaces = await invoke('get_network_interfaces');
        const allOption = { name: 'All Interfaces', ip: '0.0.0.0' };
        setNetworkInterfaces([allOption, ...interfaces]);
      } catch (err) {
        console.error('Failed to fetch network interfaces:', err);
      }
    };
    fetchInterfaces();

    // Fetch sniffer info
    const fetchSnifferInfo = async () => {
      try {
        const available = await invoke('check_npcap_available');
        setNpcapAvailable(available);

        if (available) {
          const interfaces = await invoke('get_capture_interfaces');
          setCaptureInterfaces(interfaces);
          if (interfaces.length > 0) {
            setSelectedCaptureInterface(interfaces[0].name);
          }
        }

        const status = await invoke('get_sniffer_status');
        setSnifferStatus(status);
        setSnifferEnabled(status.enabled);
      } catch (err) {
        console.error('Failed to fetch sniffer info:', err);
      }
    };
    fetchSnifferInfo();
  }, []);

  // Save color mode
  const handleColorModeChange = (mode) => {
    setColorMode(mode);
    localStorage.setItem('lxmonitor-colormode', mode);
  };

  // Save view mode
  const handleViewModeChange = (mode) => {
    setViewMode(mode);
    localStorage.setItem('lxmonitor-viewmode', mode);
  };

  // Toggle channel tracking for graph
  const handleToggleChannel = (channelNum) => {
    setTrackedChannels(prev => {
      const newChannels = prev.includes(channelNum)
        ? prev.filter(c => c !== channelNum)
        : [...prev, channelNum].slice(-10); // Max 10 channels
      localStorage.setItem('lxmonitor-trackedchannels', JSON.stringify(newChannels));
      return newChannels;
    });
  };

  // Handle interface change
  const handleInterfaceChange = async (ip) => {
    setSelectedInterface(ip);
    localStorage.setItem('lxmonitor-interface', ip);
    // Note: Would need backend support to actually rebind listeners
    console.log('Selected interface:', ip);
  };

  // Handle protocol filter change
  const handleProtocolChange = (protocol) => {
    setProtocolFilter(protocol);
    localStorage.setItem('lxmonitor-protocol', protocol);
    // Note: Would need backend support to actually filter listeners
    console.log('Protocol filter:', protocol);
  };

  // Handle sniffer mode toggle
  const handleSnifferToggle = async (enabled) => {
    try {
      await invoke('set_sniffer_mode', {
        enabled,
        interface: enabled ? selectedCaptureInterface : null
      });
      setSnifferEnabled(enabled);

      // Refresh status after a short delay
      setTimeout(async () => {
        const status = await invoke('get_sniffer_status');
        setSnifferStatus(status);
      }, 500);
    } catch (err) {
      console.error('Failed to toggle sniffer mode:', err);
      alert('Failed to toggle sniffer mode: ' + err);
    }
  };

  // Handle capture interface change
  const handleCaptureInterfaceChange = (interfaceName) => {
    setSelectedCaptureInterface(interfaceName);
    // If sniffer is running, restart with new interface
    if (snifferEnabled) {
      handleSnifferToggle(false).then(() => {
        setTimeout(() => handleSnifferToggle(true), 100);
      });
    }
  };

  // Filter sources based on device tab
  const filteredSources = sources.filter(source => {
    if (deviceTab === 'all') return true;
    if (deviceTab === 'sending') return source.direction === 'sending' || source.direction === 'both';
    if (deviceTab === 'receiving') return source.direction === 'receiving' || source.direction === 'both';
    return true;
  });

  // Fetch sources from backend
  const fetchSources = useCallback(async () => {
    try {
      const result = await invoke('get_sources');
      setSources(result);

      if (result.length > 0 && !selectedSource) {
        setSelectedSource(result[0]);
        if (result[0].universes.length > 0) {
          setSelectedUniverse(result[0].universes[0]);
        }
      }

      if (selectedSource) {
        const updated = result.find(s => s.id === selectedSource.id);
        if (updated) {
          setSelectedSource(updated);
        }
      }
    } catch (err) {
      console.error('Failed to fetch sources:', err);
    }
  }, [selectedSource]);

  // Fetch DMX data for selected universe
  const fetchDmxData = useCallback(async (universe) => {
    try {
      const result = await invoke('get_dmx_data', { universe });
      if (result) {
        setDmxData(prev => ({ ...prev, [universe]: result }));

        // Update channel history for last-used mode
        const now = Date.now();
        setChannelHistory(prev => {
          const updated = { ...prev };
          result.forEach((value, index) => {
            if (value > 0) {
              updated[index + 1] = { lastActive: now, lastValue: value };
            }
          });
          return updated;
        });
      }
    } catch (err) {
      console.error('Failed to fetch DMX data:', err);
    }
  }, []);

  // Set up event listeners
  useEffect(() => {
    const unlistenSources = listen('sources-updated', (event) => {
      setSources(event.payload);
    });

    const unlistenDmx = listen('dmx-updated', async (event) => {
      const { universe } = event.payload;
      const result = await invoke('get_dmx_data', { universe });
      if (result) {
        setDmxData(prev => ({ ...prev, [universe]: result }));

        // Update channel history
        const now = Date.now();
        setChannelHistory(prev => {
          const updated = { ...prev };
          result.forEach((value, index) => {
            if (value > 0) {
              updated[index + 1] = { lastActive: now, lastValue: value };
            }
          });
          return updated;
        });
      }

      setUniverseStats(prev => ({
        ...prev,
        [universe]: {
          ...prev[universe],
          packets: (prev[universe]?.packets || 0) + 1,
          lastUpdate: Date.now()
        }
      }));
    });

    fetchSources();

    return () => {
      unlistenSources.then(fn => fn());
      unlistenDmx.then(fn => fn());
    };
  }, [fetchSources]);

  // Fetch DMX data when selected universe changes
  useEffect(() => {
    if (selectedUniverse) {
      fetchDmxData(selectedUniverse);
    }
  }, [selectedUniverse, fetchDmxData]);

  // Handle source selection
  const handleSourceSelect = (source) => {
    setSelectedSource(source);
    if (source.universes.length > 0) {
      setSelectedUniverse(source.universes[0]);
    }
  };

  const getUniverseStats = (universe) => {
    const source = sources.find(s => s.universes.includes(universe));
    return {
      fps: source?.fps ? Math.round(source.fps) : 0,
      packets: universeStats[universe]?.packets || 0
    };
  };

  return (
    <div className="app">
      {/* Header */}
      <header className="header">
        <div className="header-title">
          <h1>LXMonitor</h1>
          <span className="subtitle">Art-Net / sACN</span>
          <span className={`connection-status ${isListening ? 'listening' : ''}`}>
            {isListening ? '● Listening' : '○ Stopped'}
          </span>
        </div>
        <div className="header-controls">
          <button className="settings-btn" onClick={() => setShowSettings(true)}>
            ⚙️ Settings
          </button>
        </div>
      </header>

      {/* Sidebar - Source List */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <h2>Devices</h2>
          <span className="source-count">{filteredSources.length}</span>
        </div>

        {/* Device Tabs */}
        <div className="devices-tabs">
          <button
            className={`devices-tab-btn ${deviceTab === 'all' ? 'active' : ''}`}
            onClick={() => setDeviceTab('all')}
          >
            All
          </button>
          <button
            className={`devices-tab-btn ${deviceTab === 'sending' ? 'active' : ''}`}
            onClick={() => setDeviceTab('sending')}
          >
            Sending
          </button>
          <button
            className={`devices-tab-btn ${deviceTab === 'receiving' ? 'active' : ''}`}
            onClick={() => setDeviceTab('receiving')}
          >
            Receiving
          </button>
        </div>

        <div className="source-list">
          {filteredSources.length === 0 ? (
            <div className="sidebar-empty">
              <p>{sources.length === 0 ? 'No devices detected yet...' : `No ${deviceTab} devices`}</p>
            </div>
          ) : (
            filteredSources.map((source) => (
              <SourceCard
                key={source.id}
                source={source}
                isActive={selectedSource?.id === source.id}
                onClick={() => handleSourceSelect(source)}
              />
            ))
          )}
        </div>
      </aside>

      {/* Main Content */}
      <main className="content">
        {allUniverses.length > 0 && selectedUniverse ? (
          <UniverseViewer
            universe={selectedUniverse}
            data={dmxData[selectedUniverse]}
            stats={getUniverseStats(selectedUniverse)}
            allUniverses={allUniverses}
            onUniverseChange={setSelectedUniverse}
            colorMode={colorMode}
            onColorModeChange={handleColorModeChange}
            viewMode={viewMode}
            onViewModeChange={handleViewModeChange}
            sources={sources}
            channelHistory={channelHistory}
            trackedChannels={trackedChannels}
            onToggleChannel={handleToggleChannel}
            theme={theme}
          />
        ) : (
          <EmptyState isListening={isListening} />
        )}
      </main>

      {/* Settings Modal */}
      <SettingsModal
        isOpen={showSettings}
        onClose={() => setShowSettings(false)}
        theme={theme}
        setTheme={setTheme}
        accessibility={accessibility}
        setAccessibility={setAccessibility}
        networkInterfaces={networkInterfaces}
        selectedInterface={selectedInterface}
        onInterfaceChange={handleInterfaceChange}
        protocolFilter={protocolFilter}
        onProtocolChange={handleProtocolChange}
        // Sniffer props
        snifferEnabled={snifferEnabled}
        onSnifferToggle={handleSnifferToggle}
        npcapAvailable={npcapAvailable}
        captureInterfaces={captureInterfaces}
        selectedCaptureInterface={selectedCaptureInterface}
        onCaptureInterfaceChange={handleCaptureInterfaceChange}
        snifferStatus={snifferStatus}
      />
    </div>
  );
}

export default App;
