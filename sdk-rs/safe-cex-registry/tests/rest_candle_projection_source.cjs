'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges');
const nativeKeys = JSON.parse(process.argv[2]);
const families = {
  apexomni: ['apexomni.exchange.ts'], asterdex: ['asterdex.exchange.ts'],
  backpack: ['backpack.exchange.ts'],
  binance: ['binance.exchange.ts', 'binanceinverse.exchange.ts', 'binancepm.exchange.ts', 'binancespot.exchange.ts'],
  bingx: ['bingx.exchange.ts'], bitget: ['bitget.exchange.ts'],
  bitunix: ['bitunix.exchange.ts'], blofin: ['blofin.exchange.ts'],
  breakoutprop: ['breakoutprop.exchange.ts'],
  bybit: ['bybit.exchange.ts', 'bybitinverse.exchange.ts', 'bybitspot.exchange.ts'],
  coinbase: ['coinbase.exchange.ts'], cryptocom: ['cryptocom.exchange.ts'],
  grvt: ['grvt.exchange.ts'], hyperliquid: ['hyperliquid.exchange.ts', 'hyperliquidspot.exchange.ts'],
  kraken: ['kraken.exchange.ts', 'krakenspot.exchange.ts'],
  lighter: ['lighter.exchange.ts'], mexc: ['mexc.utils.ts'], okx: ['okx.exchange.ts'],
  orderly: ['orderly.exchange.ts'], paradex: ['paradex.exchange.ts'],
  phemex: ['phemex.exchange.ts'], polymarket: ['polymarket.exchange.ts'],
  weex: ['weex.utils.ts'], woo: ['woo.exchange.ts'],
};
const registry = fs.readFileSync(path.join(root, 'registry.ts'), 'utf8');
const registeredFamilies = [...registry.matchAll(/import\(\s*(?:\/\*[\s\S]*?\*\/\s*)?['"]\.\/([^/]+)\//g)].map((m) => m[1]);
assert.equal(registeredFamilies.length, 49, 're-audit newly registered native families');
assert.deepEqual([...new Set(registeredFamilies)].sort(), [...Object.keys(families), 'kucoin', 'bitrue', 'txflow'].sort(), 'every registered family is overridden or has a default proof');
const records = [];
for (const [family, files] of Object.entries(families)) {
  const constructors = [];
  for (const file of files) {
    const filename = path.join(root, family, file);
    const tree = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
    function visit(node) {
      if (ts.isObjectLiteralExpression(node)) {
        const names = node.properties.map((p) => p.name?.getText(tree));
        if (['timestamp', 'open', 'close'].every((key) => names.includes(key))) {
          assert(node.properties.every((p) => !ts.isSpreadAssignment(p) && p.name && !ts.isComputedPropertyName(p.name)), `${family}/${file} static candle constructor`);
          assert.deepEqual(names, nativeKeys, `REST_CANDLE_DEFAULT_ORDER ${family}/${file}:${tree.getLineAndCharacterOfPosition(node.getStart()).line + 1}`);
          constructors.push({ file, line: tree.getLineAndCharacterOfPosition(node.getStart()).line + 1, keys: names });
        }
      }
      ts.forEachChild(node, visit);
    }
    visit(tree);
  }
  assert(constructors.length > 0, `${family}: actual candle constructors discovered`);
  records.push({ family, evidence: 'static literal without spreads/computed keys equals executed native serializer', constructors });
}
assert.equal(records.length, 24, 'all default non-scaffold families; KuCoin and Bitrue use their Js receiver');
const tree = ts.createSourceFile('base.ts', fs.readFileSync(path.join(root, 'base.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
const cls = tree.statements.find((n) => ts.isClassDeclaration(n) && n.name?.text === 'BaseExchange');
const method = cls.members.find((n) => n.name?.getText(tree) === 'fetchOHLCV');
const vm = require('node:vm');
const code = ts.transpileModule(`class Base { ${method.getText(tree)} } globalThis.base = new Base();`, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
const context = {};
vm.runInNewContext(code, context);
(async () => {
  assert.equal((await context.base.fetchOHLCV({})).length, 0, 'TxFlow inherited empty candle fallback has no model keys');
  records.push({ family: 'txflow', evidence: 'executed maintained inherited empty fallback', constructors: [] });
  process.stdout.write(JSON.stringify(records));
})().catch((error) => { console.error(error); process.exitCode = 1; });
