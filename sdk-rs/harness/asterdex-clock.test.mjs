import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildAsterTape } from './private-tape-asterdex.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const moduleUrl = (path) => new URL(path, import.meta.url).href;

// Isolate controlled signing time per child. Keep loader/deadline clocks real;
// no signer, sync method or request validator is replaced with a stub.
const runClockProfile = (profile, mutation = 'none', startupTimer = false) => {
  const dir = mkdtempSync(join(tmpdir(), 'asterdex-replay-clock-'));
  try {
    const tape = join(dir, 'clock.jsonl'),
      out = join(dir, 'output.json');
    const evidence = join(dir, 'time.json'),
      preload = join(dir, 'controlled-time.mjs');
    let keptOrder = false;
    const records = buildAsterTape().filter((record) => {
      if (record.type !== 'http') return record.t <= 600;
      if (record.method === 'GET' || new URL(record.url).pathname.endsWith('/listenKey')) return true;
      if (record.method === 'POST' && !keptOrder) {
        keptOrder = true;
        return true;
      }
      return false;
    });
    const epoch = records.find((record) => record.type === 'meta')?.epoch;
    assert.ok(Number.isSafeInteger(epoch));
    writeFileSync(tape, records.map((record) => JSON.stringify(record)).join('\n') + '\n');
    writeFileSync(
      preload,
      `
import assert from 'node:assert/strict';
import {registerHooks} from 'node:module';
import {writeFileSync} from 'node:fs';
import {asterDEXVirtualClock} from ${JSON.stringify(moduleUrl('../../packages/safe-cex/src/exchanges/asterdex/asterdex.virtualClock.ts'))};
import {VENUES} from ${JSON.stringify(moduleUrl('./venues.mjs'))};
import {Tape,ReplayClock,withTimeout} from ${JSON.stringify(moduleUrl('./tape.mjs'))};
const profile=${JSON.stringify(profile)}, mutation=${JSON.stringify(mutation)}, startupTimer=${JSON.stringify(startupTimer)};
const epoch=${epoch};
let elapsed=0, setup=0, startup=0, replayClock, baseline;
const timings={profile,mutation,requests:[]};
const save=()=>writeFileSync(${JSON.stringify(evidence)},JSON.stringify(timings));
// Only replay.mjs sees this performance clock; loader/deadlines keep real time.
globalThis.__asterClockTime={now:()=>elapsed};
assert.equal(VENUES.asterdex.virtualPrivateClock,true);
if(profile==='continuous')VENUES.asterdex.virtualPrivateClock=false;
timings.virtual=VENUES.asterdex.virtualPrivateClock;
const once=(source,from,to)=>{assert.equal(source.split(from).length-1,1,'mutation site');return source.replace(from,to);};
registerHooks({load(url,context,nextLoad){
 const result=nextLoad(url,context);
 if(url.endsWith('/asterdex/asterdex.exchange.ts')){assert.equal(setup++,0);elapsed+=21000;}
 if(url.endsWith('/harness/replay.mjs')){
  let source=typeof result.source==='string'?result.source:Buffer.from(result.source).toString();
  source='const performance = globalThis.__asterClockTime;\\n'+source;
  const marker='async function runTs() {';assert.equal(source.split(marker).length-1,1);
  const at=source.indexOf(marker),before=source.slice(0,at);source=source.slice(at);
  if(mutation==='no-reset')source=once(source,'  clockStart = performance.now();\\n','');
  else if(mutation==='rebase')source=once(source,'    clock,\\n    undefined,\\n    values.performance ? undefined : (point) => {','    clock,\\n    () => { clockStart = performance.now(); },\\n    values.performance ? undefined : (point) => {');
  return {...result,source:before+source};
 }
 return result;
}});
const install=ReplayClock.prototype.install;
ReplayClock.prototype.install=function(){
 assert.equal(replayClock,undefined);
 replayClock=this;return install.call(this);
};
const originalStart=asterDEXVirtualClock.start;
asterDEXVirtualClock.start=async(...args)=>{
 assert.equal(setup,1);
 assert.equal(startup++,0);
 // Install after module loading, before the real server/client clock sync.
 const OriginalDate=Date, now=Date.now;
 globalThis.Date=class extends OriginalDate {
  constructor(...args){super(...(args.length?args:[profile==='virtual'?Date.now():1800000000000+elapsed]));}
  static now=now;
 };
 assert.equal(new Date(42).valueOf(),42);
 assert.equal(Date.parse('1970-01-01T00:00:00.000Z'),0);
 assert.equal(Date.UTC(1970,0,1),0);
 await originalStart(...args);
 timings.before={oracle:Date.now(),signer:asterDEXVirtualClock.getCurrentTimeMs()};
 if(profile==='virtual'){assert.ok(replayClock);replayClock.time+=1500;}else elapsed+=1500;
 timings.after={oracle:Date.now(),signer:asterDEXVirtualClock.getCurrentTimeMs()};
 timings.setupCount=setup;timings.startupCount=startup;save();
 if(startupTimer){
  assert.equal(profile,'virtual');
  await new Promise(resolve=>setTimeout(resolve,1000));
  timings.startupTimerBoundary=Date.now();save();
 }
};
const play=Tape.prototype.play;
Tape.prototype.play=function(onTick,ready,runCommand,clock,begin,checkpoint){
 if(profile==='continuous'){
  assert.equal(clock,undefined);
  // Advance before each frame; pending commands retain a real timeout and rejection.
  clock={
   epoch,time:epoch,active:false,
   start(){baseline=elapsed;this.active=true;timings.baseline=baseline;},
   async advance(t){elapsed=baseline+t;this.time=this.epoch+t;},
   wait(p,label,ms){return withTimeout(p,label,ms);},
  };
 }else{
  assert.equal(clock,replayClock);
  const start=clock.start;
  clock.start=function(){timings.readyBefore=this.time;const r=start.call(this);timings.readyAfter=this.time;return r;};
 }
 return play.call(this,onTick,ready,runCommand,clock,begin,checkpoint);
};
const factory=VENUES.asterdex.createRequestValidator;
VENUES.asterdex.createRequestValidator=function(...args){
 const validate=factory(...args);
 return function(request){
 // Record real signed wire fields before request canonicalization.
 const u=new URL(request.url),stamp=u.searchParams.get('timestamp');
 if(stamp===null)return validate(request);
 const row={path:u.pathname,timestamp:Number(stamp),now:request.now??Date.now()};
 timings.requests.push(row);
 try { const result=validate(request); row.valid=true; return result; }
 catch(error) { row.error=error.message; throw error; }
 finally { save(); }
 };
};
`,
    );
    const child = spawnSync(
      process.execPath,
      [
        '--expose-gc',
        '--import',
        'tsx',
        '--import',
        preload,
        'sdk-rs/harness/replay.mjs',
        '--impl',
        'ts',
        '--venue',
        'asterdex',
        '--tape',
        tape,
        '--books',
        '',
        '--out',
        out,
      ],
      { cwd: root, encoding: 'utf8', timeout: 25000, env: { ...process.env, ESBUILD_WORKER_THREADS: '0' } },
    );
    assert.ifError(child.error);
    const timings = JSON.parse(readFileSync(evidence, 'utf8'));
    assert.equal(timings.setupCount, 1);
    assert.equal(timings.startupCount, 1);
    assert.equal(timings.virtual, profile === 'virtual');
    if (mutation !== 'none') {
      assert.notEqual(child.status, 0, 'clock regression unexpectedly passed');
      const rejected = timings.requests.find((request) => request.error === 'Aster V1 timestamp/window is invalid');
      assert.ok(rejected, child.stdout + '\n' + child.stderr);
      if (mutation === 'no-reset') assert.equal(rejected.now - rejected.timestamp, 21000);
      else assert.equal(rejected.timestamp - rejected.now, 1500);
      return;
    }
    assert.equal(child.status, 0, child.stdout + '\n' + child.stderr);
    assert.deepEqual(timings.before, { oracle: epoch, signer: epoch });
    assert.deepEqual(timings.after, { oracle: epoch + 1500, signer: epoch + 1500 });
    const orders = timings.requests.filter((request) => request.path === '/fapi/v1/order');
    assert.equal(orders.length, 1);
    const timestamp = epoch + 600 + (profile === 'continuous' ? 1500 : 0);
    assert.deepEqual(orders[0], { path: '/fapi/v1/order', timestamp, now: timestamp, valid: true });
    assert.ok(timings.requests.every((request) => request.valid && request.timestamp === request.now));
    if (profile === 'virtual') {
      if (startupTimer) {
        assert.ok(Number.isSafeInteger(timings.startupTimerBoundary));
        assert.ok(timings.startupTimerBoundary >= timings.after.oracle + 1000);
      }
      assert.ok(Number.isSafeInteger(timings.readyBefore));
      assert.ok(timings.readyBefore >= (startupTimer ? timings.startupTimerBoundary : timings.after.oracle));
      assert.equal(timings.readyAfter, epoch);
    } else assert.equal(timings.baseline, 22500);
    const replay = JSON.parse(readFileSync(out, 'utf8'));
    assert.equal(replay.invalidSignatures, 0);
    assert.equal(replay.unserved, 0);
    assert.equal(replay.unusedWrites, 0);
    assert.deepEqual(replay.commands, [{ ok: ['1002'] }]);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
};

test('Aster signing advances exactly through controlled module setup and slow startup', { timeout: 30000 }, () =>
  runClockProfile('continuous'),
);
test('Aster tape-clock signing follows frozen startup and readiness rewind', { timeout: 30000 }, () =>
  runClockProfile('virtual'),
);
test('Aster frozen startup permits a legitimate startup timer before readiness', { timeout: 30000 }, () =>
  runClockProfile('virtual', 'none', true),
);
for (const mutation of ['no-reset', 'rebase']) {
  test(`Aster clock regression ${mutation} is rejected by the real timestamp validator`, { timeout: 30000 }, () =>
    runClockProfile('continuous', mutation),
  );
}
