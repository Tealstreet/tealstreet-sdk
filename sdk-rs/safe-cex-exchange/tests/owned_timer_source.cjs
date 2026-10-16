const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const source = process.binding('natives')['internal/timers'];
assert.equal(typeof source, 'string', 'NODE-INTERVAL actual timer implementation available');
const file = ts.createSourceFile('internal/timers.js', source, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
const matches = [];
function visit(node) {
  if (ts.isFunctionDeclaration(node) && node.name?.text === 'listOnTimeout') matches.push(node);
  ts.forEachChild(node, visit);
}
visit(file);
assert.equal(matches.length, 1, 'NODE-INTERVAL unique actual listOnTimeout body');
const body = matches[0].getText(file);
const rows = [];
for (const spec of [
  { name: 'fast', period: 10, captured: 100, cost: 3 },
  { name: 'slow', period: 5, captured: 100, cost: 25 },
  { name: 'late-loop', period: 5, captured: 143, cost: 0 },
  { name: 'throwing', period: 7, captured: 100, cost: 9, throws: true },
  { name: 'self-cancel', period: 11, captured: 100, cost: 13, cancel: true },
]) {
  let clock = spec.captured;
  let available = true;
  let inserted = null;
  const trace = [];
  const timer = {
    _idleStart: spec.captured - spec.period,
    _idleTimeout: spec.period,
    _repeat: spec.period,
    _onTimeout() {
      trace.push("callback");
      clock += spec.cost;
      if (spec.cancel) timer._idleTimeout = -1;
      if (spec.throws) throw new TypeError('source callback failure');
    },
  };
  const noop = () => {};
  const context = {
    debug: noop, MathMax: Math.max, timerListId: 0,
    L: { peek: () => available ? timer : null, remove: () => { available = false; } },
    runNextTicks: noop, binding: { getLibuvNow: () => clock },
    async_id_symbol: Symbol(), trigger_async_id_symbol: Symbol(), async_context_frame: Symbol(),
    kHasPrimitive: Symbol(), kRefed: Symbol(), knownTimersById: {}, timeoutInfo: [0],
    AsyncContextFrame: { exchange: noop, set: noop }, emitBefore: noop, emitAfter: noop, emitDestroy: noop,
    cleanTimer: noop, enqueueRemoveStoresIfNotReinserted: noop, removeStoresFromResource: noop,
    ReflectApply: Reflect.apply, timerListMap: {}, timerListQueue: { shift: noop, percolateDown: noop },
    insert: (_, period, captured) => { trace.push("rearm"); inserted = { period, captured, deadline: captured + period }; },
  };
  const run = vm.runInNewContext(`(${body})`, context);
  let error = null;
  try { run({ msecs: spec.period }, spec.captured); } catch (caught) { trace.push("report"); error = caught.message; }
  assert.equal(error, spec.throws ? 'source callback failure' : null, `NODE-INTERVAL ${spec.name} callback error identity`);
  assert.deepEqual(inserted, spec.cancel ? null : { period: spec.period, captured: spec.captured, deadline: spec.captured + spec.period }, `NODE-INTERVAL ${spec.name} captures before callback including finally`);
  assert.deepEqual(trace, ["callback", ...(spec.cancel ? [] : ["rearm"]), ...(spec.throws ? ["report"] : [])], `NODE-INTERVAL ${spec.name} rearm before report`);
  rows.push({ ...spec, inserted, error, afterCallback: clock, trace });
}
process.stdout.write(JSON.stringify({ node: process.versions.node, architecture: process.arch,
  sourceSha256: crypto.createHash('sha256').update(body).digest('hex'), rows }));
