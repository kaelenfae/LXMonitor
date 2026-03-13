#!/usr/bin/env node
/**
 * DMX Test Packet Sender — GUI Edition
 * Supports Art-Net and sACN (E1.31)
 *
 * Run:  node tools/artnet-test-sender-gui.mjs
 * Then open http://localhost:9741 in your browser.
 */

import dgram from "node:dgram";
import crypto from "node:crypto";
import http from "node:http";

const HTTP_PORT = 9741;
const ARTNET_PORT = 6454;
const SACN_PORT = 5568;

// Stable CID for this sender session
const CID = crypto.randomBytes(16);

// ── Shared: generate random DMX data ────────────────────────────────────
function randomDmx(channels) {
  channels = Math.max(1, Math.min(512, channels));
  const data = Buffer.alloc(channels);
  for (let i = 0; i < channels; i++) {
    data[i] = Math.floor(Math.random() * 256);
  }
  return data;
}

// ── Art-Net send logic ──────────────────────────────────────────────────
function sendArtNet({ universe = 0, host = "255.255.255.255", channels = 512 }) {
  return new Promise((resolve, reject) => {
    universe = Math.max(0, Math.min(32767, universe));
    channels = Math.max(1, Math.min(512, channels));
    const dmxLength = channels % 2 === 0 ? channels : channels + 1;

    const packet = Buffer.alloc(18 + dmxLength);
    packet.write("Art-Net\0", 0, 8, "ascii");
    packet.writeUInt16LE(0x5000, 8);
    packet.writeUInt16BE(14, 10);
    packet[12] = 0;
    packet[13] = 0;
    packet.writeUInt16LE(universe, 14);
    packet.writeUInt16BE(dmxLength, 16);

    const dmxData = randomDmx(channels);
    dmxData.copy(packet, 18);

    const socket = dgram.createSocket("udp4");
    socket.bind(() => {
      socket.setBroadcast(true);
      socket.send(packet, 0, packet.length, ARTNET_PORT, host, (err) => {
        socket.close();
        if (err) return reject(err);
        resolve({
          protocol: "Art-Net",
          universe, host, port: ARTNET_PORT, channels,
          preview: [...dmxData.subarray(0, 32)],
        });
      });
    });
  });
}

// ── sACN (E1.31) send logic ─────────────────────────────────────────────
let sacnSeq = 0;

