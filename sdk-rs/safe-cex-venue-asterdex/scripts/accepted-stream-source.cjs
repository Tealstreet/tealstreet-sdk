// Independently authored contract expectations checked against maintained TS bodies.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');
const printer = ts.createPrinter();
const parse = file => ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), ts.ScriptTarget.Latest, true);
const publicAst = parse('packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts');
const privateAst = parse('packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts');
const exchangeAst = parse('packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
const print = node => printer.printNode(ts.EmitHint.Unspecified, node, node.getSourceFile());
const declarations = [parse('packages/safe-cex/src/types.ts'), parse('packages/logging/src/LogSeverity.ts')]
  .flatMap(ast => ast.statements.filter(ts.isEnumDeclaration)).map(print).join('\n');
const compile = source => ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const enums = {};
new Function('exports', compile(declarations))(enums);
const partial = publicAst.statements.filter(ts.isVariableStatement).flatMap(s => [...s.declarationList.declarations])
  .find(node => node.name.getText() === 'PARTIAL_DEPTH_STREAM');
assert(partial && ts.isRegularExpressionLiteral(partial.initializer));
const partialDepth = new Function(`return ${partial.initializer.getText()}`)();
function methods(ast, names, className, reconnecting = false) {
  const cls = ast.statements.find(ts.isClassDeclaration);
  const members = names.map(name => {
    const member = cls.members.find(node => node.name?.getText() === name);
    assert(member, `missing maintained source member ${name}`);
    return print(member);
  });
  const exports = {};
  new Function('exports', 'PARTIAL_DEPTH_STREAM', 'ENABLE_RECONNECTING_WEBSOCKET', 'WebSocket', 'performance', ...Object.keys(enums),
    compile(`export class ${className} { ${members.join('\n')} }`))
    (exports, partialDepth, reconnecting, { OPEN: 1 }, { now: () => 1234 }, ...Object.values(enums));
  return exports[className];
}
const Pub = methods(publicAst, ['PING_REQUEST_ID', 'handleEventData', 'handleStreamData', 'handleTickerUpdate',
  'handleMarkPriceUpdate', 'handleTradeUpdate', 'handleKlineUpdate', 'subscribe', 'unsubscribe', 'ping'], 'AsterDEXPublicWebsocket');
const Mapping = methods(exchangeAst, ['reverseMapOrderStatus'], 'SourceMapping');
const numberBits = value => { const buffer = Buffer.alloc(8); buffer.writeDoubleBE(value); return buffer.toString('hex'); };
const bits = value => value === undefined ? null : numberBits(value);
const guarded = fn => { try { return fn(); } catch { return { ok: false }; } };
function ticker(input, mark) {
  const instance = new Pub(), original = { symbol: 'BTC' }, table = input.own ? { BTC: original } : {};
  let update, target;
  instance.parent = { store: { tickersBySymbol: table, updateTicker: (row, patch) => { target = row; update = patch; } }, emitter: { emit: () => {} } };
  try { instance[mark ? 'handleMarkPriceUpdate' : 'handleTickerUpdate'](input.data); }
  catch (error) { return { ok: false, error: error.name }; }
  if (!update) return { ok: true, kind: 'missing', same: false };
  const out = { ok: true, kind: target === original ? 'own' : 'inherited', same: target === original,
    symbol: input.data.s, bits: (mark ? ['mark', 'index', 'fundingRate'] : ['last', 'percentage', 'volume', 'quoteVolume']).map(key => bits(update[key])) };
  if (mark) out.fundingTime = update.fundingTime ?? null;
  return out;
}
async function observe(name, input) {
  switch (name) {
    case 'public_stream::string': case 'private_stream::text':
      return guarded(() => { const text = String(input.value); return { ok: true, text, units: text.length }; });
    case 'private_stream::optional_id': return guarded(() => { const text = input.value?.toString(); return { ok: true, present: text !== undefined, text: text ?? null }; });
    case 'private_stream::parsed': return guarded(() => { const n = parseFloat(input.value || '0'); return { ok: true, bits: numberBits(n), nan: Number.isNaN(n) }; });
    case 'private_stream::status': return guarded(() => { const status = new Mapping().reverseMapOrderStatus(String(input.value));
      return typeof status === 'string' ? { ok: true, status, terminal: status !== 'open' } : { ok: false }; });
    case 'private_stream::stringify': { const text = JSON.stringify(input); return { ok: true, text, units: text.length }; }
    case 'ws_helpers::is_partial_depth_stream': return partialDepth.test(input);
    case 'ws_helpers::public_ticker_update': return ticker(input, false);
    case 'ws_helpers::public_mark_price_update': return ticker(input, true);
    case 'ws_helpers::public_kline_update': {
      const instance = new Pub(); let candle, stream;
      instance.klineCallbacks = new Map(input.callbacks.map(key => [key, value => { candle = value; stream = key; }]));
      try { instance.handleKlineUpdate(input.data); }
      catch (error) { return { ok: false, error: error.name }; }
      return candle ? { ok: true, present: true, stream, timestamp: candle.timestamp ?? null,
        hasTimestamp: candle.timestamp !== undefined, bits: ['open', 'high', 'low', 'close', 'volume'].map(key => numberBits(candle[key])) }
        : { ok: true, present: false, stream: null };
    }
    case 'ws_helpers::public_stream_trade_update': {
      const instance = new Pub(), original = {}, table = input.own ? { BTC: original } : {}; let event, target, last;
      instance.parent = { store: { tickersBySymbol: table, updateTicker: (row, patch) => { target = row; last = patch.last; } }, emitter: { emit: (_, data) => { event = data; } } };
      instance.handleTradeUpdate(input.trade);
      // ticker_last is the helper's parsed candidate, even when the host has no ticker to patch.
      return { ok: true, kind: target ? target === original ? 'own' : 'inherited' : 'missing', same: target === original,
        symbol: event.symbol, side: event.side, bits: [event.price, event.qty, event.timestamp, target ? last : event.price].map(numberBits) };
    }
    case 'ws_helpers::public_event_data_dispatch': {
      const instance = new Pub(); let seen, event;
      instance.messageHandlers = Object.fromEntries(input.handlers.map(key => [key, [data => { seen = data; }]]));
      instance.recordMessageHandlerStats = key => { event = String(key); };
      try { instance.handleEventData(input.data); }
      catch (error) { return event ? { ok: true, kind: 'inherited', event } : { ok: false, error: error.name }; }
      return seen ? { ok: true, kind: 'handlers', event, data: seen } : { ok: true, kind: 'ignore', event: null };
    }
    case 'ws_helpers::public_stream_dispatch': {
      const instance = new Pub(); let kind;
      instance.handleDepthSnapshot = () => { kind = 'depth'; };
      instance.recordMessageHandlerStats = () => {};
      instance.messageHandlers = Object.fromEntries(['24hrTicker', 'aggTrade', 'markPriceUpdate', 'kline'].map(key => [key, [() => { kind = key; }]]));
      const data = input.symbol ? { s: 'already-supplied' } : {};
      try { instance.handleStreamData(input.stream, data); }
      catch { return { ok: false, error: 'streamType.split is not a function' }; }
      return { ok: true, kind: kind ?? 'ignore', assigned: !input.symbol && kind ? data.s : null };
    }
    case 'ws_helpers::private_ping_with': case 'ws_helpers::public_ping_with': {
      const Private = methods(privateAst, ['ping'], 'SourcePrivate');
      const instance = name.endsWith('private_ping_with') ? new Private() : new Pub(), calls = [];
      instance.isDisposed = input.disposed;
      Object.defineProperty(instance, 'pingAt', { set: () => calls.push('mark') });
      instance.ws = { readyState: input.connected ? 1 : 0, send: message => { calls.push(message); if (input.fail) throw Error('send refused'); } };
      try { instance.ping(); return { ok: true, calls }; } catch { return { ok: false, calls }; }
    }
    case 'ws_helpers::private_on_open': {
      const Private = methods(privateAst, ['onOpen'], 'SourcePrivate', input.reconnecting), instance = new Private(), calls = [];
      instance.isDisposed = input.disposed;
      Object.defineProperty(instance, 'connectRetryCount', { set: count => calls.push(`reset:${count}`) });
      instance.parent = { log: (message, severity) => calls.push(`log:${Object.keys(enums.LogSeverity).find(key => enums.LogSeverity[key] === severity)}:${message}`), fetchAndSetPrivateSnapshot: count => calls.push(`snapshot:${count}`) };
      instance.ping = () => { calls.push('ping'); if (input.fail) throw Error('ping refused'); };
      try { instance.onOpen(); return { ok: true, calls }; } catch { return { ok: false, calls }; }
    }
    case 'ws_helpers::private_listen_key_expired': {
      const Private = methods(privateAst, ['handleListenKeyExpired'], 'SourcePrivate'), instance = new Private(), calls = [...input.before];
      instance.parent = { log: (message, severity) => calls.push(`log:${Object.keys(enums.LogSeverity).find(key => enums.LogSeverity[key] === severity)}:${message}`) };
      instance.clearListenKeyRefreshTimer = () => calls.push('clear-refresh');
      instance.clearListenKeyWatchdogTimer = () => calls.push('clear-watchdog'); instance.reconnect = () => calls.push('reconnect');
      instance.handleListenKeyExpired(); return { ok: true, last: calls.at(-1), calls };
    }
    case 'ws_helpers::public_subscribe_plan': case 'ws_helpers::public_unsubscribe_plan': {
      const instance = new Pub(); let message;
      instance.ws = { readyState: input.connected ? 1 : 0 }; instance.subscriptionId = input.id;
      instance.send = text => { message = text; };
      const mutations = []; instance.topics = new Proxy(structuredClone(input.topics ?? {}), { set(object, key, value) { object[key] = value; return true; },
        get(object, key) { const value = object[key]; if (!Array.isArray(value)) return value;
          return new Proxy(value, { get(array, member) { if (member !== 'push') return array[member]; return symbol => { array.push(symbol); mutations.push([key, [...array]]); }; } }); } });
      // Observe assignments of full depth/kline topic rows and subsequent array pushes.
      const topics = instance.topics;
      instance.topics = new Proxy(topics, { set(object, key, value) { object[key] = value; if (key.includes('@')) mutations.push([key, [...value]]); return true; } });
      await instance[name.endsWith('public_subscribe_plan') ? 'subscribe' : 'unsubscribe'](input.streams);
      const out = { ok: true, message: message ?? null, next: message === undefined ? null : instance.subscriptionId };
      if (name.endsWith('public_subscribe_plan')) out.mutations = message === undefined ? null : mutations;
      return out;
    }
    default: throw Error(`unreviewed source adapter ${name}`);
  }
}
(async () => {
  const { loadContracts } = await import('../../safe-cex-parity/scripts/accepted-contract-files.mjs');
  const contracts = loadContracts(process.argv[2]); let owners = 0, cases = 0;
  const selected = [
    ...['is_partial_depth_stream', 'private_listen_key_expired', 'private_on_open',
      'private_ping_with', 'public_event_data_dispatch', 'public_kline_update',
      'public_mark_price_update', 'public_ping_with', 'public_stream_dispatch',
      'public_stream_trade_update', 'public_subscribe_plan', 'public_ticker_update',
      'public_unsubscribe_plan'].map(name => `ws_helpers::${name}`),
    ...['optional_id', 'parsed', 'status', 'stringify', 'text'].map(name => `private_stream::${name}`),
    'public_stream::string',
  ];
  for (const name of selected) {
    const key = `safe_cex_venue_asterdex::${name}`, contract = contracts[key];
    assert.equal(contract?.category, 'decoder', key);
    owners++;
    for (const row of contract.cases) { assert.deepEqual(await observe(name, structuredClone(row.input)), row.output, `${key}: actual TS source for ${JSON.stringify(row.input)}`); cases++; }
  }
  assert.equal(owners, 19, 'all assigned stream owners must have live-source observations');
  console.log(`Aster stream source: ${owners} owners / ${cases} expectations PASS.`);
})().catch(error => { console.error(error); process.exitCode = 1; });
