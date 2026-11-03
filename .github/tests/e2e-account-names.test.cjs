const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const { testAccountName } = require('../../apps/web/cypress/config/accountNames.js');
const { VALID_EXCHANGES } = require('../../apps/web/cypress/config/exchangeLists.js');

const source = readFileSync(resolve(__dirname, '../../apps/web/cypress/e2e/spec.cy.ts'), 'utf8');
const accountFactory = source.slice(source.indexOf('const getAccount = () => {'), source.indexOf('const getAccMetadata ='));

function accountFor(exchange) {
  const account = {
    name: 'shared-secret-name', exchange, user: 'unused-user', apiKey: 'placeholder-key',
    apiSecret: 'placeholder-secret', passphrase: 'placeholder-passphrase',
    tealstreetProxy: true, tealstreetTunnel: false, extra: { retained: true },
  };
  const config = { accountExtra: { override: true } };
  const getAccount = vm.runInNewContext(`${accountFactory}; getAccount`, {
    Cypress: { env: name => name === 'account' ? account : undefined },
    getExchangeConfig: () => config,
    testAccountName,
  });
  return { account, result: JSON.parse(JSON.stringify(getAccount())), retry: getAccount() };
}

test('parallel matrix accounts have distinct hosted names and stable retry identities', () => {
  const hostedKeys = VALID_EXCHANGES.map(exchange => {
    const { result, retry } = accountFor(exchange);
    assert.equal(result.name, `test-${exchange}`);
    assert.equal(retry.name, result.name);
    return JSON.stringify(['same-authenticated-user', result.name]);
  });
  assert.equal(new Set(hostedKeys).size, VALID_EXCHANGES.length);
});

test('the actual account factory preserves credentials, routing and extra settings', () => {
  for (const exchange of ['bybit', 'okx']) {
    const { account, result } = accountFor(exchange);
    const { user, ...expected } = account;
    assert.deepEqual(result, {
      ...expected, name: `test-${exchange}`, extra: { retained: true, override: true },
    });
    assert.equal(account.name, 'shared-secret-name');
    assert.deepEqual(account.extra, { retained: true });
  }
});

test('credential variants never alias another venue or the normal account', () => {
  const variants = ['primary', 'badkey', 'goodkey', 'form-badkey'];
  const names = VALID_EXCHANGES.flatMap(exchange => variants.map(variant => {
    const name = testAccountName(exchange, variant);
    assert.equal(testAccountName(exchange, variant), name);
    return name;
  }));
  assert.equal(new Set(names).size, VALID_EXCHANGES.length * variants.length);
  assert.equal(testAccountName('okx', 'badkey'), 'test-okx-badkey');
});
