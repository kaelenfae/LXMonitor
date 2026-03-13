#!/usr/bin/env node
/**
 * Art-Net Test Packet Sender
 * Sends a single ArtDmx packet with random channel intensities
 * to the specified universe on the broadcast address.
 *
 * Usage:
 *   node artnet-test-sender.mjs [options]
 *
 * Options:
 *   -u, --universe <n>    Art-Net universe (0-32767, default: 0)
 *   -h, --host <ip>       Destination IP (default: 255.255.255.255)
 *   -c, --channels <n>    Number of channels to send (1-512, default: 512)
 *   --help                Show this help message
 */

import dgram from "node:dgram";
import { parseArgs } from "node:util";

// ── CLI args ────────────────────────────────────────────────────────────
const { values } = parseArgs({
  options: {
    universe: { type: "string", short: "u", default: "0" },
    host:     { type: "string", short: "h", default: "255.255.255.255" },
    channels: { type: "string", short: "c", default: "512" },
    help:     { type: "boolean", default: false },
  },
  strict: true,
});

if (values.help) {
  console.log(`
Art-Net Test Packet Sender
──────────────────────────
Sends one ArtDmx packet with random channel intensities.

Options:
  -u, --universe <n>    Art-Net universe  (0–32767, default: 0)
  -h, --host <ip>       Destination IP    (default: 255.255.255.255)
  -c, --channels <n>    Channel count     (1–512,   default: 512)
      --help            Show this help
`);
  process.exit(0);
}

const ARTNET_PORT = 6454;
const universe = Math.max(0, Math.min(32767, parseInt(values.universe, 10)));
const host = values.host;
const channelCount = Math.max(1, Math.min(512, parseInt(values.channels, 10)));
// ArtDmx length must be even
const dmxLength = channelCount % 2 === 0 ? channelCount : channelCount + 1;

// ── Build ArtDmx packet ─────────────────────────────────────────────────
//  0- 7  "Art-Net\0"                      (8 bytes)
//  8- 9  OpCode 0x5000 (little-endian)    (2 bytes)
// 10-11  Protocol version 14 (big-endian) (2 bytes)
// 12     Sequence                         (1 byte)
// 13     Physical port                    (1 byte)
// 14-15  Universe (little-endian)         (2 bytes)
// 16-17  Length (big-endian)              (2 bytes)
// 18+    DMX data

const packet = Buffer.alloc(18 + dmxLength);

// Header
packet.write("Art-Net\0", 0, 8, "ascii");
// OpCode – ArtDmx = 0x5000 (little-endian)
packet.writeUInt16LE(0x5000, 8);
// Protocol version 14 (big-endian)
packet.writeUInt16BE(14, 10);
// Sequence (0 = disabled)
packet[12] = 0;
// Physical port
packet[13] = 0;
// Universe (little-endian, 15-bit: low 8 = SubUni, high 7 = Net)
packet.writeUInt16LE(universe, 14);
// Data length (big-endian)
packet.writeUInt16BE(dmxLength, 16);

// Fill DMX data with random intensities
const dmxData = Buffer.alloc(dmxLength);
for (let i = 0; i < channelCount; i++) {
  dmxData[i] = Math.floor(Math.random() * 256);
}
dmxData.copy(packet, 18);

// ── Send ─────────────────────────────────────────────────────────────────
const socket = dgram.createSocket("udp4");
socket.bind(() => {
  socket.setBroadcast(true);
  socket.send(packet, 0, packet.length, ARTNET_PORT, host, (err) => {
    if (err) {
      console.error("❌ Send failed:", err.message);
    } else {
      console.log(`✅ Art-Net packet sent!`);
      console.log(`   Universe : ${universe}`);
      console.log(`   Host     : ${host}:${ARTNET_PORT}`);
      console.log(`   Channels : ${channelCount}`);
      console.log(`   Data (first 16 ch): [${[...dmxData.subarray(0, 16)].join(", ")}]`);
    }
    socket.close();
  });
});
