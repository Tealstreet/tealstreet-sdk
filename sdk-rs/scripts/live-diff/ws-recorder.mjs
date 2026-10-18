import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import https from 'node:https';
import { join } from 'node:path';
import WebSocket, { WebSocketServer } from 'ws';
import { certificateCache, hash } from './recorder.mjs';

export function buildConnectHook(directory, source) {
  const library = join(directory, process.platform === 'darwin' ? 'ws-connect.dylib' : 'ws-connect.so');
  if (!['darwin', 'linux'].includes(process.platform)) throw new Error('WS recording supports macOS and Linux');
  execFileSync(
    'cc',
    [
      process.platform === 'darwin' ? '-dynamiclib' : '-shared',
      '-fPIC',
      '-Wall',
      '-Wextra',
      '-Werror',
      source,
      '-o',
      library,
      ...(process.platform === 'linux' ? ['-ldl'] : []),
    ],
    { stdio: 'pipe' },
  );
  return library;
}

export function connectHookEnvironment(library, ports) {
  return {
    [process.platform === 'darwin' ? 'DYLD_INSERT_LIBRARIES' : 'LD_PRELOAD']: library,
    SAFE_CEX_PROBE_TLS_PORT: String(ports.v4),
    SAFE_CEX_PROBE_TLS_PORT6: String(ports.v6),
  };
}

