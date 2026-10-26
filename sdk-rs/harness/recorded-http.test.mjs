import { test } from 'node:test';
import assert from 'node:assert/strict';
import { prepareRecordedHttp } from './recorded-http.mjs';

test('recorded preparation hands the final packet to the original streamed transport only at release', async () => {
  const events = [];
  const controller = new AbortController();
  const response = { status: 200, headers: 'x-native: retained', body: (async function* () {
    events.push('chunk');
    controller.signal.throwIfAborted();
    yield new Uint8Array([1, 2]);
    controller.signal.throwIfAborted();
    yield new Uint8Array([3]);
  })() };
  const transport = {
    now: () => 100,
    prepareHttp: prepareRecordedHttp,
    request(packet) {
      assert.equal(this, transport);
      events.push('send');
      assert.equal(packet.url, 'https://recorded.invalid/write?signature=final');
      assert.equal(packet.headers.signature, 'final');
      assert.equal(packet.body, 'native-body');
      return Promise.resolve(response);
    },
  };
  const request = { request: 1, method: 'POST', url: 'https://recorded.invalid/write', signal: controller.signal };
  const prepared = transport.prepareHttp(request);
  const packet = { ...request, url: `${request.url}?signature=final`, headers: { signature: 'final' }, body: 'native-body' };
  const bound = prepared.bind(packet);
  packet.headers.signature = 'mutated';
  assert.deepEqual(events, []);
  assert.equal(bound.ready(), true);
  const actual = await bound.dispatch();
  assert.equal(actual, response);
  assert.deepEqual(events, ['send']);
  assert.equal(bound.ready(), false);
  assert.throws(() => bound.dispatch(), /unavailable/);
  const chunks = actual.body[Symbol.asyncIterator]();
  assert.deepEqual((await chunks.next()).value, new Uint8Array([1, 2]));
  controller.abort();
  await assert.rejects(chunks.next(), { name: 'AbortError' });
  assert.deepEqual(events, ['send', 'chunk']);
});
