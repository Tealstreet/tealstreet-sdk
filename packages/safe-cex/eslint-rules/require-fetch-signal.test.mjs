import assert from 'node:assert/strict';
import test from 'node:test';

import tsParser from '@typescript-eslint/parser';
import { Linter } from 'eslint';

import plugin from './require-fetch-signal.mjs';

const linter = new Linter({ configType: 'flat' });
const config = {
  files: ['**/*.ts'],
  languageOptions: {
    parser: tsParser,
    parserOptions: { ecmaVersion: 'latest', sourceType: 'module' },
  },
  plugins: { network: plugin },
  rules: { 'network/require-fetch-signal': 'error' },
};

const flagged = (code) => linter.verify(code, config, { filename: 'client.ts' }).map(({ line }) => line);

test('rejects a fetch, an axios call and a WebSocket open with no bound', () => {
  assert.deepEqual(
    flagged(`
      async function load() {
        await fetch('/api/proxykey');
        await fetch('/api/x', { credentials: 'include' });
        await globalThis.fetch('/api/x', init);
        await axios.get('/api/feature-flags');
        await axios({ url: '/api/x' });
        new WebSocket(url);
        new ReconnectingWebSocket(provider, undefined, { maxRetries: Infinity });
      }
    `),
    [3, 4, 5, 6, 7, 8, 9],
  );
});

test('accepts a signal, a timeout, a connection timeout, the bounded helpers and the boot clients', () => {
  assert.deepEqual(
    flagged(`
      async function load() {
        await fetch('/api/x', { signal });
        await fetch('/api/x', { ...options, signal: abort.signal });
        await axios.get('/api/x', { timeout: 10_000 });
        await axios.post('/api/x', body, { signal });
        await withTimeout(10_000, (given) => fetch('/api/x', { signal: given }));
        await withTimeout(10_000, () => fetch('/api/x'));
        await withDeadline(fetch('/api/x'), 5_000, 'x');
        await bootHttp.get('/api/feature-flags');
        await bootFetch('/api/x');
        new ReconnectingWebSocket(provider, undefined, { connectionTimeout: 10_000 });
        const options = { connectionTimeout: 4_000 };
        new ReconnectingWebSocket(url, undefined, options);
        const init = { method: 'POST', signal };
        await fetch('/api/x', init);
        axios.create({ timeout: 10_000 });
      }
    `),
    [],
  );
});
