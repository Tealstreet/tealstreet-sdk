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
  capturedSockets = [],
  openUpstream = (url) => new WebSocket(url, { handshakeTimeout: 15000, perMessageDeflate: false, autoPong: false }),
}) {
  const certificateFor = certificateCache(directory, ca);
  const servers = [],
    sockets = new Set(),
    upstreams = new Set(),
    peers = new Map();
  const occurrences = new Map(),
    boundReplayConnections = new Set();
  let frozen = false,
    cancelled = false;
  const misses = [];
  const progress = { frameIndex: -1, completed: 0 };
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
      let connection =
        mode === 'capture'
          ? { id: tape.connections.length, key, occurrence, url: url.href, openedAtMs: elapsed() }
          : tape.connections.find((c) => c.key === key && c.occurrence === occurrence);
      const duplicateReplay = mode === 'replay' && tape.connections.filter((c) => c.key === key).length > 1;
      if (duplicateReplay) connection = undefined;
      if (mode === 'capture') tape.connections.push(connection);
      const record = { mode, key, occurrence, networkOccurrence: occurrence, sent: [], delivered: 0, dropped: 0 };
      records.push(record);
      // Replay must never synthesize an extra parser-facing pong. In capture,
      // protocol ping/pong travels end-to-end and is recorded on the way back.
      const wss = new WebSocketServer({ noServer: true, perMessageDeflate: false, autoPong: false });
      wss.handleUpgrade(request, socket, head, (peer) => {
        peer.on('error', (error) => {
          if (!cancelled) record.failure = error.message;
        });
        if (!connection && !duplicateReplay) {
          record.failure = 'No matching captured socket';
          peer.close();
          return;
        }
        const bindPeer = () => {
          peers.set(connection.id, peer);
          record.occurrence = connection.occurrence;
          if (mode === 'replay') boundReplayConnections.add(connection.id);
        };
        if (connection) bindPeer();
        peer.on('close', () => {
          if (connection) peers.delete(connection.id);
        });
        peer.on('message', (data, binary) => {
          const base64 = data.toString('base64');
          record.sent.push({ base64, binary, atMs: elapsed() });
          if (!connection && duplicateReplay) {
            // Accept order is not logical identity when a venue opens several
            // sockets at one URL. Match exact captured subscription bytes.
            const matches = tape.connections.filter((candidate) => {
              if (candidate.key !== key || boundReplayConnections.has(candidate.id)) return false;
              const sent = capturedSockets.find((r) => r.key === key && r.occurrence === candidate.occurrence)
                ?.sent?.[0];
              return sent?.base64 === base64 && sent.binary === binary;
            });
            if (matches.length !== 1) {
              record.failure = `Duplicate public dial has ${matches.length} exact initial-send matches`;
              peer.close();
              return;
            }
            connection = matches[0];
            bindPeer();
          }
        });
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
    progress,
    async ready(timeoutMs = 10000) {
      const until = Date.now() + timeoutMs;
      while (true) {
        const duplicateUrls = new Set(tape.connections.map((c) => c.key)).size !== tape.connections.length;
        const first = duplicateUrls ? tape.connections : tape.connections.filter((c) => c.occurrence === 0);
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
    async playControlled({ advance, receipt, timeoutMs = 60000 }) {
      const expected = new Map(),
        closes = new Map();
      const transportExpected = new Map(
        tape.connections.map((c) => [`${c.key}#${c.occurrence}`, { message: 0, ping: 0, pong: 0, close: 0 }]),
      );
      const sent = [];
      const duplicateUrls = new Set(tape.connections.map((c) => c.key)).size !== tape.connections.length;
      const parserBindings = new Map(),
        transportBindings = new Map();
      const applicationDrained = (value, closeCounts) =>
        duplicateUrls
          ? verifyMappedDrain(tape, expected, value, closeCounts, parserBindings)
          : verifyDrain(tape, expected, value, closeCounts);
      const transportDrained = (value, counts) =>
        duplicateUrls
          ? verifyMappedTransportDrain(tape, value, counts, transportBindings)
          : verifyTransportDrain(tape, value, counts);
      let lastReceipt;
      for (const [index, frame] of tape.frames.entries()) {
        progress.frameIndex = index;
        progress.atMs = frame.atMs;
        await advance(frame.atMs);
        const peer = peers.get(frame.connection);
        if (!peer || peer.readyState !== WebSocket.OPEN) throw new Error(`Missing replay socket for frame ${index}`);
        const bytes = Buffer.from(frame.base64, 'base64');
        const connection = tape.connections.find((c) => c.id === frame.connection);
        if (frame.kind === 'message') {
          const key = `${connection.key}#${connection.occurrence}`;
          expected.set(key, (expected.get(key) ?? 0) + 1);
          peer.send(bytes, { binary: frame.binary });
        } else if (frame.kind === 'close') closePeer(peer, frame.code, bytes);
        else peer[frame.kind](bytes);
        sent.push(frame);
        const identity = `${connection.key}#${connection.occurrence}`;
        transportExpected.get(identity)[frame.kind]++;
        if (frame.kind === 'close') closes.set(identity, (closes.get(identity) ?? 0) + 1);
        const deadline = Date.now() + timeoutMs;
        while (true) {
          lastReceipt = await receipt();
          progress.receipt = lastReceipt;
          if (
            applicationDrained(lastReceipt, closes) &&
            (!lastReceipt.transport || transportDrained(lastReceipt, transportExpected))
          )
            break;
          if (Date.now() >= deadline) throw new Error(`Native/TS processing drain timeout at frame ${index}`);
          await new Promise((resolve) => setTimeout(resolve, 1));
        }
        progress.completed = sent.length;
      }
      await advance(tape.durationMs + 350);
      lastReceipt = await receipt();
      const verified = applicationDrained(lastReceipt, closeCounts(tape));
      const nativeTransportVerified = transportDrained(lastReceipt, transportCounts(tape));
      return {
        delivered: sent.length,
        dropped: 0,
        inputHash: hash(JSON.stringify(sent)),
        expectedInputHash: hash(JSON.stringify(tape.frames)),
        processed: lastReceipt,
        applicationDrainVerified: verified,
        ...(duplicateUrls
          ? {
              dialBindings: {
                parser: Object.fromEntries(parserBindings),
                transport: Object.fromEntries(transportBindings),
              },
            }
          : {}),
        nativeTransportVerified,
        // Ping/pong control bytes are delivered over TLS but terminate below the
        // venue parser. They are deliberately excluded from application counts.
        expectedApplicationFrames: Object.fromEntries(expected),
        clockMs: tape.durationMs + 350,
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

// Fail closed: compare exact counts per physical URL occurrence, rejecting extra
// messages/sockets as well as missing or unfinished handlers.
export function verifyDrain(tape, expected, receipt, expectedCloses = closeCounts(tape)) {
  if (!receipt?.sockets || !Number.isSafeInteger(receipt.pendingJobs) || receipt.pendingJobs !== 0) return false;
  if (receipt.transport && !verifyRawTaskDrain(receipt.rawTasks)) return false;
  const occurrences = new Map(),
    actual = new Map();
  for (const socket of receipt.sockets) {
    const key = streamKey(socket.url),
      occurrence = occurrences.get(key) ?? 0;
    occurrences.set(key, occurrence + 1);
    const identity = `${key}#${occurrence}`;
    if (!tape.connections.some((c) => c.key === key && c.occurrence === occurrence)) return false;
    if (socket.consumed !== socket.processed) return false;
    const closes = expectedCloses.get(identity) ?? 0;
    if ((socket.closeConsumed ?? 0) !== closes || (socket.closeProcessed ?? 0) !== closes) return false;
    actual.set(identity, socket.processed);
  }
  for (const [key, count] of expected) if (actual.get(key) !== count) return false;
  for (const [key, count] of actual) if (count !== (expected.get(key) ?? 0)) return false;
  return true;
}

// Old addons and malformed/inconsistent counters cannot certify raw task drain.
export function verifyRawTaskDrain(tasks) {
  if (tasks?.version !== 1 || tasks.scope !== 'exchange-runtime') return false;
  for (const key of ['unknown', 'finite', 'ready', 'waitingTimers', 'waitingSockets', 'promiseDrivers', 'blocking']) {
    if (!Number.isSafeInteger(tasks[key]) || tasks[key] < 0) return false;
  }
  return tasks.blocking === tasks.unknown + tasks.finite + tasks.ready && tasks.blocking === 0;
}

export function verifyTransportDrain(tape, receipt, counts = transportCounts(tape)) {
  if (!receipt?.transport) return false;
  const expected = new Map(counts);
  const occurrences = new Map();
  for (const { url, counts } of receipt.transport) {
    const key = streamKey(url),
      occurrence = occurrences.get(key) ?? 0;
    occurrences.set(key, occurrence + 1);
    const identity = `${key}#${occurrence}`,
      wanted = expected.get(identity);
    if (!wanted || Object.keys(wanted).some((kind) => counts[kind] !== wanted[kind])) return false;
    expected.delete(identity);
  }
  return expected.size === 0;
}

function transportCounts(tape) {
  const expected = new Map(
    tape.connections.map((c) => [`${c.key}#${c.occurrence}`, { message: 0, ping: 0, pong: 0, close: 0 }]),
  );
  for (const frame of tape.frames) {
    const connection = tape.connections.find((c) => c.id === frame.connection);
    expected.get(`${connection.key}#${connection.occurrence}`)[frame.kind]++;
  }
  return expected;
}
function closeCounts(tape) {
  const counts = new Map();
  for (const frame of tape.frames) {
    if (frame.kind !== 'close') continue;
    const connection = tape.connections.find((c) => c.id === frame.connection);
    const key = `${connection.key}#${connection.occurrence}`;
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return counts;
}

function bindExactCounters(tape, rows, wantedCounters, bindings) {
  if (rows.length !== tape.connections.length) return false;
  const candidate = new Map(bindings),
    used = new Set();
  const identity = (connection) => `${connection.key}#${connection.occurrence}`;
  const same = (a, b) => a.length === b.length && a.every((n, i) => Number.isSafeInteger(n) && n >= 0 && n === b[i]);
  for (const [key, bound] of candidate) {
    const row = rows[bound.index],
      connection = tape.connections.find((c) => identity(c) === key);
    if (
      !row ||
      !connection ||
      row.key !== connection.key ||
      row.generation !== bound.generation ||
      !same(row.counts, wantedCounters.get(key))
    )
      return false;
    if (used.has(bound.index)) return false;
    used.add(bound.index);
  }
  for (const connection of tape.connections) {
    const key = identity(connection),
      wanted = wantedCounters.get(key);
    if (candidate.has(key) || wanted.every((n) => n === 0)) continue;
    const matches = rows.flatMap((row, index) =>
      !used.has(index) && row.key === connection.key && same(row.counts, wanted) ? [index] : [],
    );
    if (matches.length !== 1) return false;
    const index = matches[0];
    candidate.set(key, { index, generation: rows[index].generation });
    used.add(index);
  }
  // An unbound zero vector carries no identity claim. Extra work on any such
  // socket fails, rather than being attributed by summing across the URL.
  if (rows.some((row, index) => !used.has(index) && row.counts.some((n) => n !== 0))) return false;
  if (rows.some((row) => !tape.connections.some((c) => c.key === row.key))) return false;
  bindings.clear();
  for (const [key, value] of candidate) bindings.set(key, value);
  return true;
}

export function verifyMappedDrain(tape, expected, receipt, expectedCloses, bindings) {
  if (!receipt?.sockets || receipt.pendingJobs !== 0) return false;
  if (receipt.transport && !verifyRawTaskDrain(receipt.rawTasks)) return false;
  if (
    receipt.sockets.some(
      (s) => s.consumed !== s.processed || s.errors !== 0 || (s.closeConsumed ?? 0) !== (s.closeProcessed ?? 0),
    )
  )
    return false;
  const wanted = new Map(
    tape.connections.map((c) => {
      const key = `${c.key}#${c.occurrence}`;
      return [key, [expected.get(key) ?? 0, expectedCloses.get(key) ?? 0]];
    }),
  );
  return bindExactCounters(
    tape,
    receipt.sockets.map((s) => ({
      key: streamKey(s.url),
      generation: s.generation,
      counts: [s.processed, s.closeProcessed ?? 0],
    })),
    wanted,
    bindings,
  );
}

export function verifyMappedTransportDrain(tape, receipt, expected, bindings) {
  if (!receipt?.transport) return false;
  const fields = ['message', 'ping', 'pong', 'close'];
  const wanted = new Map([...expected].map(([key, counts]) => [key, fields.map((kind) => counts[kind])]));
  return bindExactCounters(
    tape,
    receipt.transport.map((s) => ({ key: streamKey(s.url), counts: fields.map((kind) => s.counts[kind]) })),
    wanted,
    bindings,
  );
}
