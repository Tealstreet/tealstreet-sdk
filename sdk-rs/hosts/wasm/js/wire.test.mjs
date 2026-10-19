import assert from 'node:assert/strict';
import { test } from 'node:test';

import { Frame, NONE, SECTION } from './wire.mjs';

const stringFrame = (strings) => {
  const encoder = new TextEncoder();
  const encoded = strings.map((text) => encoder.encode(text));
  const payloadBytes = 4 + (strings.length + 1) * 4 + encoded.reduce((n, bytes) => n + bytes.length, 0);
  const buffer = new ArrayBuffer(24 + payloadBytes);
  const header = new DataView(buffer);
  header.setUint32(0, 0x46584353, true);
  header.setUint16(4, 1, true);
  header.setUint16(6, 1, true);
  header.setUint16(16, SECTION.STRINGS, true);
  header.setUint32(20, payloadBytes, true);
  header.setUint32(24, strings.length, true);
  const offsets = new Uint32Array(buffer, 28, strings.length + 1);
  const bytes = new Uint8Array(buffer, 28 + offsets.byteLength);
  let at = 0;
  for (let i = 0; i < encoded.length; i++) {
    offsets[i] = at;
    bytes.set(encoded[i], at);
    at += encoded[i].length;
  }
  offsets[strings.length] = at;
  return new Frame(buffer);
};

test('unused strings do not reserve decode-cache slots', () => {
  const frame = stringFrame(Array.from({ length: 4096 }, (_, i) => `market-${i}`));
  assert.equal(frame.strings.cache.length, 0);
  assert.equal(frame.string(NONE), undefined);
  assert.equal(frame.strings.cache.length, 0);
  assert.equal(frame.string(0), 'market-0');
  assert.equal(frame.strings.cache.length, 1);
  assert.equal(frame.string(4095), 'market-4095');
});

test('lazy string decoding still caches empty and multibyte values', () => {
  const frame = stringFrame(['', 'é', 'SOL_USDC_PERP']);
  assert.equal(frame.string(0), '');
  assert.equal(frame.string(1), 'é');
  frame.strings.bytes.fill(0);
  assert.equal(frame.string(0), '');
  assert.equal(frame.string(1), 'é');
});
