import { describe, expect, it } from 'vitest';

import { decompress } from '@tealstreet/compression/src/decompress';

import { decodeTradeTunnelMessage } from './tradeTunnelBinaryFrame';

const RAW_BODY = '{"orderId":9007199254740993}';
const GZIP_BODY_BASE64 = 'H4sIAAAAAAAEA6tWyi9KSS3yTFGysjQwMDe0tDQyNTE3MbC0NK4FAERmNdIcAAAA';

function base64Bytes(encoded: string): Uint8Array {
  return Uint8Array.from(globalThis.atob(encoded), (character) => character.charCodeAt(0));
}

function binaryFrame(): Uint8Array {
  const header = new TextEncoder().encode(
    JSON.stringify({
      type: 'rpc.http',
      nonce: 'binary-test',
      data: {
        data_encoding: 'gzip',
        status: 200,
        headers: { 'content-type': 'application/json' },
      },
    }),
  );
  const payload = base64Bytes(GZIP_BODY_BASE64);
  const frame = new Uint8Array(8 + header.length + payload.length);
  frame.set([0x54, 0x53, 0x42, 0x31], 0);
  new DataView(frame.buffer).setUint32(4, header.length, false);
  frame.set(header, 8);
  frame.set(payload, 8 + header.length);
  return frame;
}

describe('TradeTunnel binary response frames', () => {
  it('decodes an ArrayBuffer and exposes the compressed payload', async () => {
    const frame = binaryFrame();
    const message = await decodeTradeTunnelMessage(frame.buffer);

    expect(message).toMatchObject({
      type: 'rpc.http',
      nonce: 'binary-test',
      data: { data_encoding: 'gzip', status: 200 },
    });
    await expect(decompress(message.data.data_binary, 'gzip')).resolves.toBe(RAW_BODY);
  });

  it('decodes browser Blob messages', async () => {
    const frame = binaryFrame();
    const message = await decodeTradeTunnelMessage(new Blob([frame.buffer as ArrayBuffer]));
    await expect(decompress(message.data.data_binary, 'gzip')).resolves.toBe(RAW_BODY);
  });

  it('decodes ArrayBuffer views used by Node and Bun', async () => {
    const frame = binaryFrame();
    const padded = new Uint8Array(frame.length + 4);
    padded.set(frame, 2);
    const message = await decodeTradeTunnelMessage(padded.subarray(2, 2 + frame.length));
    await expect(decompress(message.data.data_binary, 'gzip')).resolves.toBe(RAW_BODY);
  });

  it('keeps legacy JSON text messages unchanged', async () => {
    await expect(
      decodeTradeTunnelMessage('{"type":"rpc.ping","status":"success"}'),
    ).resolves.toEqual({ type: 'rpc.ping', status: 'success' });
  });

  it('rejects invalid magic bytes', async () => {
    const frame = binaryFrame();
    frame[0] = 0;
    await expect(decodeTradeTunnelMessage(frame.buffer)).rejects.toThrow('invalid magic');
  });

  it('rejects invalid header lengths', async () => {
    const frame = binaryFrame();
    new DataView(frame.buffer).setUint32(4, 2 * 1024 * 1024, false);
    await expect(decodeTradeTunnelMessage(frame.buffer)).rejects.toThrow('invalid header length');
  });

  it('rejects frames without a binary payload', async () => {
    const frame = binaryFrame();
    const headerLength = new DataView(frame.buffer).getUint32(4, false);
    await expect(decodeTradeTunnelMessage(frame.slice(0, 8 + headerLength))).rejects.toThrow('missing its payload');
  });
});