// Native WebSockets do not use HTTPS_PROXY. The child-only connect hook brings
// their unmodified TLS sockets here; SNI and peer certificates still validate.
// One real TS stream is frozen, then its exact text/binary messages are replayed
// through the native socket and production venue handlers.
export async function websocketRecorder({
  directory,
  ca,
  hosts,
  implementation,
  streams,
  records,
  captureDirectory,
  openUpstream = (url) => new WebSocket(url, { handshakeTimeout: 15000, perMessageDeflate: false }),
}) {
  const certificateFor = certificateCache(directory, ca);
  const peers = new Set(),
    upstreams = new Set(),
    servers = [],
    connections = new Set(),
    timers = new Set();
  let frozen = false;
  const serverOptions = {
    SNICallback(host, callback) {
      if (!hosts.includes(host)) return callback(new Error('unapproved public WS host'));
      try {
        callback(null, certificateFor(host));
      } catch (error) {
        callback(error);
      }
    },
  };
  const accept = (request, socket, head) => {
    const url = new URL(request.url, `wss://${request.headers.host}`);
    if (!hosts.includes(url.hostname)) return socket.destroy();
    // Keep business query parameters; only correlation/time inputs are omitted.
    const keyUrl = new URL(url);
    keyUrl.searchParams.delete('requestId');
    keyUrl.searchParams.delete('timestamp');
    const key = keyUrl.href;
    const server = new WebSocketServer({ noServer: true, perMessageDeflate: false });
    server.handleUpgrade(request, socket, head, (peer) => {
      peers.add(peer);
      peer.on('error', () => {});
      peer.on('close', () => peers.delete(peer));
      const record = {
        implementation,
        protocol: 'websocket',
        host: url.hostname,
        path: url.pathname,
        outboundMessages: 0,
        inboundMessages: 0,
        replay: implementation === 'rust',
      };
      records.push(record);
      const openedAt = performance.now();
      record.sent = [];
      if (implementation === 'ts' && !frozen) {
        const frames = [];
        streams.set(key, frames);
        const queued = [];
        const upstream = openUpstream(url);
        upstreams.add(upstream);
        upstream.on('open', () => {
          record.status = 101;
          for (const bytes of queued) upstream.send(bytes.data, { binary: bytes.binary });
        });
        peer.on('message', (data, binary) => {
          record.outboundMessages++;
          record.sent.push(Buffer.from(data).toString('base64'));
          if (upstream.readyState === WebSocket.OPEN) upstream.send(data, { binary });
          else queued.push({ data, binary });
        });
        upstream.on('message', (data, binary) => {
          if (frozen) return;
          const bytes = Buffer.from(data);
          frames.push({ bytes, binary, afterMs: performance.now() - openedAt });
          record.inboundMessages++;
          if (peer.readyState === WebSocket.OPEN) peer.send(bytes, { binary });
        });
        upstream.on('unexpected-response', (_request, response) => {
          record.status = response.statusCode;
          const chunks = [];
          response.on('data', (bytes) => chunks.push(bytes));
          response.on('end', () => {
            record.failureBody = Buffer.concat(chunks).toString().slice(0, 500);
            peer.close();
          });
        });
        upstream.on('error', (error) => {
          record.failure = error.message;
          peer.close();
        });
        upstream.on('close', () => {
          upstreams.delete(upstream);
          if (!frozen) peer.close();
        });
        peer.on('close', () => upstream.terminate());
      } else {
        const frames = streams.get(key);
        if (!frames) {
          record.failure = 'native socket URL has no recorded TS stream';
          peer.close();
          return;
        }
        record.status = 101;
        record.frameHash = hash(
          JSON.stringify(
            frames.map(({ bytes, binary, afterMs }) => ({ base64: bytes.toString('base64'), binary, afterMs })),
          ),
        );
        let delivered = false;
        peer.on('message', (data) => {
          record.outboundMessages++;
          record.sent.push(Buffer.from(data).toString('base64'));
          if (delivered) return;
          delivered = true;
          // Subscribe/open handlers are installed before delivering any frame.
          for (const { bytes, binary, afterMs } of frames) {
            const timer = setTimeout(
              () => {
                timers.delete(timer);
                if (peer.readyState === WebSocket.OPEN) {
                  peer.send(bytes, { binary });
                  record.inboundMessages++;
                }
              },
              Math.max(30, afterMs - (performance.now() - openedAt)),
            );
            timers.add(timer);
          }
        });
      }
    });
  };
  const ports = {};
  for (const [family, address] of [
    ['v4', '127.0.0.1'],
    ['v6', '::1'],
  ]) {
    const server = https.createServer(serverOptions);
    server.on('upgrade', accept);
    server.on('tlsClientError', (error) =>
      records.push({ implementation, protocol: 'websocket', tlsError: error.message }),
    );
    server.on('connection', (socket) => {
      connections.add(socket);
      socket.on('close', () => connections.delete(socket));
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
    async waitForMessage(timeoutMs) {
      const deadline = Date.now() + timeoutMs;
      while (Date.now() < deadline) {
        if (
          records.some(
            (record) =>
              record.implementation === implementation && record.protocol === 'websocket' && record.inboundMessages > 0,
          )
        )
          return true;
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
      return false;
    },
    freeze() {
      frozen = true;
      for (const frames of streams.values()) {
        const serialized = JSON.stringify(
          frames.map(({ bytes, binary, afterMs }) => ({ base64: bytes.toString('base64'), binary, afterMs })),
        );
        const digest = hash(serialized);
        if (captureDirectory) {
          mkdirSync(captureDirectory, { recursive: true });
          writeFileSync(join(captureDirectory, `${digest}.ws.json`), serialized + '\n');
        }
        for (const record of records.filter(
          (row) => row.implementation === implementation && row.protocol === 'websocket',
        )) {
          record.frameHash = digest;
          if (captureDirectory) {
            record.fixture = join(captureDirectory, `${digest}.ws.json`);
            writeFileSync(join(captureDirectory, `${digest}.sent.json`), JSON.stringify(record.sent) + '\n');
          }
        }
      }
    },
    async close() {
      frozen = true;
      for (const timer of timers) clearTimeout(timer);
      timers.clear();
      for (const socket of upstreams) socket.terminate();
      for (const socket of peers) socket.terminate();
      for (const socket of connections) socket.destroy();
      await Promise.all(servers.map((server) => new Promise((resolve) => server.close(resolve))));
    },
  };
}
