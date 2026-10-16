import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { dirname, resolve, sep, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import puppeteer from './puppeteer.mjs';
import { packageVersion } from '../../scripts/tooling.mjs';

const crate = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const expectedVersion = packageVersion();
const mime = { '.html': 'text/html', '.mjs': 'text/javascript', '.js': 'text/javascript', '.wasm': 'application/wasm' };
const server = createServer(async (request, response) => {
  try {
    const pathname = new URL(request.url, 'http://localhost').pathname;
    const path = resolve(crate, `.${pathname === '/' ? '/smoke/web/index.html' : pathname}`);
    if (!path.startsWith(`${crate}${sep}`)) throw new Error('outside smoke root');
    const body = await readFile(path);
    response.writeHead(200, { 'Content-Type': mime[extname(path)] || 'application/octet-stream' });
    response.end(body);
  } catch {
    response.writeHead(404);
    response.end();
  }
});
let browser;
try {
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  browser = await puppeteer.launch({ headless: true, args: ['--no-sandbox'] });
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.waitForFunction(() => window.ffiSmokeResult !== undefined, { timeout: 15000 });
  const result = await page.evaluate(() => window.ffiSmokeResult);
  assert.equal(result.error, undefined, result.error);
  assert.equal(result.isWorker, true, 'probe must run inside a browser Worker');
  assert.deepEqual(result.values, ['safe-cex:0', 'safe-cex:42', 'safe-cex:4294967295']);
  assert.equal(result.version, expectedVersion);
  console.log(`Browser Worker smoke passed: ${JSON.stringify(result)}`);
} finally {
  try {
    await browser?.close();
  } finally {
    await new Promise(resolve => server.close(resolve));
  }
}
