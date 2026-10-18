import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import https from 'node:https';
import { join } from 'node:path';
import tls from 'node:tls';

export const hash = (value) => createHash('sha256').update(value).digest('hex');

export function makeCA(directory) {
  const key = join(directory, 'ca.key'),
    cert = join(directory, 'ca.pem');
  execFileSync(
    'openssl',
    [
      'req',
      '-x509',
      '-newkey',
      'rsa:2048',
      '-nodes',
      '-keyout',
      key,
      '-out',
      cert,
      '-days',
      '1',
      '-subj',
      '/CN=safe-cex ephemeral live probe CA',
    ],
    { stdio: 'ignore' },
  );
  return { key, cert };
}

export function certificateCache(directory, ca) {
  const certificates = new Map();
  return (hostname) => {
    if (!certificates.has(hostname)) {
      const key = join(directory, `${hostname}.key`),
        csr = join(directory, `${hostname}.csr`);
      const cert = join(directory, `${hostname}.pem`),
        extensions = join(directory, `${hostname}.ext`);
      execFileSync(
        'openssl',
        ['req', '-newkey', 'rsa:2048', '-nodes', '-keyout', key, '-out', csr, '-subj', `/CN=${hostname}`],
        { stdio: 'ignore' },
      );
      // SAN validates in Node and Rust. An extfile works with macOS LibreSSL
      // too, where `-copy_extensions` / `-addext` are not always available.
      writeFileSync(extensions, `subjectAltName=DNS:${hostname}\nextendedKeyUsage=serverAuth\n`);
      execFileSync(
        'openssl',
        [
          'x509',
          '-req',
          '-in',
          csr,
          '-CA',
          ca.cert,
          '-CAkey',
          ca.key,
          '-CAcreateserial',
          '-out',
          cert,
          '-days',
          '1',
          '-extfile',
          extensions,
        ],
        { stdio: 'ignore' },
      );
      certificates.set(hostname, tls.createSecureContext({ key: readFileSync(key), cert: readFileSync(cert) }));
    }
    return certificates.get(hostname);
  };
}

export function isRead(method, url, body) {
  if (method === 'GET' || method === 'HEAD') return true;
  // Hyperliquid reads use POST /info. No other POST is implicitly a read.
  if (method !== 'POST') return false;
  if (url.hostname === 'api.hyperliquid.xyz' && url.pathname === '/info') return true;
  if (url.hostname === 'clob.polymarket.com' && url.pathname === '/midpoints') return true;
  if (url.hostname === 'breakout.prosp.devexperts.com' && url.pathname === '/dxsca-web/marketdata') return true;
  if (
    url.hostname === 'market-data.grvt.io' &&
    ['/lite/v1/all_instruments', '/lite/v1/ticker', '/lite/v1/mini'].includes(url.pathname)
  )
    return true;
  return url.hostname === 'api.crypto.com' && url.pathname.startsWith('/exchange/v1/public/get-');
}

export function readKey(method, url, body) {
  const stable = new URL(url);
  // Bybit appends a random correlation requestId even to public GETs.
  // This only keys response replay; raw request bytes remain untouched.
  for (const key of ['timestamp', 'signature', 'recvWindow', 'requestId']) stable.searchParams.delete(key);
  if (stable.hostname === 'openapi.blofin.com' && stable.pathname === '/api/v1/market/instruments') {
    for (const [key, value] of stable.searchParams) {
      if (value === '' && /^[A-Za-z0-9_-]{21}$/.test(key)) stable.searchParams.delete(key);
    }
  }
  stable.searchParams.sort();
  return `${method} ${stable.href}\n${body}`;
}

