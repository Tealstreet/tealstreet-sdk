// Supplement the original oracle without modifying its bytes or starting another Node.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
const filename = path.join(__dirname, 'reads-oracle.cjs');
const sandbox = {require: createRequire(filename), __dirname, Buffer, console};
sandbox.process = new Proxy(process, {get(target, key) {
 if (key !== 'stdout') return Reflect.get(target, key);
 return {write(text) {
  (async () => {
   const original = JSON.parse(text);
   original.tokenSupplement = await sandbox.tokenSupplement();
   Object.assign(original, await sandbox.leverageSupplement());
   Object.assign(original, await sandbox.historySupplement());
   Object.assign(original, await sandbox.ohlcvSupplement());
   Object.assign(original, await sandbox.tickerSupplement());
   Object.assign(original, await sandbox.futuresCandlesSupplement());
   process.stdout.write(JSON.stringify(original));
  })().catch(error => {console.error(error); process.exitCode = 1;});
  return true;
 }};
}});
vm.runInNewContext(fs.readFileSync(filename, 'utf8') + `
globalThis.tokenSupplement = async () => {
 const rows = [];
 for (const raw of [
  '{"result":{"token":[17.25,"second",[-0,29.5],{"id":"fourth"}]}}',
  '{"result":{"token":["first","second"]}}'
 ]) rows.push(await token(raw));
 return rows;
};` + fs.readFileSync(path.join(__dirname, 'reads-leverage-supplement.cjs'), 'utf8') + fs.readFileSync(path.join(__dirname, 'reads-history-supplement.cjs'), 'utf8') + fs.readFileSync(path.join(__dirname, 'reads-ohlcv-supplement.cjs'), 'utf8') + fs.readFileSync(path.join(__dirname, 'reads-tickers-supplement.cjs'), 'utf8') + fs.readFileSync(path.join(__dirname, 'reads-futures-candles-supplement.cjs'), 'utf8'), sandbox, {filename});
