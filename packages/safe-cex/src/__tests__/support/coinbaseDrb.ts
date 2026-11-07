import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

type Capture = {
  type: string;
  url: string;
  method?: string;
  httpMethod?: string;
  params?: Record<string, unknown>;
  status?: number;
  body?: string;
  text?: string;
  [key: string]: unknown;
};

// The same live, public-only gateway capture drives the native S1 replay gate.
const capture: Capture[] = readFileSync(
  resolve(__dirname, '../../../../../sdk-rs/venues/coinbase/tests/fixtures/drb-public.jsonl'),
  'utf8',
).trim().split('\n').map((line) => JSON.parse(line));
const startupMethods = new Set(['public/get_instruments', 'public/get_book_summary_by_currency']);

export const coinbaseDrbPublicRecords = capture
  .filter((row) => row.type === 'http'
    ? startupMethods.has(row.method!)
    : row.type === 'ws' && JSON.parse(row.text!).method === 'subscription')
  .map((row) => row.type === 'http' ? { ...row, method: row.httpMethod } : row);

export const coinbaseDrbPublicResponse = (request: Parameters<Transport['request']>[0]) => {
  const sent = JSON.parse(request.body!);
  const record = capture.find((row) => row.type === 'http' && row.method === sent.method);
  assert.ok(startupMethods.has(sent.method) && record, `Untaped drb startup method ${sent.method}`);
  assert.equal(request.url, record.url);
  assert.equal(request.method, record.httpMethod);
  assert.equal(sent.jsonrpc, '2.0');
  assert.ok(Number.isSafeInteger(sent.id) && sent.id > 0);
  assert.deepEqual(sent.params, record.params);
  assert.ok(Object.keys(request.headers).every((key) => !key.toLowerCase().includes('authorization')));
  const body = JSON.parse(record.body!);
  // Correlate the captured public result to this engine's request; financial
  // fields, timestamps, instrument IDs and all result rows remain untouched.
  body.id = sent.id;
  return { status: record.status!, body: new TextEncoder().encode(JSON.stringify(body)) };
};
