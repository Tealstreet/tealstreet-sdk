const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const test = require('node:test');
const { runInNewContext } = require('node:vm');
const { VALID_EXCHANGES } = require('../config/exchangeLists.js');

const spec = readFileSync(resolve(__dirname, '../e2e/spec.cy.ts'), 'utf8');
const definition = /^const getAccount = \(\) => \{[\s\S]*?^\};/m.exec(spec)?.[0];
assert.ok(definition, 'exercise the actual Cypress account getter');
function accountFor(account, config = {}) {
  const getter = runInNewContext(definition.replace('const getAccount = ', '').replace(/;$/, ''), {
    Cypress: { env: name => name === 'account' ? account : account?.exchange },
    getExchangeConfig: () => config,
  });
  return getter();
}
const workflow = readFileSync(resolve(__dirname, '../../../../.github/workflows/e2e.yml'), 'utf8');
function checkWorkflow(text) {
  assert.match(text, /^concurrency:\n  group: e2e-shared-test-accounts\n  cancel-in-progress: false\n/m,
    'all branches and targets share credentials; never cancel a running trading suite');
  assert.match(text, /- name: Verify account isolation\n        run: node --test apps\/web\/cypress\/scripts\/account-isolation\.test\.js\n/);
}

test('every configured parallel venue has a distinct stable hosted account name', () => {
  const names = VALID_EXCHANGES.map(exchange => accountFor({ exchange, name: 'shared fixture' }).name);
  assert.equal(new Set(names).size, VALID_EXCHANGES.length, 'different venues must not share a hosted account lease');
  for (const [index, exchange] of VALID_EXCHANGES.entries()) {
    assert.equal(names[index], `e2e-${exchange}`);
    assert.equal(accountFor({ exchange, name: 'another label' }).name, names[index]);
  }
});

test('account isolation preserves credentials and configuration without mutating the input', () => {
  const input = { exchange: 'bitget', name: 'shared fixture', user: 'fixture-user',
    key: 'fixture-key', secret: 'fixture-secret', passphrase: 'fixture-passphrase', extra: { keep: true, override: 1 } };
  const before = JSON.stringify(input);
  const account = accountFor(input, { accountExtra: { override: 2 } });
  assert.equal(account.user, undefined);
  for (const name of ['exchange', 'key', 'secret', 'passphrase']) assert.equal(account[name], input[name]);
  assert.deepEqual(JSON.parse(JSON.stringify(account.extra)), { keep: true, override: 2 });
  assert.equal(JSON.stringify(input), before);
  assert.throws(() => accountFor(undefined), /No account found/);
});

test('shared-credential workflow runs serialize across refs and cannot skip their offline guard', () => {
  checkWorkflow(workflow);
  for (const [before, after] of [
    ['group: e2e-shared-test-accounts', 'group: e2e-${{ github.ref }}'],
    ['cancel-in-progress: false', 'cancel-in-progress: true'],
    ['concurrency:\n  group: e2e-shared-test-accounts\n  cancel-in-progress: false\n', ''],
    ['      - name: Verify account isolation\n', '      - if: false\n        name: Verify account isolation\n'],
  ]) {
    const changed = workflow.replace(before, after);
    assert.notEqual(changed, workflow);
    assert.throws(() => checkWorkflow(changed));
  }
});
