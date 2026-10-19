import assert from 'node:assert/strict';
import { gzipSync, deflateSync } from 'node:zlib';
import { test } from 'node:test';
import { socketMessages } from './host.mjs';

test('text is synchronous and binary feeds retain message order', async () => {
  const got = [];
  const receive = socketMessages((text) => got.push(text), (error) => { throw error; });
  receive('Ping');
  assert.deepEqual(got, ['Ping']);
  receive(new Blob([gzipSync('{"n":1}')]));
  receive('second');
  receive(deflateSync('{"n":3}'));
  receive(new TextEncoder().encode('fourth').buffer);
  for (let i=0; i<50 && got.length<5; i++) await new Promise((r) => setTimeout(r, 5));
  assert.deepEqual(got, ['Ping', '{"n":1}', 'second', '{"n":3}', 'fourth']);
});

test('invalid compression reaches the transport error callback', async () => {
  const errors = [];
  const receive = socketMessages(() => assert.fail('unexpected frame'), (e) => errors.push(e));
  receive(new Uint8Array([0x1f, 0x8b, 0]));
  for (let i=0; i<50 && errors.length===0; i++) await new Promise((r) => setTimeout(r, 5));
  assert.equal(errors.length, 1);
});
