const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/httpOverWebsocket');
const control = process.argv[2];
assert.ok([undefined, 'omit-init', 'omit-domain-guard', 'wrong-domain'].includes(control));
function load(name, imports) {
  let source = fs.readFileSync(path.join(root, name), 'utf8');
  const changes = {
    'omit-init': ['initializeHOW();', ''],
    'omit-domain-guard': ['if (!metadata.latencyDomain)', 'if (false)'],
    'wrong-domain': ['getLatencyForDomain(metadata.latencyDomain)', "getLatencyForDomain('wrong.test')"],
  };
  if (control && ((control === 'omit-init') === (name === 'configureHOW.ts'))) {
    const [before, after] = changes[control];
    assert.ok(source.includes(before), 'wrapper mutation anchor');
    source = source.replace(before, after);
  }
  const exports = {};
  vm.runInNewContext(ts.transpileModule(source, {compilerOptions: {
    module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022,
  }}).outputText, {exports, require(name) {
    assert.ok(Object.hasOwn(imports, name), `unexpected dependency ${name}`);
    return imports[name];
  }});
  return exports;
}
const configured = [], clocks = [];
const token = () => Promise.resolve('token');
const expiry = () => false;
const domains = () => ['venue.test'];
const reverse = new Map([['venue.test', ['woo']]]);
const init = load('configureHOW.ts', {
  '@tealstreet/http-over-websocket/src/config': {configureHOW: value => configured.push(value)},
  '../accessToken': {getProxyToken: token, isTokenExpired: expiry},
  '../config/config': {SIMULATE_HTTP_OVER_WEBSOCKET: true},
  '../exchangeMetadata': {getUniqueLatencyDomains: domains, latencyDomainToExchanges: reverse},
  '../utils/virtualClock': {virtualClock: {syncBybitClientServerTime: value => clocks.push(value)}},
});
assert.equal(configured.length, 1, 'HOW-BRIDGE import initializes exactly once');
init.initializeHOW();
assert.equal(configured.length, 2, 'HOW-BRIDGE explicit initialization repeats');
for (const config of configured) {
  assert.equal(Object.keys(config).length, 6, 'HOW-BRIDGE six dependency bindings');
  assert.equal(config.getProxyToken, token);
  assert.equal(config.isTokenExpired, expiry);
  assert.equal(config.simulateHttpOverWebsocket, true);
  assert.equal(config.getUniqueLatencyDomains, domains);
  assert.equal(config.latencyDomainToExchanges, reverse);
  for (const value of [0, -0, 17.125, Infinity]) config.syncServerTime(value);
}
assert.equal(clocks.length, 8);
for (let i = 0; i < 8; i++) assert.ok(Object.is(clocks[i], [0, -0, 17.125, Infinity][i % 4]));
let active;
const trace = [];
const marker = {latency: 13, timestamp: 29};
const adapter = load('axiosHttpOverWebsocketAdapter.ts', {
  './configureHOW': init,
  '../types': {},
  '@tealstreet/http-over-websocket/src/types': {},
  '../exchangeMetadata': {getExchangeMetadata(exchange) {
    trace.push(['metadata', exchange]);
    if (active === 'unknown') throw new Error('Unknown exchange metadata: missing');
    return {latencyDomain: active};
  }},
  '@tealstreet/http-over-websocket/src/axiosHttpOverWebsocketAdapter': {getHOWWebSocketForExchange(exchange) {
    trace.push(['socket', exchange]);
    return {getLatencyForDomain(domain) {
      trace.push(['domain', domain]);
      if (active === 'throw.test') throw new Error('lookup failure');
      return active === 'missing.test' ? undefined : marker;
    }};
  }},
});
let comparisons = 0;
for (const domain of [undefined, '', 'venue.test', 'missing.test', 'unknown', 'throw.test']) {
  active = domain;
  trace.length = 0;
  if (domain === 'unknown' || domain === 'throw.test') {
    assert.throws(() => adapter.getServerLatencyForExchange('woo'), {message: domain === 'unknown' ? 'Unknown exchange metadata: missing' : 'lookup failure'});
  } else {
    assert.equal(adapter.getServerLatencyForExchange('woo'), domain && domain !== 'missing.test' ? marker : undefined, 'HOW-BRIDGE latency result identity');
  }
  assert.equal(trace.length, !domain || domain === 'unknown' ? 1 : 3, 'HOW-BRIDGE metadata guard precedes socket');
  assert.deepEqual(trace[0], ['metadata', 'woo']);
  if (domain && domain !== 'unknown') {
    assert.deepEqual(trace[1], ['socket', 'woo']);
    assert.deepEqual(trace[2], ['domain', domain], 'HOW-BRIDGE forwards metadata domain');
  }
  comparisons++;
}
assert.equal(comparisons, 6, 'HOW-BRIDGE independent comparison count');
console.log(`HOW wrapper source witnesses: 2 configurations, 8 clock forwards, 6 latency paths; ${process.arch} Node ${process.versions.node}`);
