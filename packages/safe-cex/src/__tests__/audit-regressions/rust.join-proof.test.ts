import type { EngineHost, HostedJoinProofOutput } from '@tealstreet/safe-cex-wasm/host';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const raw = '{"id":"attempt","joinProof":{"version":1,"version":1}}';
const signed: HostedJoinProofOutput = { id: 'attempt', joinProof: { version: 1, signature: 'public-signature' } };
const live: RustEngineExchange[] = [];
const deferred = <T>() => {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
};
const tick = async () => { for (let n = 0; n < 10; n++) await Promise.resolve(); };

function fixture(ready: Promise<void> = Promise.resolve(), answer: Promise<HostedJoinProofOutput> = Promise.resolve(signed)) {
  const exchange = new RustEngineExchange('backpack', { key: 'synthetic-key', secret: Buffer.alloc(32, 1).toString('base64') }, new DefaultStore());
  live.push(exchange);
  const hostedJoinProof = vi.fn(() => answer);
  const host = { hostedJoinProof } as unknown as EngineHost;
  const internal = exchange as unknown as { host: EngineHost; hostInitialization: Promise<void>; ensureSigningHost(): Promise<void> };
  internal.host = host;
  internal.hostInitialization = ready;
  vi.spyOn(internal, 'ensureSigningHost').mockImplementation(() => ready);
  return { exchange, internal, host, hostedJoinProof };
}

afterEach(() => {
  for (const exchange of live) {
    // The fake owns no engine allocation; do not run production engine teardown on it.
    (exchange as unknown as { host?: EngineHost }).host = undefined;
    exchange.dispose();
  }
  live.length = 0;
  vi.restoreAllMocks();
});

describe('SDK join-proof source lifetime', () => {
  it('keeps raw request bytes and returns the captured proof without requiring an owner hook', async () => {
    const { exchange, hostedJoinProof } = fixture();
    await expect(exchange.signJoinProof(raw)).resolves.toEqual(signed);
    expect(hostedJoinProof).toHaveBeenCalledExactlyOnceWith(raw);
  });

  it.each(['B', 'ABA'])('retires an original owner while initialization is deferred (%s)', async transition => {
    const ready = deferred<void>();
    const { exchange, hostedJoinProof } = fixture(ready.promise);
    let owner = { ownerId: 'A', generation: 1 };
    exchange.setWorkerOwner(() => owner);
    const pending = exchange.signJoinProof(raw);
    owner = { ownerId: 'B', generation: 2 };
    if (transition === 'ABA') owner = { ownerId: 'A', generation: 3 };
    ready.resolve();
    await expect(pending).rejects.toMatchObject({ code: 'WORKER_STOPPED' });
    expect(hostedJoinProof).not.toHaveBeenCalled();
  });

  it.each(['B', 'ABA'])('drops the original signature after an owner switch during signing (%s)', async transition => {
    const answer = deferred<HostedJoinProofOutput>();
    const { exchange, hostedJoinProof } = fixture(Promise.resolve(), answer.promise);
    let owner = { ownerId: 'A', generation: 1 };
    exchange.setWorkerOwner(() => owner);
    const pending = exchange.signJoinProof(raw);
    await tick();
    expect(hostedJoinProof).toHaveBeenCalledOnce();
    owner = { ownerId: 'B', generation: 2 };
    if (transition === 'ABA') owner = { ownerId: 'A', generation: 3 };
    answer.resolve(signed);
    await expect(pending).rejects.toMatchObject({ code: 'WORKER_STOPPED' });
  });

  it('does not retarget a pending initialization to a same-owner successor host', async () => {
    const ready = deferred<void>();
    const { exchange, internal, hostedJoinProof } = fixture(ready.promise);
    exchange.setWorkerOwner(() => ({ ownerId: 'A', generation: 1 }));
    const pending = exchange.signJoinProof(raw);
    const successor = vi.fn(async () => signed);
    internal.host = { hostedJoinProof: successor } as unknown as EngineHost;
    internal.hostInitialization = Promise.resolve();
    ready.resolve();
    await expect(pending).rejects.toThrow(/signing|worker|changed/i);
    expect(hostedJoinProof).not.toHaveBeenCalled();
    expect(successor).not.toHaveBeenCalled();
  });

  it('keeps a same-owner pending signature control', async () => {
    const answer = deferred<HostedJoinProofOutput>();
    const { exchange } = fixture(Promise.resolve(), answer.promise);
    exchange.setWorkerOwner(() => ({ ownerId: 'A', generation: 1 }));
    const pending = exchange.signJoinProof(raw);
    await tick();
    answer.resolve(signed);
    await expect(pending).resolves.toEqual(signed);
  });

  it('drops a completed proof when its same-owner signing host was replaced', async () => {
    const answer = deferred<HostedJoinProofOutput>();
    const { exchange, internal } = fixture(Promise.resolve(), answer.promise);
    exchange.setWorkerOwner(() => ({ ownerId: 'A', generation: 1 }));
    const pending = exchange.signJoinProof(raw);
    await tick();
    internal.host = { hostedJoinProof: vi.fn(async () => signed) } as unknown as EngineHost;
    answer.resolve(signed);
    await expect(pending).rejects.toThrow(/signing|worker|changed/i);
  });

  it('refuses an installed but unbound owner before touching the signing host', async () => {
    const { exchange, hostedJoinProof } = fixture();
    exchange.setWorkerOwner(() => undefined);
    await expect(exchange.signJoinProof(raw)).rejects.toMatchObject({ code: 'WORKER_STOPPED' });
    expect(hostedJoinProof).not.toHaveBeenCalled();
  });
});
