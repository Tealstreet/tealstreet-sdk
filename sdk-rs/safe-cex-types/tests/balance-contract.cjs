const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const generated = path.join(root, 'packages/safe-cex/src/generated');

test('Balance generation preserves the exact public fields, optionality and mutability', () => {
  for (const name of ['Balance', 'CoinBalance']) {
    assert.match(fs.readFileSync(path.join(generated, `${name}.ts`), 'utf8'), /generated/);
  }
  const file = path.join(generated, '__balance_contract__.ts');
  const source = `
import type { Balance } from './Balance';
import type { CoinBalance } from './CoinBalance';
import { emptyBalance } from './Balance';
import type { Balance as PublicBalance, CoinBalance as PublicCoin } from '../types';
type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;
type Check<T extends true> = T;
type SameBalance = Check<Equal<Balance, PublicBalance>>;
type SameCoin = Check<Equal<CoinBalance, PublicCoin>>;
type Keys = Check<Equal<keyof Balance, 'used' | 'free' | 'total' | 'upnl' | 'info' | 'currencies' | 'coins'>>;
declare const publicBalance: PublicBalance;
const native: Balance = publicBalance;
const publicAgain: PublicBalance = native;
const empty: Balance = emptyBalance;
const zero: Balance = { used: 0, free: 0, total: 0, upnl: 0 };
const coin: CoinBalance = { base: 'BTC', amount: 0, notional: 0 };
coin.base = 'ETH'; coin.amount = 2; coin.notional = 7;
const nested: Balance = { ...zero, currencies: { USD: zero }, coins: { BTC: coin } };
nested.coins!.ETH = coin;
nested.currencies!.EUR = zero;
// @ts-expect-error Balance aggregates are readonly.
zero.free = 1;
// @ts-expect-error Optional dictionaries are readonly properties.
nested.coins = {};
// @ts-expect-error An aggregate is required, including when its value is zero.
const missing: Balance = { used: 0, total: 0, upnl: 0 };
// @ts-expect-error Aggregate null is not a numeric account balance.
const nullAggregate: Balance = { ...zero, free: null };
// @ts-expect-error String amounts are not a numeric balance.
coin.amount = '1';
// @ts-expect-error A null dictionary is distinct from an omitted dictionary.
const nullDictionary: Balance = { ...zero, coins: null };
`;
  const configFile = path.join(root, 'packages/safe-cex/tsconfig.json');
  const config = ts.readConfigFile(configFile, ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configFile));
  const host = ts.createCompilerHost(parsed.options);
  const original = host.getSourceFile.bind(host);
  host.getSourceFile = (name, version, ...args) => name === file
    ? ts.createSourceFile(name, source, version, true) : original(name, version, ...args);
  const program = ts.createProgram([file], { ...parsed.options, incremental: false }, host);
  assert.deepEqual(ts.getPreEmitDiagnostics(program).map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')), []);
});

test('only the owned Balance outputs are fresh from the Rust slice module', () => {
  require('../generate-balance.cjs').generate(true);
});
