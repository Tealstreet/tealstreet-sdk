import assert from 'node:assert/strict';
import {loadSource} from './source_loader.mjs';
import fs from 'node:fs';
const {virtualClock} = loadSource(new URL('../../../packages/safe-cex/src/utils/virtualClock.ts', import.meta.url));
const check = process.argv.includes('--check');
const nativeProcess = globalThis.process;
const NativeDate = Date;
const nativeSetTimeout = setTimeout;
const nativeClearTimeout = clearTimeout;
const nativeWindow = globalThis.window;
let now = 10000;
class FixedDate extends NativeDate {
  constructor(...args) { super(...(args.length ? args : [now])); }
  static now() { return now; }
}
const timers = [];
globalThis.Date = FixedDate;
globalThis.setTimeout = callback => { timers.push(callback); return timers.length; };
globalThis.clearTimeout = () => {};
const cases = [];
try {
  for (const runtime of ['native','browser','worker']) {
    for (const delta of [-1000,-999,0,999,1000,10000]) {
      globalThis.process = runtime === 'native' ? nativeProcess : undefined;
      globalThis.window = runtime === 'browser' ? {} : undefined;
      virtualClock.stop();
      virtualClock.syncFromServerTime(now + delta);
      const pushed = virtualClock.getCurrentTimeMs();
      virtualClock.setServerTimeSource(async () => 30000);
      await virtualClock.syncClientServerTime();
      const synced = virtualClock.getCurrentTimeMs();
      await virtualClock.syncBybitClientServerTime(20000);
      const bybit = virtualClock.getCurrentTimeMs();
      cases.push({runtime,delta,pushed,synced,bybit});
    }
  }
  globalThis.process = undefined;
  globalThis.window = undefined;
  virtualClock.stop(); timers.length = 0;
  virtualClock.syncFromServerTime(now + 10000);
  await virtualClock.start();
  now += 300000;
  await timers.shift()();
  cases.push({runtime:'worker-timer',elapsed:300000,after:virtualClock.getCurrentTimeMs()});
  virtualClock.stop();
} finally {
  globalThis.process = nativeProcess;
  globalThis.window = nativeWindow;
  globalThis.Date = NativeDate;
  globalThis.setTimeout = nativeSetTimeout;
  globalThis.clearTimeout = nativeClearTimeout;
}
// Corpus pins prevent empty or truncated snapshots from turning this gate green.
assert.equal(cases.length, 19);
const path = new URL('./clock_oracle.json', import.meta.url);
const output = JSON.stringify(cases,null,2)+'\n';
if (check) {
  if (fs.readFileSync(path,'utf8') !== output) throw new Error('clock oracle fixtures changed');
  console.log(`${cases.length} clock source-oracle cases verified`);
} else fs.writeFileSync(path,output);
