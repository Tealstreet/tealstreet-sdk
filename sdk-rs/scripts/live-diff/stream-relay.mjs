import { mkdirSync, writeFileSync } from 'node:fs';
import https from 'node:https';
import { join } from 'node:path';
import WebSocket, { WebSocketServer } from 'ws';
import { certificateCache, hash } from './recorder.mjs';

export function streamKey(input) {
  const url = new URL(input);
  url.searchParams.delete('requestId');
  url.searchParams.delete('timestamp');
  url.searchParams.sort();
  return url.href;
}

// A single global timeline retains ordering across Binance's two sockets and
// OKX's public/business sockets. Each replay receives a fresh connection set.
export async function streamRelay({
  directory,
  ca,
  hosts,
  mode,
  tape,
  records,
  openUpstream = (url) => new WebSocket(url, { handshakeTimeout: 15000, perMessageDeflate: false, autoPong: false }),
}) {
  const certificateFor = certificateCache(directory, ca);
  const servers = [],
    sockets = new Set(),
    upstreams = new Set(),
    peers = new Map();
  const occurrences = new Map();
  let frozen = false,
    cancelled = false;
  const misses = [];
  const closePeer = (peer, code, reason) => {
    if (code === 1005)
      peer.close(); // An empty close frame, not a TCP abort.
    else if ([1006, 1015].includes(code)) peer.terminate();
    else peer.close(code, reason);
  };
  const epoch = performance.now();
  const elapsed = () => performance.now() - epoch;
  if (mode === 'capture') Object.assign(tape, { connections: [], frames: [] });
  const ports = {};
  for (const [family, address] of [
    ['v4', '127.0.0.1'],
    ['v6', '::1'],
  ]) {
    const server = https.createServer({
      SNICallback(host, cb) {
        if (!hosts.includes(host)) return cb(new Error(`Unapproved public WS host ${host}`));
        try {
          cb(null, certificateFor(host));
        } catch (error) {
          cb(error);
        }
      },
    });
    server.on('connection', (socket) => {
      sockets.add(socket);
      socket.on('close', () => sockets.delete(socket));
    });
    server.on('tlsClientError', (error) => {
      if (!cancelled) records.push({ mode, failure: error.message });
    });
    server.on('upgrade', (request, socket, head) => {
      if (mode === 'capture' && frozen) return socket.destroy();
      const url = new URL(request.url, `wss://${request.headers.host}`);
      if (!hosts.includes(url.hostname) || /\/private(?:\/|$)/.test(url.pathname)) return socket.destroy();
      const key = streamKey(url),
        occurrence = occurrences.get(key) ?? 0;
      occurrences.set(key, occurrence + 1);
      const connection =
        mode === 'capture'
          ? { id: tape.connections.length, key, occurrence, url: url.href, openedAtMs: elapsed() }
          : tape.connections.find((c) => c.key === key && c.occurrence === occurrence);
      if (mode === 'capture') tape.connections.push(connection);
      const record = { mode, key, occurrence, sent: [], delivered: 0, dropped: 0 };
      records.push(record);
      // Replay must never synthesize an extra parser-facing pong. In capture,
      // protocol ping/pong travels end-to-end and is recorded on the way back.
      const wss = new WebSocketServer({ noServer: true, perMessageDeflate: false, autoPong: false });
      wss.handleUpgrade(request, socket, head, (peer) => {
        peer.on('error', (error) => {
          if (!cancelled) record.failure = error.message;
        });
        if (!connection) {
          record.failure = 'No matching captured socket';
          peer.close();
          return;
        }
        peers.set(connection.id, peer);
        peer.on('close', () => peers.delete(connection.id));
        peer.on('message', (data, binary) =>
          record.sent.push({ base64: data.toString('base64'), binary, atMs: elapsed() }),
        );
        if (mode !== 'capture') {
          record.status = 101;
          return;
        }
        const upstream = openUpstream(url),
          queued = [];
        upstreams.add(upstream);
        upstream.on('open', () => {
          record.status = 101;
          for (const { data, binary, kind } of queued) {
            if (kind) upstream[kind](data);
            else upstream.send(data, { binary });
          }
        });
        peer.on('message', (data, binary) => {
          if (upstream.readyState === WebSocket.OPEN) upstream.send(data, { binary });
          else queued.push({ data, binary });
        });
        peer.on('ping', (data) => {
          if (upstream.readyState === WebSocket.OPEN) upstream.ping(data);
          else queued.push({ data, kind: 'ping' });
        });
        peer.on('pong', (data) => {
          if (upstream.readyState === WebSocket.OPEN) upstream.pong(data);
          else queued.push({ data, kind: 'pong' });
        });
        const frame = (kind, data, binary) => {
          if (frozen) return;
          const entry = {
            connection: connection.id,
            atMs: elapsed(),
            kind,
            base64: Buffer.from(data).toString('base64'),
          };
          if (kind === 'message') entry.binary = binary;
          tape.frames.push(entry);
          if (peer.readyState === WebSocket.OPEN) {
            if (kind === 'message') peer.send(data, { binary });
            else peer[kind](data);
            record.delivered++;
          }
        };
        upstream.on('message', (data, binary) => frame('message', data, binary));
        upstream.on('ping', (data) => frame('ping', data));
        upstream.on('pong', (data) => frame('pong', data));
        upstream.on('error', (error) => {
          record.failure = error.message;
          peer.close();
        });
        upstream.on('unexpected-response', (_req, response) => {
          record.status = response.statusCode;
          const chunks = [];
          response.on('data', (chunk) => chunks.push(chunk));
          response.on('end', () => {
            record.failure = Buffer.concat(chunks).toString().slice(0, 500);
            peer.close();
          });
        });
        upstream.on('close', (code, reason) => {
          upstreams.delete(upstream);
          if (!frozen) {
            tape.frames.push({
              connection: connection.id,
              atMs: elapsed(),
              kind: 'close',
              code,
              base64: reason.toString('base64'),
            });
            closePeer(peer, code, reason);
          }
        });
        peer.on('close', () => upstream.terminate());
      });
    });
    await new Promise((resolve, reject) => {
      server.once('error', reject);
      server.listen(0, address, resolve);
    });
    ports[family] = server.address().port;
    servers.push(server);
  }
  return {
    ports,
    async ready(timeoutMs = 10000) {
      const until = Date.now() + timeoutMs;
      while (true) {
        const first = tape.connections.filter((c) => c.occurrence === 0);
        if (first.length && first.every((c) => peers.get(c.id)?.readyState === WebSocket.OPEN)) return true;
        if (Date.now() >= until) return false;
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
    },
    freeze(captureDirectory) {
      frozen = true;
      tape.durationMs = elapsed();
      const serialized = JSON.stringify(tape);
      const digest = hash(serialized);
      if (captureDirectory) {
        mkdirSync(captureDirectory, { recursive: true });
        writeFileSync(join(captureDirectory, `${digest}.stream.json`), serialized + '\n');
      }
      return {
        hash: digest,
        frames: tape.frames.length,
        connections: tape.connections.length,
        durationMs: tape.durationMs,
        fixture: captureDirectory ? join(captureDirectory, `${digest}.stream.json`) : undefined,
      };
    },
    async play() {
      const started = performance.now();
      let maxLatenessMs = 0;
      const receipt = [];
      for (const [index, frame] of tape.frames.entries()) {
        const wait = frame.atMs - (performance.now() - started);
        if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
        if (cancelled) break;
        maxLatenessMs = Math.max(maxLatenessMs, performance.now() - started - frame.atMs);
        const peer = peers.get(frame.connection);
        const connection = tape.connections[frame.connection];
        const record = records.find((r) => r.key === connection.key && r.occurrence === connection.occurrence);
        if (!peer || peer.readyState !== WebSocket.OPEN) {
          if (record) record.dropped++;
          misses.push(index);
          continue;
        }
        const bytes = Buffer.from(frame.base64, 'base64');
        if (frame.kind === 'close') closePeer(peer, frame.code, bytes);
        else if (frame.kind === 'message') peer.send(bytes, { binary: frame.binary });
        else peer[frame.kind](bytes);
        record.delivered++;
        receipt.push(frame);
      }
      const remaining = tape.durationMs - (performance.now() - started);
      if (remaining > 0 && !cancelled) await new Promise((resolve) => setTimeout(resolve, remaining));
      return {
        delivered: receipt.length,
        dropped: misses.length,
        missingFrameIndices: misses,
        inputHash: hash(JSON.stringify(receipt)),
        expectedInputHash: hash(JSON.stringify(tape.frames)),
        durationMs: performance.now() - started,
        expectedDurationMs: tape.durationMs,
        maxLatenessMs,
      };
    },
    async close() {
      cancelled = frozen = true;
      for (const socket of upstreams) socket.terminate();
      for (const peer of peers.values()) peer.terminate();
      for (const socket of sockets) socket.destroy();
      await Promise.all(servers.map((server) => new Promise((resolve) => server.close(resolve))));
    },
  };
}
