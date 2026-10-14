// Starts the built dist/index.js over stdio and checks it answers initialize and
// tools/list, so a release cannot publish a bundle that does not boot.
import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ENTRY = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../dist/index.js');
const TIMEOUT_MS = 30_000;

const child = spawn(process.execPath, [ENTRY], {
  env: { ...process.env, EXCHANGE: 'bybit' },
  stdio: ['pipe', 'pipe', 'inherit'],
});

const fail = (message) => {
  console.error(`smoke: ${message}`);
  child.kill();
  process.exit(1);
};
const timer = setTimeout(() => fail(`no tools/list answer within ${TIMEOUT_MS}ms`), TIMEOUT_MS);
child.on('exit', (code) => fail(`server exited early with code ${code}`));

const send = (message) => child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', ...message })}\n`);
let buffer = '';
child.stdout.on('data', (chunk) => {
  buffer += chunk;
  let newline;
  while ((newline = buffer.indexOf('\n')) >= 0) {
    const line = buffer.slice(0, newline);
    buffer = buffer.slice(newline + 1);
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      fail(`stdout carried a non-protocol line: ${line.slice(0, 200)}`);
    }
    if (message.id === 1) {
      send({ method: 'notifications/initialized' });
      send({ id: 2, method: 'tools/list' });
    } else if (message.id === 2) {
      const names = (message.result?.tools ?? []).map((tool) => tool.name);
      for (const required of ['get_markets', 'get_tickers', 'place_order', 'cancel_orders']) {
        if (!names.includes(required)) fail(`tools/list is missing ${required}: ${names.join(', ')}`);
      }
      clearTimeout(timer);
      child.removeAllListeners('exit');
      child.kill();
      console.log(`smoke: ${names.length} tools listed`);
      process.exit(0);
    }
  }
});

send({
  id: 1,
  method: 'initialize',
  params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'smoke', version: '0' } },
});
