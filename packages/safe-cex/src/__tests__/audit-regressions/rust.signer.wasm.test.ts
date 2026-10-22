import type { SignRequest, Transport } from '@tealstreet/safe-cex-wasm/host';
import { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import { createHmac } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { expect, it } from 'vitest';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';

const identities = { primary: { credential: 'primary.credential', signing: 'primary.signing' } };
const vector = JSON.parse(readFileSync(new URL('../../../../../sdk-rs/crypto/tests/fixtures/hmac/binance.json', import.meta.url), 'utf8')) as { secret: string };
const makeTransport = (sent: Parameters<Transport['request']>[0][]): Transport => ({
  now: () => 1_700_000_000_000,
  request: async (request) => {
    sent.push(request);
    return { status: 200, headers: '', body: new TextEncoder().encode('[{"accountAlias":"account"}]') };
  },
  connect: () => { throw new Error('unexpected socket'); },
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>),
});

it('actual WASM requires an injected provider at construction and signs through it before HTTP', async () => {
  const { WasmEngine } = await loadNodeEngine();
  const options = JSON.stringify({ signing: identities });
  const missing = new WasmEngine('binance', options);
  expect(() => new EngineHost(missing, makeTransport([]), {})).toThrow();
  missing.free?.();
  const sent: Parameters<Transport['request']>[0][] = [];
  const batches: SignRequest[] = [];
  const host = new EngineHost(new WasmEngine('binance', options), makeTransport(sent), {}, {
    ownsEngine: true,
    signer: async (work) => {
      batches.push(work);
      return work.operations.map((operation) => {
        if (operation.kind === 'present') {
          expect(operation.key).toBe(identities.primary.credential);
          return Array.from(new TextEncoder().encode('public-key-only'));
        }
        if (operation.kind !== 'sign' || operation.payload.some((part) => part.kind !== 'bytes')) throw new Error('unexpected primitive');
        expect(operation.key).toBe(identities.primary.signing);
        expect(operation.algorithm).toBe('hmacSha256');
        const bytes = operation.payload.flatMap((part) => part.kind === 'bytes' ? part.bytes : []);
        return Array.from(createHmac('sha256', vector.secret).update(Uint8Array.from(bytes)).digest());
      });
    },
  });
  try {
    await expect(host.command({ op: 'fetchAccount' })).resolves.toEqual({ userId: 'account' });
    expect(batches).toHaveLength(1);
    expect(JSON.stringify(batches)).not.toContain(vector.secret);
    expect(sent).toHaveLength(1);
    const url = new URL(sent[0].url);
    const signature = url.searchParams.get('signature');
    const canonical = url.search.slice(1).split('&signature=')[0];
    expect(signature).toBe(createHmac('sha256', vector.secret).update(canonical).digest('hex'));
    expect(url.searchParams.get('timestamp')).toBe('1700000000000');
    expect(sent[0].headers['X-MBX-APIKEY']).toBe('public-key-only');
  } finally { host.dispose(); }
});

it('actual WASM provider refusal is typed notSent and never becomes successful empty history', async () => {
  const { WasmEngine } = await loadNodeEngine();
  for (const command of [{ op: 'fetchAccount' }, { op: 'fetchOrderHistory', symbol: 'BTCUSDT' }]) {
    const sent: Parameters<Transport['request']>[0][] = [];
    const host = new EngineHost(new WasmEngine('binance', JSON.stringify({ signing: identities })), makeTransport(sent), {}, {
      ownsEngine: true,
      signer: () => { throw new Error('audit-secret-sentinel'); },
    });
    try {
      await expect(host.command(command)).rejects.toMatchObject({ outcome: 'notSent', kind: 'invalidInput' });
      expect(sent).toEqual([]);
    } finally { host.dispose(); }
  }
});

it('actual WASM consumes separate local setup and rejects malformed or unsupported setup without echoing it', async () => {
  const { WasmEngine } = await loadNodeEngine();
  const setup = JSON.stringify({ primary: { key: 'public-key-only', secret: vector.secret } });
  for (const [venue, options, credentials] of [
    ['binance', '{', setup],
    ['bybit', '{}', setup],
    ['binance', '{}', '{"primary":{"secret":"audit-secret-sentinel"'],
  ]) {
    let failure: unknown;
    try { new WasmEngine(venue, options, credentials).free?.(); } catch (error) { failure = error; }
    expect(failure).toBeInstanceOf(Error);
    expect(String(failure)).not.toContain(vector.secret);
    expect(String(failure)).not.toContain('audit-secret-sentinel');
  }
  const sent: Parameters<Transport['request']>[0][] = [];
  const engine = new WasmEngine('binance', JSON.stringify({ publicMode: { requested: false } }), setup);
  expect(JSON.parse(engine.construction!())).toMatchObject({ canSign: true, publicDataOnly: false });
  const host = new EngineHost(engine, makeTransport(sent), {}, { ownsEngine: true });
  try {
    await expect(host.command({ op: 'fetchAccount' })).resolves.toEqual({ userId: 'account' });
    expect(sent).toHaveLength(1);
    const [canonical, signature] = new URL(sent[0].url).search.slice(1).split('&signature=');
    expect(signature).toBe(createHmac('sha256', vector.secret).update(canonical).digest('hex'));
  } finally { host.dispose(); }
});


it('actual WASM refuses external-provider override on an unmigrated family', async () => {
  const { WasmEngine } = await loadNodeEngine();
  const engine = new WasmEngine('bybit', JSON.stringify({ publicDataOnly: true }));
  try {
    expect(() => new EngineHost(engine, makeTransport([]), {}, { signer: () => [] })).toThrow(/not supported/);
  } finally { engine.free?.(); }
});
