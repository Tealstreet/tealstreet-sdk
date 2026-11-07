const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { stripTypeScriptTypes } = require('node:module');
const { resolve } = require('node:path');
const test = require('node:test');
const { runInNewContext } = require('node:vm');
const { testAccountName } = require('../../apps/web/cypress/config/accountNames.js');
const { VALID_EXCHANGES } = require('../../apps/web/cypress/config/exchangeLists.js');

// Execute the spec's actual naming and storage code, with synthetic accounts only.
const source = stripTypeScriptTypes(readFileSync(resolve(__dirname, '../../apps/web/cypress/e2e/spec.cy.ts'), 'utf8'));
const badKeyTest = source.slice(source.indexOf("if (shouldRunTest('bad-api-key'))"), source.indexOf("if (shouldRunTest('add-api-key'))"));
const formTest = source.slice(source.indexOf("if (shouldRunTest('add-api-key'))"), source.indexOf("if (shouldRunTest('load-balance'))"));
const evaluate = (code, context = {}) => runInNewContext(code, { testAccountName, ...context });

function formNames(exchange) {
  const declarations = formTest.match(/const (?:good|bad)AccountName = .*;/g);
  assert.equal(declarations?.length, 2);
  return evaluate(`${declarations.join('\n')}; [goodAccountName, badAccountName]`, { exchange });
}

test('credential variants are stable and distinct across the configured matrix and primary accounts', () => {
  const names = VALID_EXCHANGES.flatMap(exchange => {
    const [good, bad] = formNames(exchange);
    assert.equal(good, `e2e-${exchange}-goodkey`);
    assert.equal(bad, `e2e-${exchange}-form-badkey`);
    for (const variant of ['badkey', 'goodkey', 'form-badkey']) {
      assert.equal(testAccountName(exchange, variant), testAccountName(exchange, variant));
    }
    return [`e2e-${exchange}`, testAccountName(exchange, 'badkey'), good, bad];
  });
  assert.equal(new Set(names).size, VALID_EXCHANGES.length * 4);
});

test('the corrupted API-key probe changes identity without changing routing or the source account', () => {
  const definition = badKeyTest.match(/const badAccount = \{[\s\S]*?^        \};/m)?.[0];
  assert.ok(definition, 'locate the actual bad-key account');
  for (const exchange of VALID_EXCHANGES) {
    const account = { exchange, name: `e2e-${exchange}`, apiKey: 'synthetic-key', apiSecret: 'synthetic-secret',
      passphrase: 'synthetic-passphrase', tealstreetProxy: true, extra: { keep: true } };
    const before = JSON.stringify(account);
    const result = evaluate(`${definition}; badAccount`, { account, corruptKey: value => `bad-${value}` });
    assert.deepEqual(JSON.parse(JSON.stringify(result)), { ...account, name: `e2e-${exchange}-badkey`,
      apiKey: 'bad-synthetic-key', apiSecret: 'bad-synthetic-secret' });
    assert.equal(JSON.stringify(account), before);
  }
  // Paradex validates its signing key through the form rather than localStorage.
  const paradexName = badKeyTest.match(/\.type\((testAccountName\(account.exchange, 'badkey'\))\)/)?.[1];
  assert.ok(paradexName);
  assert.equal(evaluate(paradexName, { account: { exchange: 'paradex' } }), 'e2e-paradex-badkey');
});

test('form lookup, injection, activation and cleanup use only the current venue variants', () => {
  const callbacks = [...formTest.matchAll(/cy\.window\([^\n]*\)\.(?:then|should)\(\(win\) => \{([\s\S]*?)^        \}\);/gm)]
    .map(match => match[1]);
  const lookup = callbacks.filter(code => code.includes('const goodAccount ='));
  const injection = callbacks.find(code => code.includes('accounts.push('));
  const activation = callbacks.find(code => code.includes('activeAccount: badAccountName'));
  const cleanup = callbacks.find(code => code.includes('const cleaned ='));
  assert.equal(lookup.length, 2);
  assert.ok(injection && activation && cleanup);
  const inputs = [...formTest.matchAll(/getAddAccountInput\('name', [^\n]+\.type\(([^)]+)\)/g)].map(match => match[1]);
  assert.equal(inputs.length, 2);

  for (const exchange of ['bybit', 'okx', 'paradex']) {
    const account = { exchange, name: `e2e-${exchange}`, apiKey: 'synthetic-key', apiSecret: 'synthetic-secret' };
    const [goodAccountName, badAccountName] = formNames(exchange);
    const retained = [account, { name: 'e2e-other-goodkey', exchange: 'other' },
      { name: 'e2e-other-form-badkey', exchange: 'other' }, { name: 'e2e-good-key', exchange: 'other' },
      { name: 'e2e-bad-key', exchange: 'other' }, { name: `e2e-${exchange}-badkey`, exchange }];
    const storage = new Map([['accounts', JSON.stringify([...retained, { name: goodAccountName, exchange }])],
      ['lastSettings', JSON.stringify({ trade: { activeSymbol: 'synthetic-symbol' }, keep: true })]]);
    const context = { account, goodAccountName, badAccountName, exchange, metaData: {},
      corruptKey: value => `bad-${value}`, cy: { task() {} },
      win: { localStorage: { getItem: key => storage.get(key), setItem: (key, value) => storage.set(key, value) } },
      expect: value => ({ to: { not: { be: { get undefined() { assert.notEqual(value, undefined); } } },
        eq: expected => assert.equal(value, expected) } }) };
    assert.deepEqual(inputs.map(code => evaluate(code, context)), [goodAccountName, badAccountName]);
    for (const code of lookup) evaluate(code, context);
    evaluate(injection, context);
    evaluate(injection, context); // Repeated injection must reuse this variant.
    const saved = JSON.parse(storage.get('accounts'));
    assert.equal(saved.filter(item => item.name === badAccountName).length, 1);
    assert.equal(saved.find(item => item.name === badAccountName).apiKey, 'bad-synthetic-key');
    evaluate(activation, context);
    assert.equal(JSON.parse(storage.get('lastSettings')).trade.activeAccount, badAccountName);
    evaluate(cleanup, context);
    assert.deepEqual(JSON.parse(storage.get('accounts')), retained);
    assert.deepEqual(JSON.parse(storage.get('lastSettings')), {
      trade: { activeAccount: account.name, activeSymbol: 'synthetic-symbol' }, keep: true,
    });
  }
});
