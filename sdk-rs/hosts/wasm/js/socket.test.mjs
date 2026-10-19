import assert from 'node:assert/strict';
import { gzipSync, deflateSync } from 'node:zlib';
import { test } from 'node:test';
import { EngineHost, socketMessages } from './host.mjs';

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

for (const [format, compress, field] of [['gzip', gzipSync, 'id'], ['deflate', deflateSync, 'symbol']]) {
  test(`${format} rejects malformed UTF-8 in JSON ${field} instead of delivering a changed value`, { timeout: 1000 }, async () => {
    const received = [];
    let failures = 0;
    let failed;
    const rejected = new Promise((resolve) => { failed = resolve; });
    const receive = socketMessages((text) => received.push(text), (error) => {
      failures++;
      failed(error);
    });
    const malformed = Buffer.concat([Buffer.from(`{"${field}":"`), Buffer.from([0xff]), Buffer.from('"}')]);
    receive(compress(malformed));
    const error = await rejected;
    assert.ok(error instanceof TypeError);
    assert.equal(failures, 1);
    assert.deepEqual(received, []);
  });
}

test('whole and streamed HTTP buffers cross in bounded chunks', async () => {
  for (const streamed of [false, true]) {
    const sizes=[];
    const engine={httpStart:()=>'',httpChunk:(_id,data)=>{sizes.push(data.length);return ''},httpEnd:()=>''};
    const host=new EngineHost(engine,{now:()=>0});
    const bytes=new Uint8Array(100_000);
    const chunks=async function*(){yield bytes};
    await host.receive(1,{status:200,body:streamed?chunks():bytes});
    assert.equal(sizes.reduce((a,b)=>a+b,0),100_000);
    assert.ok(Math.max(...sizes)<=16*1024);
  }
});
