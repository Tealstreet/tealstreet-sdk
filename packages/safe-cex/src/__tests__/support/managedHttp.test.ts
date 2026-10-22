import type { HttpPacket, Transport } from '@tealstreet/safe-cex-wasm/host';

import { expect, it, vi } from 'vitest';

import { prepareRecordedHttp } from './managedHttp';

const fixture = () => {
  const transport: Transport = {
    now: () => 100,
    connect: () => {
      throw new Error('No fixture sockets');
    },
    request: vi.fn(async () => ({ status: 200, body: new Uint8Array() })),
    prepareHttp: prepareRecordedHttp,
    setTimeout: () => undefined,
    clearTimeout: () => {},
  };
  const packet: HttpPacket & { intent: 'primary' } = {
    request: 1,
    method: 'POST',
    url: 'https://venue.invalid/order',
    headers: {},
    intent: 'primary',
  };
  return { transport, packet };
};

it('uses the current overriding request callback only once at dispatch with the bound snapshot', async () => {
  const { transport, packet } = fixture();
  const request = vi.fn(transport.request);
  const wrapper = { ...transport, request };
  const prepared = await wrapper.prepareHttp!(packet);
  const final = { ...packet, url: `${packet.url}?signature=final`, headers: { signature: 'final' } };
  const bound = prepared.bind(final);
  final.headers.signature = 'changed';
  expect(request).not.toHaveBeenCalled();
  expect(bound.ready()).toBe(true);
  await bound.dispatch();
  expect(request).toHaveBeenCalledExactlyOnceWith({ ...final, headers: { signature: 'final' } });
  expect(bound.ready()).toBe(false);
  expect(() => bound.dispatch()).toThrow('unavailable');
  expect(() => prepared.bind(final)).toThrow('mismatch');
});

it('refuses mismatched packets and cancellation without sending', async () => {
  const { transport, packet } = fixture();
  const prepared = await transport.prepareHttp!(packet);
  expect(() => prepared.bind({ ...packet, request: 2 })).toThrow('mismatch');
  const bound = prepared.bind(packet);
  prepared.cancel!();
  expect(bound.ready()).toBe(false);
  expect(() => bound.dispatch()).toThrow('unavailable');
  expect(transport.request).not.toHaveBeenCalled();
});

it('checks the original raw expiry and abort signal at physical release', async () => {
  const { transport, packet } = fixture();
  const controller = new AbortController();
  const prepared = await transport.prepareHttp!({ ...packet, timeoutMs: 10 });
  const bound = prepared.bind({ ...packet, signal: controller.signal });
  transport.now = () => 110;
  expect(bound.ready()).toBe(false);
  expect(() => bound.dispatch()).toThrow('unavailable');
  transport.now = () => 100;
  controller.abort();
  expect(bound.ready()).toBe(false);
  expect(() => bound.dispatch()).toThrow('unavailable');
  expect(transport.request).not.toHaveBeenCalled();
});
