import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

async function sign(expected, actual) {
  const events = [];
  const engine = {
    useExternalSigner() {},
    signed(...args) { events.push(['signed', ...args]); return ''; },
    signFailed(...args) { events.push(['failed', ...args]); return ''; },
  };
  const host = new EngineHost(engine, {
    now: () => 1000, setTimeout: () => 1, clearTimeout() {},
  }, {}, { signer: () => ({ values: [[1]], receipt: actual }) });
  host.execute({ op: 'sign', attempt: 7, timeoutMs: 1000, work: {
    receipt: expected, operations: [{ kind: 'present', key: 'account/primary' }], context: {},
  } });
  for (let i = 0; i < 8; i++) await Promise.resolve();
  host.dispose();
  return events;
}

test('exact decimal u64 nonce crosses the actual provider completion boundary', async () => {
  for (const nonce of ['0', '1', '9007199254740993', '18446744073709551615']) {
    const receipt = { stampMs: 1000, nonce };
    const events = await sign(receipt, receipt);
    assert.equal(events.length, 1);
    assert.equal(events[0][0], 'signed');
    assert.deepEqual(JSON.parse(events[0][2]), { values: [[1]], receipt });
  }
});
test('a required nonce cannot be omitted or substituted by the provider', async () => {
  for (const actual of [{}, { nonce: '2' }, { nonce: 1 }]) {
    assert.deepEqual(await sign({ nonce: '1' }, actual), [['failed', 7, 'invalidResult', 1000]]);
  }
});
test('nonce-free work does not accept an invented provider nonce', async () => {
  assert.deepEqual(await sign({}, { nonce: '1' }), [['failed', 7, 'invalidResult', 1000]]);
  const events = await sign({ stampMs: 1000 }, { stampMs: 1000 });
  assert.equal(events[0][0], 'signed');
});
test('strict nonce grammar rejects rounded or noncanonical provider values', async () => {
  for (const nonce of ['', '00', '01', '+1', '-1', ' 1', '1 ', '1.0', '1e3',
    '١', '1\n', '1\r', '1\u2028', '1\u2029', '18446744073709551616', 1, null, true, [], {}]) {
    assert.deepEqual(await sign({ nonce: '1' }, { nonce }), [['failed', 7, 'invalidResult', 1000]]);
  }
});