// Each implementation has a separate port, so all native and Axios terminal
// requests can be attributed without changing either production implementation.
export async function recordingProxy({
  directory,
  ca,
  cache,
  records,
  implementation,
  allowHost,
  forward = https.request,
  validateLive = false,
  captureDirectory,
}) {
  const certificateFor = certificateCache(directory, ca);
  let allowWrites = false;
  const pending = new Set();
  const sockets = new Set();
  async function handle(request, response, connectHost) {
    try {
      const url = new URL(request.url, `https://${connectHost}`);
      if (url.protocol !== 'https:' || !allowHost(url.hostname) || (url.port && url.port !== '443')) {
        throw new Error('unapproved destination');
      }
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      const bytes = Buffer.concat(chunks),
        body = bytes.toString();
      const method = request.method;
      const sessionRead =
        method === 'POST' &&
        ((allowWrites && url.hostname === 'fapi.binance.com' && url.pathname === '/fapi/v1/listenKey') ||
          (validateLive && url.hostname === 'breakout.prosp.devexperts.com' && url.pathname === '/dxsca-web/login'));
      const read = isRead(method, url, body) || sessionRead;
      // Only explicit placement/cancel phases enable writes. Public mode fails
      // closed even if a venue accidentally performs a write during a read.
      const permittedOrderWrite =
        url.hostname === 'fapi.binance.com' && url.pathname === '/fapi/v1/order' && ['POST', 'DELETE'].includes(method);
      if (!read && !(allowWrites && permittedOrderWrite)) {
        records.push({ implementation, method, path: url.pathname, blocked: true });
        response.writeHead(403, { 'content-type': 'application/json' });
        response.end('{"error":"probe blocked write outside order phase"}');
        return;
      }
      const key = readKey(method, url, body);
      const cacheHit = read && !sessionRead && cache.has(key);
      let snapshot = cacheHit ? cache.get(key) : undefined;
      const record = {
        implementation,
        method,
        host: url.hostname,
        path: url.pathname,
        read,
        sessionRead,
        cacheHit,
        requestBytes: `${method} ${url.pathname}${url.search}\n${body}`,
      };
      if (url.hostname === 'api.hyperliquid.xyz' && url.pathname === '/info') {
        try {
          record.infoType = JSON.parse(body).type;
        } catch {}
      }
      records.push(record);
      let live;
      // Even when mapping against one shared snapshot, both authenticated
      // implementations must have their own requests accepted by the exchange.
      // Never let replay conceal an invalid native signature or permissions.
      if (!cacheHit || validateLive) {
        const headers = { ...request.headers, host: url.host };
        for (const name of ['proxy-connection', 'proxy-authorization', 'connection', 'accept-encoding'])
          delete headers[name];
        const promise = new Promise((resolve, reject) => {
          // Direct https.request deliberately bypasses proxy environment vars.
          const upstream = forward(url, { method, headers, timeout: 20000 }, (incoming) => {
            const parts = [];
            incoming.on('data', (part) => parts.push(part));
            incoming.on('error', reject);
            incoming.on('end', () => {
              const cleanHeaders = { ...incoming.headers };
              for (const name of ['transfer-encoding', 'connection', 'content-length']) delete cleanHeaders[name];
              resolve({ status: incoming.statusCode, headers: cleanHeaders, bytes: Buffer.concat(parts) });
            });
          });
          upstream.on('timeout', () => upstream.destroy(new Error('upstream timeout')));
          upstream.on('error', reject);
          upstream.end(bytes);
        });
        if (read && !cacheHit) {
          snapshot = promise;
          cache.set(key, promise);
        }
        live = await promise;
        record.liveStatus = live.status;
        record.liveResponseHash = hash(live.bytes);
      }
      const answer = live && live.status >= 400 ? live : read ? await snapshot : live;
      if (captureDirectory) {
        mkdirSync(captureDirectory, { recursive: true });
        record.fixture = join(captureDirectory, `${hash(answer.bytes)}.json`);
        writeFileSync(record.fixture, answer.bytes, { mode: 0o600 });
        writeFileSync(
          record.fixture.replace(/\.json$/, '.request.json'),
          JSON.stringify(
            {
              method,
              url: url.href,
              requestBody: body,
              status: answer.status,
              responseHash: hash(answer.bytes),
            },
            null,
            2,
          ) + '\n',
          { mode: 0o600 },
        );
      }
      record.status = answer.status;
      // Both production implementations explicitly fall back to default fees
      // for a keyless userFees read (HyperliquidExchange.fetchFee). Keep the
      // rejected exchange response visible without failing an identical,
      // successfully populated public bootstrap.
      record.expectedFallback = answer.status === 422 && record.infoType === 'userFees' && JSON.parse(body).user === '';
      if (url.hostname === 'fapi.asterdex.com') {
        let code;
        try {
          code = JSON.parse(answer.bytes).code;
        } catch {}
        record.expectedFallback =
          (url.pathname === '/fapi/v1/leverageBracket' &&
            answer.status === 401 &&
            !validateLive &&
            code === -2014 &&
            !request.headers['x-mbx-apikey']) ||
          (url.pathname === '/fapi/v1/openInterest' &&
            answer.status === 400 &&
            code === -1102 &&
            !url.searchParams.has('symbol'));
      }
      record.responseHash = hash(answer.bytes);
      if (
        url.hostname === 'api.international.coinbase.com' &&
        url.pathname === '/api/v1/instruments/' &&
        answer.status === 200
      ) {
        const instruments = JSON.parse(answer.bytes);
        if (Array.isArray(instruments))
          record.venueData = {
            instruments: instruments.length,
            tradingPerpetuals: instruments.filter(
              (instrument) => instrument.type === 'PERP' && instrument.trading_state === 'TRADING',
            ).length,
            states: instruments.reduce((counts, instrument) => {
              counts[instrument.trading_state] = (counts[instrument.trading_state] ?? 0) + 1;
              return counts;
            }, {}),
          };
      }
      response.writeHead(answer.status, answer.headers);
      response.end(answer.bytes);
    } catch {
      records.push({ implementation, transportError: true });
      if (!response.headersSent) response.writeHead(502);
      response.end('probe upstream transport failed');
    }
  }
  const server = http.createServer((req, res) => {
    const task = handle(req, res);
    pending.add(task);
    task.finally(() => pending.delete(task));
  });
  server.on('connection', (socket) => {
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
  });
  server.on('connect', (request, socket, head) => {
    const hostname = request.url.split(':')[0];
    if (!allowHost(hostname) || !request.url.endsWith(':443')) return socket.destroy();
    try {
      const context = certificateFor(hostname);
      socket.write('HTTP/1.1 200 Connection Established\r\n\r\n');
      if (head.length) socket.unshift(head);
      const secure = new tls.TLSSocket(socket, { isServer: true, secureContext: context });
      secure.on('error', () => socket.destroy());
      const inside = http.createServer((req, res) => {
        const task = handle(req, res, hostname);
        pending.add(task);
        task.finally(() => pending.delete(task));
      });
      inside.emit('connection', secure);
    } catch {
      socket.destroy();
    }
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return {
    url: `http://127.0.0.1:${server.address().port}`,
    writes(enabled) {
      allowWrites = enabled;
    },
    async close() {
      for (const socket of sockets) socket.destroy();
      await new Promise((resolve) => server.close(resolve));
      await Promise.allSettled(pending);
    },
  };
}
