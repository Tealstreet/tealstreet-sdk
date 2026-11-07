// Public-only live capture. Never reads credentials or records HTTP headers.
// node sdk-rs/harness/record-coinbase-drb-public.mjs [output.jsonl]
import { writeFile } from 'node:fs/promises';

const output = process.argv[2] ?? 'sdk-rs/venues/coinbase/tests/fixtures/drb-public.jsonl';
const epoch = Date.now();
const instrument = 'BTC_USDC-PERPETUAL';
const start = Math.floor(epoch / 3_600_000) * 3_600_000 - 3_600_000;
const records = [{ type: 'meta', source: 'LIVE public drb gateway', epoch, instrument, start, end: start + 3_600_000 }];
const calls = [
  ['public/get_instruments', { currency: 'USDC', kind: 'future', expired: false }],
  ['public/get_book_summary_by_currency', { currency: 'USDC', kind: 'future' }],
  ['public/ticker', { instrument_name: instrument }],
  ['public/get_order_book', { instrument_name: instrument, depth: 10 }],
  ['public/get_last_trades_by_instrument', { instrument_name: instrument, count: 10, sorting: 'asc' }],
  ['public/get_tradingview_chart_data', { instrument_name: instrument, resolution: '1', start_timestamp: start, end_timestamp: start + 3_600_000 }],
  ['public/get_funding_rate_history', { instrument_name: instrument, start_timestamp: start - 86_400_000, end_timestamp: start }],
];
for (const [index, [method, params]] of calls.entries()) {
  const httpMethod = method === 'public/get_tradingview_chart_data' ? 'GET' : 'POST';
  const url = `https://drb.coinbase.com/api/v2/${method}${httpMethod === 'GET' ? `?${new URLSearchParams(params)}` : ''}`;
  const requestBody = httpMethod === 'GET' ? undefined : JSON.stringify({ jsonrpc: '2.0', id: index + 1, method, params });
  const response = await fetch(url, { method: httpMethod, ...(requestBody ? { headers: { 'Content-Type': 'application/json' }, body: requestBody } : {}), signal: AbortSignal.timeout(20_000) });
  const text = await response.text();
  const body = JSON.parse(text);
  if (!response.ok || body.error || body.result === undefined) throw new Error(`${method}: ${text}`);
  records.push({ type: 'http', t: Date.now() - epoch, method, params, url, httpMethod, requestBody, status: response.status, body: text });
  console.log(`${method}: ${text.length} bytes`);
}
const url = 'wss://streams.drb.coinbase.com/ws/api/v2';
const channels = [`book.${instrument}.100ms`, `ticker.${instrument}.100ms`, `trades.${instrument}.100ms`, `chart.trades.${instrument}.1`];
await new Promise((resolve, reject) => {
  const socket = new WebSocket(url);
  const timer = setTimeout(() => { socket.close(); resolve(); }, 15_000);
  socket.addEventListener('open', () => {
    const text = JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'public/subscribe', params: { channels } });
    records.push({ type: 'send', url, text });
    socket.send(text);
  });
  socket.addEventListener('message', ({ data }) => records.push({ type: 'ws', t: Date.now() - epoch, url, text: String(data) }));
  socket.addEventListener('error', () => { clearTimeout(timer); reject(new Error('public drb WebSocket failed')); });
});
for (const channel of channels) {
  if (!records.some(r => r.type === 'ws' && JSON.parse(r.text).params?.channel === channel))
    throw new Error(`No live frames for ${channel}; capture again`);
}
await writeFile(output, records.map(r => JSON.stringify(r)).join('\n') + '\n');
console.log(`Wrote ${records.length} records to ${output}`);