function sendSACN({ universe = 1, host, channels = 512, priority = 100 }) {
  return new Promise((resolve, reject) => {
    universe = Math.max(1, Math.min(63999, universe));
    channels = Math.max(1, Math.min(512, channels));
    priority = Math.max(0, Math.min(200, priority));

    // Default host = sACN multicast for this universe
    const multicastAddr = `239.255.${(universe >> 8) & 0xff}.${universe & 0xff}`;
    const destHost = host || multicastAddr;

    const dmxData = randomDmx(channels);
    const propCount = channels + 1; // +1 for DMX start code

    // Total packet = 126 (header) + propCount
    const packetLen = 126 + propCount;
    const packet = Buffer.alloc(packetLen);
    let o = 0;

    // ── Root Layer ──────────────────────────────────────────
    // Preamble Size (2)
    packet.writeUInt16BE(0x0010, o); o += 2;
    // Post-amble Size (2)
    packet.writeUInt16BE(0x0000, o); o += 2;
    // ACN Packet Identifier (12)
    Buffer.from("ASC-E1.17\0\0\0", "ascii").copy(packet, o); o += 12;
    // Flags & Length — root layer: covers from here (offset 16) to end
    const rootLen = packetLen - 16;
    packet.writeUInt16BE(0x7000 | (rootLen & 0x0fff), o); o += 2;
    // Vector: VECTOR_ROOT_E131_DATA = 0x00000004
    packet.writeUInt32BE(0x00000004, o); o += 4;
    // CID (16)
    CID.copy(packet, o); o += 16;

    // ── Framing Layer ───────────────────────────────────────
    // Flags & Length — framing: covers from here (offset 38) to end
    const framingLen = packetLen - 38;
    packet.writeUInt16BE(0x7000 | (framingLen & 0x0fff), o); o += 2;
    // Vector: VECTOR_E131_DATA_PACKET = 0x00000002
    packet.writeUInt32BE(0x00000002, o); o += 4;
    // Source Name (64 bytes, null-padded)
    const srcName = "DMX Test Sender";
    packet.write(srcName, o, 64, "utf8"); o += 64;
    // Priority (1)
    packet[o++] = priority;
    // Synchronization Address (2)
    packet.writeUInt16BE(0, o); o += 2;
    // Sequence Number (1)
    packet[o++] = sacnSeq & 0xff;
    sacnSeq = (sacnSeq + 1) & 0xff;
    // Options (1) — 0 = normal
    packet[o++] = 0;
    // Universe (2, big-endian)
    packet.writeUInt16BE(universe, o); o += 2;

    // ── DMP Layer ───────────────────────────────────────────
    // Flags & Length — DMP: covers from here (offset 115) to end
    const dmpLen = packetLen - 115;
    packet.writeUInt16BE(0x7000 | (dmpLen & 0x0fff), o); o += 2;
    // Vector: 0x02
    packet[o++] = 0x02;
    // Address Type & Data Type: 0xa1
    packet[o++] = 0xa1;
    // First Property Address (2)
    packet.writeUInt16BE(0x0000, o); o += 2;
    // Address Increment (2)
    packet.writeUInt16BE(0x0001, o); o += 2;
    // Property Value Count (2)
    packet.writeUInt16BE(propCount, o); o += 2;
    // DMX start code (1)
    packet[o++] = 0x00;
    // DMX data
    dmxData.copy(packet, o);

    // ── Send ────────────────────────────────────────────────
    const socket = dgram.createSocket({ type: "udp4", reuseAddr: true });
    socket.bind(() => {
      // If sending to multicast, set multicast TTL
      if (destHost === multicastAddr) {
        socket.setMulticastTTL(4);
      }
      socket.setBroadcast(true);
      socket.send(packet, 0, packet.length, SACN_PORT, destHost, (err) => {
        socket.close();
        if (err) return reject(err);
        resolve({
          protocol: "sACN",
          universe, host: destHost, port: SACN_PORT, channels, priority,
          preview: [...dmxData.subarray(0, 32)],
        });
      });
    });
  });
}

