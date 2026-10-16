const fs = require('node:fs');
const path = require('node:path');
const {load, root} = require('./tables.cjs');
const source = load(path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoin.uta-account-mode.ts'));
const cases = JSON.parse(fs.readFileSync(0,'utf8'));
process.stdout.write(JSON.stringify({
 constants: Object.fromEntries(Object.entries(source).filter(([key])=>key.startsWith('KUCOIN_'))),
 cases: cases.map(value=>{
 const detection = source.decodeKucoinAccountMode(value);
 return {detection, decision: source.decideKucoinClassicRuntime(detection)};
 }),
}));
