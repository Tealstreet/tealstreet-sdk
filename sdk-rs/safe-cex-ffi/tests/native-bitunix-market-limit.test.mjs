import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createSecureContext, TLSSocket } from 'node:tls';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const implementation = process.argv[2];
if (implementation) {
  const options = {
    key: '',
    secret: '',
    publicDataOnly: true,
    hostedMode: true,
    tealstreetProxy: false,
    tealstreetTunnel: false,
    tuning: { consoleLogging: true },
  };
  let markets;
  if (implementation === 'ts') {
    const { exchangeRegistry } = await import('../../../packages/safe-cex/src/exchanges/registry.ts');
    const { DefaultStore } = await import('../../../packages/safe-cex/src/store/store.base.ts');
    const Exchange = await exchangeRegistry.bitunix();
    const exchange = new Exchange('bitunix', options, new DefaultStore());
    markets = await exchange.fetchMarkets();
    await exchange.dispose();
  } else {
    const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
    const id = await native.nativeAccountOpen(
      { exchange: 'bitunix', options },
      () => {},
      () => {},
    );
    markets = await native.nativeAccountCallMethod(id, 'fetchMarkets', []);
    await native.nativeAccountClose(id);
  }
  process.send({ markets });
} else {
  test('Bitunix preserves recorded amount maximum strings through the production addon', async (t) => {
    const directory = mkdtempSync(join(tmpdir(), 'safe-cex-bitunix-fixture-'));
    t.after(() => rmSync(directory, { recursive: true, force: true }));
    const key = join(directory, 'key.pem'),
      cert = join(directory, 'cert.pem'),
      config = join(directory, 'openssl.cnf');
    writeFileSync(
      config,
      '[req]\ndistinguished_name=dn\nx509_extensions=v3\nprompt=no\n[dn]\nCN=safe-cex fixture CA\n[v3]\nsubjectAltName=DNS:fapi.bitunix.com\nbasicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n',
    );
    execFileSync(
      'openssl',
      ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', key, '-out', cert, '-days', '1', '-config', config],
      { stdio: 'ignore' },
    );
    const leafKey = join(directory, 'server.key'),
      leafCert = join(directory, 'server.pem');
    const csr = join(directory, 'server.csr'),
      extensions = join(directory, 'server.ext');
    writeFileSync(
      extensions,
      'subjectAltName=DNS:fapi.bitunix.com\nextendedKeyUsage=serverAuth\nbasicConstraints=critical,CA:FALSE\n',
    );
    execFileSync(
      'openssl',
      ['req', '-newkey', 'rsa:2048', '-nodes', '-keyout', leafKey, '-out', csr, '-subj', '/CN=fapi.bitunix.com'],
      { stdio: 'ignore' },
    );
    execFileSync(
      'openssl',
      [
        'x509',
        '-req',
        '-in',
        csr,
        '-CA',
        cert,
        '-CAkey',
        key,
        '-CAcreateserial',
        '-out',
        leafCert,
        '-days',
        '1',
        '-extfile',
        extensions,
      ],
      { stdio: 'ignore' },
    );
    const context = createSecureContext({ key: readFileSync(leafKey), cert: readFileSync(leafCert) });
    const bytes = readFileSync(new URL('./fixtures/live-public/bitunix-markets.json', import.meta.url));
    const requests = [],
      sockets = new Set();
    const handle = (request, response) => {
      const url = new URL(request.url, 'https://fapi.bitunix.com');
      assert.equal(url.hostname, 'fapi.bitunix.com');
      assert.equal(request.method, 'GET');
      assert.equal(url.pathname, '/api/v1/futures/market/trading_pairs');
      requests.push(url.pathname);
      response.writeHead(200, { 'content-type': 'application/json' }).end(bytes);
    };
    const server = createServer(handle);
    server.on('connection', (socket) => {
      sockets.add(socket);
      socket.on('error', () => {});
      socket.on('close', () => sockets.delete(socket));
    });
    server.on('connect', (request, socket, head) => {
      assert.equal(request.url, 'fapi.bitunix.com:443');
      socket.write('HTTP/1.1 200 Connection Established\r\n\r\n');
      if (head.length) socket.unshift(head);
      const secure = new TLSSocket(socket, { isServer: true, secureContext: context });
      secure.on('error', () => socket.destroy());
      createServer(handle).emit('connection', secure);
    });
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    t.after(async () => {
      for (const socket of sockets) socket.destroy();
      server.closeAllConnections();
      await new Promise((resolve) => server.close(resolve));
    });
    const proxy = `http://127.0.0.1:${server.address().port}`;
    async function run(kind) {
      const start = requests.length;
      let diagnostic = '';
      const markets = await new Promise((resolve, reject) => {
        const child = spawn(process.execPath, ['--import', 'tsx', fileURLToPath(import.meta.url), kind], {
          stdio: ['ignore', 'ignore', 'pipe', 'ipc'],
          env: {
            ...process.env,
            HTTPS_PROXY: proxy,
            HTTP_PROXY: proxy,
            ALL_PROXY: proxy,
            https_proxy: proxy,
            http_proxy: proxy,
            all_proxy: proxy,
            NO_PROXY: '127.0.0.1,localhost',
            no_proxy: '127.0.0.1,localhost',
            SSL_CERT_FILE: cert,
            NODE_EXTRA_CA_CERTS: cert,
            TSX_TSCONFIG_PATH: fileURLToPath(new URL('../../../packages/safe-cex/tsconfig.json', import.meta.url)),
          },
        });
        let result,
          errors = '';
        const timer = setTimeout(() => {
          child.kill('SIGKILL');
          reject(new Error(`${kind} fixture read timed out`));
        }, 10000);
        child.on('message', (message) => {
          result = message.markets;
        });
        child.stderr.on('data', (data) => {
          errors += data;
          diagnostic += data;
        });
        child.on('error', reject);
        child.on('exit', (code) => {
          clearTimeout(timer);
          if (code === 0 && result) resolve(result);
          else reject(new Error(`${kind} child exited ${code}: ${errors}`));
        });
      });
      assert.equal(requests.length - start, 1, `${kind} must consume the recorded 200: ${diagnostic}`);
      assert.equal(markets.length, 697);
      return markets;
    }
    const expected = await run('ts');
    assert.equal(expected[0].limits.amount.max, '1200');
    const actual = await run('rust');
    assert.equal(actual[0].limits.amount.max, '1200', 'maximum retains its source string type');
    assert.deepEqual(actual, expected);
  });
}