// ── HTML page ───────────────────────────────────────────────────────────
const HTML = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>DMX Test Sender</title>
<style>
  @import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap');

  *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

  :root {
    --bg: #0c0e14;
    --surface: #151823;
    --surface-hover: #1c2033;
    --border: #252a3a;
    --border-focus: #6366f1;
    --text: #e2e4ec;
    --text-dim: #8a8ea0;
    --accent: #6366f1;
    --accent-glow: rgba(99, 102, 241, 0.25);
    --green: #22c55e;
    --green-glow: rgba(34, 197, 94, 0.2);
    --red: #ef4444;
    --orange: #f59e0b;
    --radius: 10px;
  }

  body {
    font-family: 'Inter', -apple-system, sans-serif;
    background: var(--bg);
    color: var(--text);
    min-height: 100vh;
    display: flex;
    justify-content: center;
    align-items: center;
    padding: 24px;
  }

  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 16px;
    padding: 36px;
    width: 100%;
    max-width: 440px;
    box-shadow: 0 8px 40px rgba(0,0,0,.4);
  }

  .header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 28px;
  }

  .logo {
    width: 40px; height: 40px;
    background: linear-gradient(135deg, var(--accent), #a78bfa);
    border-radius: 10px;
    display: flex; align-items: center; justify-content: center;
    font-size: 20px;
    flex-shrink: 0;
  }

  .header h1 {
    font-size: 18px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  .header p {
    font-size: 12px;
    color: var(--text-dim);
    margin-top: 2px;
  }

  /* ── Protocol toggle ─────────────────── */
  .proto-toggle {
    display: flex;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    margin-bottom: 18px;
  }
  .proto-toggle button {
    flex: 1;
    padding: 9px 0;
    background: transparent;
    border: none;
    border-radius: 0;
    color: var(--text-dim);
    font-family: 'JetBrains Mono', monospace;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: background .2s, color .2s;
    box-shadow: none;
    letter-spacing: 0.04em;
  }
  .proto-toggle button:hover {
    background: var(--surface-hover);
    transform: none;
    box-shadow: none;
  }
  .proto-toggle button.active {
    background: var(--accent);
    color: #fff;
  }

  /* ── Form ─────────────────────────────── */
  .field { margin-bottom: 18px; }
  .field:last-of-type { margin-bottom: 24px; }

  label {
    display: block;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin-bottom: 6px;
  }

  input, select {
    width: 100%;
    padding: 10px 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text);
    font-family: 'JetBrains Mono', monospace;
    font-size: 14px;
    transition: border-color .2s, box-shadow .2s;
    outline: none;
    -webkit-appearance: none;
  }
  input:focus, select:focus {
    border-color: var(--border-focus);
    box-shadow: 0 0 0 3px var(--accent-glow);
  }
  input::placeholder { color: var(--text-dim); opacity: 0.5; }

  .row { display: flex; gap: 14px; }
  .row .field { flex: 1; }

  .field.hidden { display: none; }

  /* ── Button ───────────────────────────── */
  .send-btn {
    width: 100%;
    padding: 12px;
    background: linear-gradient(135deg, var(--accent), #818cf8);
    border: none;
    border-radius: var(--radius);
    color: #fff;
    font-family: 'Inter', sans-serif;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
    transition: transform .12s, box-shadow .2s, opacity .2s;
    letter-spacing: 0.01em;
  }
  .send-btn:hover { transform: translateY(-1px); box-shadow: 0 4px 20px var(--accent-glow); }
  .send-btn:active { transform: translateY(0); }
  .send-btn:disabled { opacity: .5; cursor: not-allowed; transform: none; }

  /* ── Result log ───────────────────────── */
  .log {
    margin-top: 20px;
    max-height: 220px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .log-entry {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 14px;
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
    line-height: 1.6;
    animation: fadeIn .25s ease;
  }
  .log-entry.ok { border-left: 3px solid var(--green); }
  .log-entry.err { border-left: 3px solid var(--red); }

  .log-entry .tag {
    font-weight: 600;
    margin-right: 4px;
  }
  .log-entry .tag.ok { color: var(--green); }
  .log-entry .tag.err { color: var(--red); }
  .log-entry .dim { color: var(--text-dim); }
  .log-entry .proto-label {
    display: inline-block;
    font-size: 10px;
    font-weight: 600;
    padding: 1px 5px;
    border-radius: 4px;
    margin-right: 4px;
    vertical-align: middle;
  }
  .log-entry .proto-label.artnet { background: #312e81; color: #a5b4fc; }
  .log-entry .proto-label.sacn   { background: #064e3b; color: #6ee7b7; }

  .channel-preview {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
    margin-top: 6px;
  }
  .ch-bar {
    width: 10px;
    border-radius: 2px;
    background: var(--accent);
    opacity: 0.8;
    transition: height .3s;
  }
  .ch-bar.sacn { background: #10b981; }

  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .log::-webkit-scrollbar { width: 5px; }
  .log::-webkit-scrollbar-track { background: transparent; }
  .log::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }
</style>
</head>
<body>
<div class="card">
  <div class="header">
    <div class="logo">⚡</div>
    <div>
      <h1>DMX Test Sender</h1>
      <p>Send random DMX data via Art-Net or sACN</p>
    </div>
  </div>

  <div class="proto-toggle">
    <button class="active" onclick="setProto('artnet')">Art-Net</button>
    <button onclick="setProto('sacn')">sACN</button>
  </div>

  <div class="row">
    <div class="field">
      <label for="universe">Universe</label>
      <input type="number" id="universe" value="0" min="0" max="32767">
    </div>
    <div class="field">
      <label for="channels">Channels</label>
      <input type="number" id="channels" value="512" min="1" max="512">
    </div>
  </div>

  <div class="field" id="hostField">
    <label for="host" id="hostLabel">Destination IP</label>
    <input type="text" id="host" value="255.255.255.255" placeholder="255.255.255.255">
  </div>

  <div class="row">
    <div class="field hidden" id="priorityField">
      <label for="priority">Priority</label>
      <input type="number" id="priority" value="100" min="0" max="200">
    </div>
  </div>

  <button class="send-btn" id="sendBtn" onclick="send()">Send Packet</button>

  <div class="log" id="log"></div>
</div>

<script>
let protocol = 'artnet';

const defaults = {
  artnet: { host: '255.255.255.255', placeholder: '255.255.255.255', univMin: 0, univMax: 32767, univDefault: 0 },
  sacn:   { host: '',                placeholder: 'auto-multicast',  univMin: 1, univMax: 63999, univDefault: 1 },
};

function setProto(p) {
  protocol = p;
  const btns = document.querySelectorAll('.proto-toggle button');
  btns.forEach(b => b.classList.remove('active'));
  btns[p === 'artnet' ? 0 : 1].classList.add('active');

  const d = defaults[p];
  const univInput = document.getElementById('universe');
  univInput.min = d.univMin;
  univInput.max = d.univMax;
  univInput.value = d.univDefault;

  const hostInput = document.getElementById('host');
  hostInput.value = d.host;
  hostInput.placeholder = d.placeholder;
  document.getElementById('hostLabel').textContent = p === 'sacn' ? 'Destination IP (blank = multicast)' : 'Destination IP';

  document.getElementById('priorityField').classList.toggle('hidden', p !== 'sacn');
}

async function send() {
  const btn = document.getElementById('sendBtn');
  btn.disabled = true;
  btn.textContent = 'Sending…';

  const body = {
    protocol,
    universe: parseInt(document.getElementById('universe').value, 10) || defaults[protocol].univDefault,
    channels: parseInt(document.getElementById('channels').value, 10) || 512,
    host: document.getElementById('host').value || '',
  };
  if (protocol === 'sacn') {
    body.priority = parseInt(document.getElementById('priority').value, 10) || 100;
  }

  try {
    const res = await fetch('/send', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    const data = await res.json();
    if (data.ok) {
      const extra = data.priority !== undefined ? '  pri ' + data.priority : '';
      addLog('ok', data.protocol,
        'Universe ' + data.universe + ' → ' + data.host + ':' + data.port + '  ·  ' + data.channels + ' ch' + extra,
        data.preview, data.protocol.toLowerCase());
    } else {
      addLog('err', 'Error', data.error);
    }
  } catch (e) {
    addLog('err', 'Error', e.message);
  }

  btn.disabled = false;
  btn.textContent = 'Send Packet';
}

function addLog(type, tag, msg, preview, proto) {
  const log = document.getElementById('log');
  const el = document.createElement('div');
  el.className = 'log-entry ' + type;

  let html = '';
  if (type === 'ok') {
    const pClass = (proto || 'artnet').replace('-', '').toLowerCase();
    html += '<span class="proto-label ' + pClass + '">' + tag + '</span> ';
  } else {
    html += '<span class="tag ' + type + '">' + tag + '</span> ';
  }
  html += msg;

  if (preview && preview.length) {
    const barClass = (proto || '').includes('sacn') || (proto || '').includes('sACN') ? 'sacn' : '';
    html += '<div class="channel-preview">';
    const max = 40;
    for (let i = 0; i < Math.min(preview.length, 32); i++) {
      const h = Math.max(3, Math.round((preview[i] / 255) * max));
      html += '<div class="ch-bar ' + barClass + '" style="height:' + h + 'px"></div>';
    }
    html += '</div>';
  }

  el.innerHTML = html;
  log.prepend(el);
  while (log.children.length > 20) log.removeChild(log.lastChild);
}

document.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !document.getElementById('sendBtn').disabled) send();
});
</script>
</body>
</html>`;

// ── HTTP server ─────────────────────────────────────────────────────────
const server = http.createServer(async (req, res) => {
  if (req.method === "GET" && (req.url === "/" || req.url === "/index.html")) {
    res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
    res.end(HTML);
    return;
  }

  if (req.method === "POST" && req.url === "/send") {
    let body = "";
    for await (const chunk of req) body += chunk;
    try {
      const params = JSON.parse(body);
      const result = params.protocol === "sacn"
        ? await sendSACN(params)
        : await sendArtNet(params);
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ ok: true, ...result }));
    } catch (err) {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ ok: false, error: err.message }));
    }
    return;
  }

  res.writeHead(404);
  res.end("Not found");
});

server.listen(HTTP_PORT, () => {
  console.log(`\n  ⚡ DMX Test Sender GUI`);
  console.log(`  ─────────────────────`);
  console.log(`  Open http://localhost:${HTTP_PORT}\n`);
});
