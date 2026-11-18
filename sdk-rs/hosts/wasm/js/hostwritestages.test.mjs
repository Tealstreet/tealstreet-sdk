import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

const work = { receipt: { stampMs: 1000, expiryMs: 2000 }, context: { venue: 'reference', method: 'POST', host: 'fixture.invalid', path: '/order' }, operations: [{ kind: 'present', key: 'account/primary' }] };
const http = (request) => ({ op: 'http', request, method: 'POST', url: 'https://fixture.invalid/order', headers: [], body: '' });
const tick = async () => { for (let n = 0; n < 16; n++) await Promise.resolve(); };

// The fake engine answers a command with `onCommand`, a signature with an HTTP
// request, and a response end with the command's completion.
function rig({ onCommand, signer, transport = {} }) {
  const engine = {
    useExternalSigner() {},
    command: (json) => JSON.stringify(onCommand(JSON.parse(json).command)),
    signed: () => JSON.stringify([http(9)]),
    httpStart: () => '',
    httpChunk: () => '',
    httpEnd: (id) => JSON.stringify([{ op: 'done', command: id === 9 || id === 5 ? 1 : 2, result: '[]' }]),
    httpFailed: () => '',
  };
  const host = new EngineHost(engine, {
    now: () => 0,
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (id) => clearTimeout(id),
    request: async () => ({ status: 200, headers: '', body: new Uint8Array() }),
    ...transport,
  }, {}, signer ? { signer } : {});
  return host;
}

test('a locally signed write stamps accepted, sent, response and reply in order', async () => {
  const host = rig({ onCommand: (command) => [http(5)] });
  const stages = [];
  await host.command({ op: 'placeOrders', orders: [] }, { onStage: (stage) => stages.push(stage) });
  assert.deepEqual(stages, ['accepted', 'sent', 'response', 'reply']);
  assert.equal(host.stageCauses.size, 0);
  host.dispose();
});

test('a provider signature stamps signed and attributes the request it releases', async () => {
  const host = rig({
    onCommand: () => [{ op: 'sign', attempt: 3, work, timeoutMs: 1000 }],
    signer: async (request) => ({ values: [[1]], receipt: request.receipt }),
  });
  const stages = [];
  await host.command({ op: 'cancelOrders', ids: [] }, { onStage: (stage) => stages.push(stage) });
  assert.deepEqual(stages, ['accepted', 'signed', 'sent', 'response', 'reply']);
  assert.equal(host.stageCauses.size, 0);
  host.dispose();
});

test('requests another command started are not attributed, and observers cannot fail a write', async () => {
  let next = 5;
  const host = rig({ onCommand: () => [http(next++)] });
  const stages = [];
  const first = host.command({ op: 'placeOrders', orders: [] }, { onStage: (stage) => stages.push(stage) });
  const second = host.command({ op: 'cancelOrders', ids: [] }, { onStage: () => { throw new Error('observer'); } });
  assert.deepEqual(await Promise.all([first, second]), [[], []]);
  assert.deepEqual(stages, ['accepted', 'sent', 'response', 'reply']);
  await assert.rejects(host.command({ op: 'placeOrders' }, { onStage: 1 }), TypeError);
  host.dispose();
});

test('an untimed command records no attribution', async () => {
  const host = rig({ onCommand: () => [http(5)] });
  const pending = host.command({ op: 'placeOrders', orders: [] });
  assert.equal(host.stageCauses.size, 0);
  await pending;
  await tick();
  host.dispose();
});
