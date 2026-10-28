// Manual credentialed probe. Tests inject a scripted transport; never run in CI
// against a venue account. Credentials enter only through liveSmokeProfile, from
// the environment or one JSON object on stdin (--credentials-stdin).
import { closeSync, mkdirSync, openSync, writeSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { setTimeout as sleep } from 'node:timers/promises';
import { EngineHost } from '../hosts/wasm/js/host.mjs';
import { gateTransport } from './gate-transport.mjs';
import { prepareFetch } from '../hosts/wasm/js/managed-http.mjs';
import { Frame, LOADED_FLAG, MARKET_COLUMNS, MARKET_FLAG, orderEnums } from '../hosts/wasm/js/wire.mjs';
import { loadEngine } from './engine.mjs';
import { venueOf } from './venues.mjs';
import { liveSmokeCredentialSets, liveSmokeProfile } from './live-smoke-venues.mjs';
import { createPrivateRecorder } from './live-smoke-tape.mjs';
import { venueMarketRules } from './live-smoke-rules.mjs';

const REQUIRED = Object.values(LOADED_FLAG).reduce((mask, bit) => mask | bit, 0);
const positive = (value) => Number.isFinite(value) && value > 0;
class SmokeFailure extends Error {}
const fail = (code) => { throw new SmokeFailure(code); };
const snap = (value) => Number(value.toPrecision(14));
// The step-count tolerance is relative: a fixed epsilon is below one ULP once
// value/step exceeds about 1e6, so an exact multiple could round a step away.
const units = (value, step, ceil = false) => {
  const count = value / step, nearest = Math.round(count);
  const whole = Math.abs(count - nearest) <= 1e-9 * Math.max(1, Math.abs(count)) ? nearest :
    ceil ? Math.ceil(count) : Math.floor(count);
  return snap(whole * step);
};

export function accountSnapshot(buffer) {
  const frame = new Frame(buffer);
  const table = frame.markets();
  const markets = Array.from({ length: table?.count ?? 0 }, (_, i) => ({
    index: i, id: table.id(i), symbol: table.symbol(i), base: table.base(i), quote: table.quote(i), flags: table.flags[i],
    ...Object.fromEntries(MARKET_COLUMNS.map((column) => [column, table[column][i]])),
  }));
  const orderTable = frame.orders();
  const allOrders = Array.from({ length: orderTable?.count ?? 0 }, (_, i) => ({
    id: orderTable.id(i), symbol: markets[orderTable.market[i]]?.symbol,
    price: orderTable.price[i], amount: orderTable.amount[i],
    filled: orderTable.filled[i], remaining: orderTable.remaining[i], ...orderEnums(orderTable.enums[i]),
  }));
  const settings = frame.settings();
  return {
    status: frame.status(), markets, allOrders,
    orders: allOrders.filter((order) => !['canceled', 'closed'].includes(order.status)),
    positions: frame.positions()?.count ?? 0, balance: frame.balance(),
    settings: Array.from({ length: settings?.count ?? 0 }, (_, i) => ({
      symbol: markets[settings.market[i]]?.symbol, leverage: settings.leverage[i],
      buyLeverage: settings.buyLeverage[i], sellLeverage: settings.sellLeverage[i],
    })),
  };
}

// Inspect discovery replies, not guessed per-venue monetary defaults. The
// selected minimum is never increased after a venue rejection.
export const marketRules = (records, market, venue = 'binance') => venueMarketRules(venue, records, market);

// A venue minimum never authorizes an unbounded live write. This is deliberately
// fixed in code: neither environment variables nor CLI sizing flags raise it.
export const LIVE_SMOKE_MAX_NOTIONAL = 100;
const dollarQuotes = new Set(['USDT', 'USDC', 'USD', 'pUSD', 'SUSDT', 'SUSDC']);

export function smokeOrder(market, book, rules = {}, postOnly = true) {
  if (rules.notionalKnown !== true) fail('unknown-minimum-notional');
  if (!dollarQuotes.has(market.quote)) fail('unknown-notional-currency');
  if (!(market.flags & MARKET_FLAG.ACTIVE) || market.flags & (MARKET_FLAG.READ_ONLY | MARKET_FLAG.EXPIRING)) fail('market-unavailable');
  const tick = market.precisionPrice;
  const step = rules.amountStep ?? market.precisionAmount;
  const minAmount = rules.minimumAmount ?? market.minAmount;
  const maxAmount = rules.maximumAmount ?? market.maxAmount;
  if (![tick, step, minAmount].every(positive) || !positive(maxAmount) || maxAmount < minAmount) fail('unknown-market-limits');
  const bid = book?.bids?.[0], ask = book?.asks?.[0];
  if (!positive(bid) || !positive(ask) || bid >= ask) fail('invalid-book');
  const floor = Math.max(positive(market.minPrice) ? market.minPrice : tick, (rules.lowerMultiplier ?? 0) * (book.mark ?? ask));
  const price = units(Math.max(bid * 0.9, floor + tick * 2), tick);
  const amendPrice = units(Math.max(bid * 0.89, floor + tick), tick);
  if (!positive(price) || !positive(amendPrice) || price >= bid * 0.98 || amendPrice >= price || amendPrice < floor) fail('no-resting-price');
  const quoteQuantity = rules.quoteQuantity ?? Boolean(market.flags & MARKET_FLAG.INVERSE);
  const scale = rules.notionalPerAmount ?? (quoteQuantity && positive(market.contractSize) ? market.contractSize : 1);
  if (!positive(scale)) fail('unknown-placement-units');
  const perUnit = quoteQuantity ? scale : amendPrice * scale;
  const minimumNotional = rules.minNotional ?? 0;
  if (!Number.isFinite(minimumNotional) || minimumNotional < 0) fail('unknown-minimum-notional');
  const amount = units(Math.max(minAmount, minimumNotional / perUnit), step, true);
  if (!positive(amount) || amount > maxAmount) fail('minimum-exceeds-market-limit');
  const notional = amount * scale * (quoteQuantity ? 1 : Math.max(price, amendPrice));
  if (!positive(notional) || notional > LIVE_SMOKE_MAX_NOTIONAL) fail('notional-cap-exceeded');
  return { symbol: market.symbol, type: 'limit', side: 'buy', amount, price, amendPrice,
    ...(postOnly ? { timeInForce: 'PostOnly' } : {}) };
}

export const selectSmokeMarket = (markets, symbol) => markets.find((market) =>
  dollarQuotes.has(market.quote) && (symbol ? market.symbol === symbol :
    (market.flags & MARKET_FLAG.ACTIVE) && !(market.flags & (MARKET_FLAG.READ_ONLY | MARKET_FLAG.EXPIRING))));

export function smokeArgs(args) {
  const { values } = parseArgs({ args, options: {
    venue: { type: 'string' }, symbol: { type: 'string' }, out: { type: 'string' },
    testnet: { type: 'boolean' }, demo: { type: 'boolean' }, mainnet: { type: 'boolean' },
    'timeout-ms': { type: 'string', default: '30000' }, help: { type: 'boolean' },
    'credentials-stdin': { type: 'boolean' }, 'dry-run': { type: 'boolean' }, 'list-credentials': { type: 'boolean' },
  } });
  if (values.help) return { help: true };
  if (values['list-credentials']) return { listCredentials: true };
  if (!values.venue) fail('venue-required');
  const modes = ['testnet', 'demo', 'mainnet'].filter((mode) => values[mode]);
  if (modes.length > 1) fail('conflicting-environments');
  const timeoutMs = Number(values['timeout-ms']);
  if (!Number.isInteger(timeoutMs) || timeoutMs < 100 || timeoutMs > 120000) fail('invalid-timeout');
  return { venue: values.venue, symbol: values.symbol, out: values.out, mode: modes[0], timeoutMs,
    credentialsStdin: Boolean(values['credentials-stdin']), dryRun: Boolean(values['dry-run']) };
}

const deadline = async (promise, ms, stage, signal) => {
  const controller = new AbortController();
  let aborted;
  try {
    return await Promise.race([promise, sleep(ms, undefined, { signal: controller.signal }).then(() => fail(`timeout-${stage}`)),
      new Promise((_, reject) => {
        aborted = () => reject(new Error('runner-interrupted'));
        if (signal?.aborted) aborted();
        else signal?.addEventListener('abort', aborted, { once: true });
      })]);
  } finally { controller.abort(); signal?.removeEventListener('abort', aborted); }
};

export async function runLiveSmoke({ venue, profile, transport, write, symbol, timeoutMs = 30000, WasmEngine, signal }) {
  let stage = 'connect', host, flush, selected, originalLeverage;
  let reason;
  let wrote = false, filled = false, successful = false, cleanupComplete = false, stopped = false, cleaning = false;
  // Mainnet runs place, confirm, cancel and read back only; the rest is reported, not passed.
  const skipped = profile.minimal ? ['amend', 'lower-leverage'] : [];
  const ids = new Set(), books = new Map();
  const pending = new Set();
  const recorder = createPrivateRecorder(transport, { venue, options: profile.options, secrets: profile.secrets, write });
  const commands = [];
  const command = async (body) => {
    commands.push(body.op);
    const work = Promise.resolve().then(() => recorder.command(body, () => host.command(body)));
    pending.add(work);
    void work.then(() => pending.delete(work), () => pending.delete(work));
    return deadline(work, timeoutMs, body.op, cleaning ? undefined : signal);
  };
  const snapshot = () => accountSnapshot(host.snapshot());
  const wait = async (predicate) => {
    const end = performance.now() + timeoutMs;
    while (!stopped) {
      if (recorder.failure) fail('recording-failed');
      if (signal?.aborted && !cleaning) fail('runner-interrupted');
      if (wrote && (filled || snapshot().positions > 0)) fail('unexpected-execution');
      if (predicate()) return;
      if (performance.now() >= end) fail(`timeout-${stage}`);
      await sleep(10);
    }
    fail('runner-stopped');
  };
  const healthy = () => {
    if (recorder.failure) fail('recording-failed');
    if (filled || snapshot().positions > 0 || snapshot().allOrders.some((o) => ids.has(o.id) && o.filled > 0)) fail('unexpected-execution');
  };
  const currentBook = () => {
    const book = books.get(selected.symbol);
    if (!book || performance.now() - book.at > 10000) fail('stale-book');
    return book;
  };
  const cancelAll = async () => {
    // Match the SDK's generic cancelAllOrders: venues without a global endpoint
    // cancel by observed symbol. The empty-account precondition bounds this set.
    const symbols = new Set([selected.symbol, ...snapshot().orders.map((o) => o.symbol)]);
    let failed = false;
    for (const symbol of symbols) {
      if (!symbol) { failed = true; continue; }
      // An Unknown write fences cancel-all (NotSent) until a fresh order read reconciles it.
      const end = performance.now() + timeoutMs;
      for (;;) {
        try { await command({ op: 'cancelSymbolOrders', symbol }); break; }
        catch (error) {
          if (error?.outcome !== 'notSent' || !String(error?.message).includes('reconcile before cancel-all') ||
              performance.now() >= end) { failed = true; break; }
          await sleep(25);
        }
      }
    }
    if (failed) fail('cancel-all-incomplete');
  };
  try {
    host = new EngineHost(new WasmEngine(venue.rust, JSON.stringify(profile.options)), recorder.transport, {
      onFrame(buffer) {
        const frame = new Frame(buffer), current = snapshot();
        for (const book of frame.books()) {
          const name = current.markets[book.market]?.symbol;
          if (name) books.set(name, { bids: [...book.bids], asks: [...book.asks], at: performance.now() });
        }
        if ((frame.fills()?.count ?? 0) > 0) filled = true;
      },
      // Venue logs may contain credentials or echoed request bodies.
      onLog() {},
    });
    host.start();
    flush = setInterval(() => host.flush(), 25);
    await wait(() => ((snapshot().status?.loaded ?? 0) & REQUIRED) === REQUIRED);
    stage = 'account-snapshot';
    const initial = snapshot();
    if (initial.orders.length || initial.positions) fail('account-not-empty');
    if (initial.status.hedged) fail('hedged-account-unsupported');
    if (!initial.balance || !Number.isFinite(initial.balance.free) || initial.balance.free <= 0) fail('account-not-funded');
    selected = selectSmokeMarket(initial.markets, symbol);
    if (!selected) fail('market-not-found');
    const setting = initial.settings.find((s) => s.symbol === selected.symbol);
    originalLeverage = setting?.leverage;
    if (!positive(originalLeverage) && (selected.flags & MARKET_FLAG.SPOT || profile.leverage === 'fixed')) originalLeverage = 1;
    if (!positive(originalLeverage) || (positive(setting?.buyLeverage) && setting.buyLeverage !== originalLeverage) ||
        (positive(setting?.sellLeverage) && setting.sellLeverage !== originalLeverage)) fail('unknown-original-leverage');
    if (profile.leverage !== 'fixed' && (!positive(selected.minLeverage) || !positive(selected.maxLeverage) ||
        originalLeverage < selected.minLeverage || originalLeverage > selected.maxLeverage)) fail('leverage-outside-engine-limits');
    recorder.markReady?.();
    stage = 'book';
    await host.command({ op: 'subscribeOrderBook', symbol: selected.symbol });
    await wait(() => books.has(selected.symbol));
    const rules = marketRules(recorder.records, selected, venue);
    const { amendPrice, ...order } = smokeOrder(selected, currentBook(), rules, profile.postOnly !== false);
    if (snapshot().orders.length || snapshot().positions) fail('account-changed-before-placement');
    healthy();
    stage = 'place';
    wrote = true;
    const placed = await command({ op: 'placeOrders', orders: [order] });
    if (!Array.isArray(placed) || placed.length !== 1 || typeof placed[0] !== 'string' || !placed[0]) fail('unproven-placement');
    ids.add(placed[0]);
    await wait(() => snapshot().orders.some((o) => o.id === placed[0] && o.status === 'open'));
    healthy();
    if (!skipped.includes('amend')) {
      stage = 'amend';
      // Recheck the live book: the market may have moved during placement.
      if (amendPrice >= currentBook().bids[0] * 0.98) fail('market-moved');
      const amended = await command({ op: 'amendOrder', id: placed[0], price: amendPrice, amount: order.amount });
      const replacements = Array.isArray(amended) ? amended : amended?.orderIds;
      if (replacements !== undefined && (!Array.isArray(replacements) || replacements.some((id) => typeof id !== 'string' || !id))) fail('unproven-amendment');
      for (const id of replacements ?? []) ids.add(id);
      const amendedIds = replacements?.length ? replacements : [placed[0]];
      await wait(() => snapshot().orders.some((o) => amendedIds.includes(o.id) && o.status === 'open' &&
        Math.abs(o.price - amendPrice) < selected.precisionPrice / 10));
      healthy();
    }
    stage = 'cancel';
    const liveIds = snapshot().orders.filter((o) => ids.has(o.id) && o.status === 'open').map((o) => o.id);
    await command({ op: 'cancelOrders', ids: liveIds.length ? liveIds : [...ids].slice(-1) });
    stage = 'cancel-all';
    await cancelAll();
    await wait(() => snapshot().orders.length === 0);
    healthy();
    // Exercise a real configuration write when a lower leverage is available,
    // after all smoke orders have gone. Fixed/minimum leverage stays unchanged.
    if (!skipped.includes('lower-leverage') && profile.leverage !== 'fixed' && originalLeverage > selected.minLeverage) {
      stage = 'lower-leverage';
      const lower = Math.max(selected.minLeverage, Math.floor(originalLeverage - 1));
      await command({ op: 'setLeverage', symbol: selected.symbol, leverage: lower });
      healthy();
    }
    // A minimal run never changed leverage, so it writes no configuration at all.
    if (!profile.minimal) {
      stage = 'restore-leverage';
      await command({ op: 'setLeverage', symbol: selected.symbol, leverage: originalLeverage });
    }
    stage = 'verify-cleanup';
    await wait(() => snapshot().orders.length === 0);
    healthy();
    const restored = snapshot().settings.find((s) => s.symbol === selected.symbol)?.leverage;
    if (profile.leverage !== 'fixed' && restored !== originalLeverage) fail('unproven-leverage-restore');
    cleanupComplete = true;
    successful = true;
    stage = 'complete';
  } catch (error) {
    reason = error instanceof SmokeFailure ? error.message : 'engine-or-transport-failed';
    cleaning = true;
    for (const id of error?.partialOrderIds ?? []) if (typeof id === 'string') ids.add(id);
    // No write retry: even a lost placement acknowledgement can leave an order.
    if (wrote && selected) {
      let canceled = false, restored = false;
      let drained = false;
      try { await deadline(Promise.allSettled([...pending]), timeoutMs, 'drain-writes'); drained = true; } catch { /* A late write prevents cleanup proof. */ }
      try { await cancelAll(); canceled = true; } catch { /* Report cleanup incomplete. */ }
      try {
        if (!profile.minimal) await command({ op: 'setLeverage', symbol: selected.symbol, leverage: originalLeverage });
        restored = true;
      } catch { /* Still dispose the host. */ }
      cleanupComplete = drained && canceled && restored && snapshot().orders.length === 0 && !filled && snapshot().positions === 0;
    }
  } finally {
    stopped = true;
    clearInterval(flush);
    host?.flush();
    host?.dispose();
    recorder.seal?.();
  }
  // Snapshot() above is the mutation engine's cache. A new instance has no
  // optimistic rows: all loaded account sections must come from fresh reads.
  let authoritative = false;
  if (wrote && selected && cleanupComplete && !filled) {
    authoritative = await verifyLiveAccount({ venue, profile, transport, write, selected, originalLeverage, timeoutMs, WasmEngine, epoch: recorder.epoch });
  }
  if (successful && (filled || recorder.failure || !authoritative)) {
    successful = false;
    stage = filled ? 'unexpected-execution' : 'verify-readback';
    reason = stage;
  }
  cleanupComplete = cleanupComplete && !filled && authoritative;
  // `skipped` holds code constants only, so it is added after redaction.
  const result = { ...recorder.redact({ ok: successful, stage, ...(successful ? {} : { reason }),
    cleanupComplete, authoritative, ...(selected ? { symbol: selected.symbol } : {}), commands }),
  ...(skipped.length ? { skipped } : {}) };
  write({ t: Math.max(0, transport.now() - recorder.epoch), type: 'live-smoke-result', ...result });
  return result;
}

export async function verifyLiveAccount({ venue, profile, transport, write, selected, originalLeverage, timeoutMs, WasmEngine, epoch }) {
  let host, flush, filled = false, passed = false;
  const recorder = createPrivateRecorder(transport, { venue, options: profile.options, secrets: profile.secrets, epoch,
    write: (record) => write({ ...record, type: `live-smoke-verify-${record.type}` }) });
  try {
    host = new EngineHost(new WasmEngine(venue.rust, JSON.stringify(profile.options)), recorder.transport, {
      onFrame(buffer) { if ((new Frame(buffer).fills()?.count ?? 0) > 0) filled = true; }, onLog() {},
    });
    host.start();
    flush = setInterval(() => host.flush(), 25);
    const end = performance.now() + timeoutMs;
    for (;;) {
      if (recorder.failure || filled) fail('verification-failed');
      const state = accountSnapshot(host.snapshot());
      if (((state.status?.loaded ?? 0) & REQUIRED) === REQUIRED) {
        const setting = state.settings.find((s) => s.symbol === selected.symbol);
        const leverage = setting?.leverage ?? (profile.leverage === 'fixed' ? 1 : NaN);
        if (state.orders.length || state.positions || state.status.hedged || leverage !== originalLeverage ||
            (positive(setting?.buyLeverage) && setting.buyLeverage !== originalLeverage) ||
            (positive(setting?.sellLeverage) && setting.sellLeverage !== originalLeverage)) fail('verification-mismatch');
        passed = true;
        break;
      }
      if (performance.now() >= end) fail('verification-timeout');
      await sleep(10);
    }
  } catch { passed = false; }
  finally {
    clearInterval(flush);
    host?.flush();
    host?.dispose();
    recorder.seal();
  }
  passed = passed && !filled && !recorder.failure;
  write({ t: Math.max(0, transport.now() - recorder.epoch), type: 'live-smoke-verification', ok: passed });
  return passed;
}

const withoutPath = (options, path) => {
  const parts = path.split('.');
  const parent = parts.slice(0, -1).reduce((target, part) => target?.[part], options);
  if (parent) delete parent[parts.at(-1)];
};

/** Refuses any outgoing text that carries supplied credential material. */
export function dryRunTransport(live, secretValues) {
  const values = secretValues.filter((value) => typeof value === 'string' && value.length >= 4);
  const guard = (...texts) => {
    for (const text of texts) if (typeof text === 'string' && values.some((value) => text.includes(value))) fail('dry-run-credential-egress');
  };
  const headerText = (headers) => JSON.stringify(Array.isArray(headers) ? headers : Object.entries(headers ?? {}));
  return {
    ...live,
    connect(url, handlers) {
      guard(url, headerText(handlers?.headers));
      const socket = live.connect(url, handlers);
      return { close: () => socket.close(), send(text) { guard(text); socket.send(text); } };
    },
    prepareHttp: typeof live.prepareHttp !== 'function' ? undefined : async (request) => {
      guard(request.url, headerText(request.headers), request.body);
      const prepared = await live.prepareHttp(request);
      return { ...prepared, bind(packet) {
        guard(packet.url, headerText(packet.headers), typeof packet.body === 'string' ? packet.body : undefined);
        return prepared.bind(packet);
      } };
    },
    request(request) {
      guard(request.url, headerText(request.headers), typeof request.body === 'string' ? request.body : undefined);
      return live.request(request);
    },
  };
}

/**
 * Everything up to the first signed request: supplied credentials are parsed by
 * the engine constructor but never started; a public-only engine discovers the
 * market, reads the book and sizes the order the live run would place.
 */
export async function dryRunLiveSmoke({ venue, profile, transport, write, symbol, timeoutMs = 30000, WasmEngine, signal }) {
  let stage = 'credentials', host, flush, selected, plan, reason, ok = false;
  let credentials = 'absent';
  const books = new Map();
  const recorder = createPrivateRecorder(transport, { venue, options: profile.options, secrets: profile.secrets, write });
  try {
    if (Object.keys(profile.secrets).length) {
      const engine = new WasmEngine(venue.rust, JSON.stringify(profile.options));
      engine.free?.();
      credentials = 'parsed';
    }
    stage = 'connect';
    const options = structuredClone(profile.options);
    for (const path of Object.keys(profile.secrets)) withoutPath(options, path);
    options.publicDataOnly = true;
    host = new EngineHost(new WasmEngine(venue.rust, JSON.stringify(options)), recorder.transport, {
      onFrame(buffer) {
        const frame = new Frame(buffer), markets = accountSnapshot(host.snapshot()).markets;
        for (const book of frame.books()) {
          const name = markets[book.market]?.symbol;
          if (name) books.set(name, { bids: [...book.bids], asks: [...book.asks], at: performance.now() });
        }
      },
      onLog() {},
    });
    host.start();
    flush = setInterval(() => host.flush(), 25);
    const wait = async (predicate) => {
      const end = performance.now() + timeoutMs;
      while (!predicate()) {
        if (recorder.failure) fail('recording-failed');
        if (signal?.aborted) fail('runner-interrupted');
        if (performance.now() >= end) fail(`timeout-${stage}`);
        await sleep(10);
      }
    };
    await wait(() => ((accountSnapshot(host.snapshot()).status?.loaded ?? 0) & LOADED_FLAG.MARKETS) !== 0);
    stage = 'market';
    selected = selectSmokeMarket(accountSnapshot(host.snapshot()).markets, symbol);
    if (!selected) fail('market-not-found');
    stage = 'book';
    await host.command({ op: 'subscribeOrderBook', symbol: selected.symbol });
    await wait(() => books.has(selected.symbol));
    stage = 'plan';
    const { amendPrice, ...order } = smokeOrder(selected, books.get(selected.symbol), marketRules(recorder.records, selected, venue),
      profile.postOnly !== false);
    plan = { ...order, ...(profile.minimal ? {} : { amendPrice }) };
    stage = 'complete';
    ok = true;
  } catch (error) {
    reason = error instanceof SmokeFailure ? error.message : 'engine-or-transport-failed';
  } finally {
    clearInterval(flush);
    host?.flush();
    host?.dispose();
    recorder.seal?.();
  }
  const sequence = ['place', 'confirm-open', ...(profile.minimal ? [] : ['amend']), 'cancel', 'cancel-all',
    ...(profile.minimal ? [] : ['lower-leverage', 'restore-leverage']), 'verify-readback'];
  // The plan is numbers sized from public data; the rest are code constants.
  const result = { ...recorder.redact({ ok, stage, ...(ok ? {} : { reason }), ...(selected ? { symbol: selected.symbol } : {}) }),
    dryRun: true, credentials, ...(plan ? { plan } : {}), sequence };
  write({ t: Math.max(0, transport.now() - recorder.epoch), type: 'live-smoke-dry-run', ...result });
  return result;
}

export function fencedTransport(live, profile) {
  return {
    ...live,
    connect(url, handlers) {
      if (!profile.allowUrl(url, 'ws')) fail('socket-endpoint-refused');
      profile.validateSocket?.(url, handlers);
      const socket = live.connect(url, handlers);
      return { close: () => socket.close(), send(text) {
        profile.validateSocketMessage?.(url, text);
        socket.send(text);
      } };
    },
    prepareHttp: typeof live.prepareHttp !== 'function' ? undefined : async (request) => {
      if (!profile.allowUrl(request.url, 'http')) fail('http-endpoint-refused');
      const prepared = await live.prepareHttp(request);
      return {
        expiresAt: prepared.expiresAt,
        cancel: () => prepared.cancel?.(),
        bind(packet) {
          try {
            if (!profile.allowUrl(packet.url, 'http')) fail('http-endpoint-refused');
            profile.validateRequest?.(packet);
            return prepared.bind(packet);
          } catch (error) {
            try { prepared.cancel?.(); } catch { /* Preserve the original refusal. */ }
            throw error;
          }
        },
      };
    },
    request(request) {
      if (!profile.allowUrl(request.url, 'http')) fail('http-endpoint-refused');
      profile.validateRequest?.(request);
      return live.request(request);
    },
  };
}

const MAX_CREDENTIAL_BYTES = 64 * 1024;

/** One JSON object of field names to string values; the text is never echoed. */
export async function readCredentials(stream) {
  const chunks = [];
  let size = 0;
  for await (const chunk of stream) {
    size += chunk.length;
    if (size > MAX_CREDENTIAL_BYTES) throw new Error('Credential input is too large');
    chunks.push(Buffer.from(chunk));
  }
  let value;
  try { value = JSON.parse(Buffer.concat(chunks).toString('utf8')); }
  catch { throw new Error('Credential input is not JSON'); }
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.values(value).some((item) => typeof item !== 'string'))
    throw new Error('Credential input must be one object of string fields');
  return value;
}

export async function main(args = process.argv.slice(2), env = process.env, stdin = process.stdin) {
  let fd;
  const controller = new AbortController();
  const interrupt = () => controller.abort();
  try {
    const values = smokeArgs(args);
    if (values.help) {
      console.log('yarn rust:live-smoke --venue <v> [--testnet|--demo|--mainnet] [--symbol <symbol>] [--out <private.jsonl>] [--timeout-ms 30000] [--credentials-stdin] [--dry-run]');
      console.log('yarn rust:live-smoke --list-credentials');
      return 0;
    }
    if (values.listCredentials) {
      console.log(JSON.stringify(liveSmokeCredentialSets(), null, 2));
      return 0;
    }
    const credentials = values.credentialsStdin ? await readCredentials(stdin) : undefined;
    const profile = liveSmokeProfile(values.venue, values.mode, env, { credentials, allowMissing: values.dryRun });
    const venue = venueOf(values.venue);
    const { WasmEngine } = await loadEngine();
    const out = values.out ?? `sdk-rs/harness/tapes/${values.venue}-live-${values.dryRun ? 'dry-run' : 'private'}.jsonl`;
    mkdirSync(dirname(out), { recursive: true });
    fd = openSync(out, 'wx', 0o600);
    const write = (record) => writeSync(fd, `${JSON.stringify(record)}\n`);
    process.on('SIGINT', interrupt);
    process.on('SIGTERM', interrupt);
    const network = gateTransport();
    // fetch's default follows redirects, which could escape the endpoint fence.
    network.prepareHttp = (request) => prepareFetch({ ...request,
      timeoutMs: Math.min(request.timeoutMs ?? 20000, values.timeoutMs, 20000) });
    network.request = async ({ method, url, headers, body, signal }) => {
      const timeout = AbortSignal.timeout(Math.min(values.timeoutMs, 20000));
      const response = await fetch(url, { method, headers, body, redirect: 'error', signal: signal ? AbortSignal.any([signal, timeout]) : timeout });
      return { status: response.status,
        headers: [...response.headers].map(([key, value]) => `${key}: ${value}`).join('\n'),
        body: response.body ?? new Uint8Array() };
    };
    const fenced = fencedTransport(network, profile);
    const result = values.dryRun ?
      await dryRunLiveSmoke({ ...values, venue, profile, transport: dryRunTransport(fenced, profile.secretValues), write, WasmEngine, signal: controller.signal }) :
      await runLiveSmoke({ ...values, venue, profile, transport: fenced, write, WasmEngine, signal: controller.signal });
    console.log(JSON.stringify({ venue: values.venue, environment: profile.mode, ...result }));
    return result.ok ? 0 : 1;
  } catch {
    // Do not expose a credential-bearing exception, argument, URL or account ID.
    console.error('Live smoke refused or failed. Check environment, credential names, build and exclusive output path; see docs/plans/RUST_REWRITE.md.');
    return 1;
  } finally {
    process.off('SIGINT', interrupt);
    process.off('SIGTERM', interrupt);
    if (fd !== undefined) closeSync(fd);
  }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) process.exitCode = await main();
