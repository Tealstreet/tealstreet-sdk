import { describe, expect, it } from 'vitest';

import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest, RemoteSignerResponse } from '../types';
import { DefaultStore } from '../store/store.base';
import { BaseExchange } from '../exchanges/base';

class TestExchange extends BaseExchange {
  constructor(options: ExchangeOptions) {
    super('binance', options, new DefaultStore());
  }

  waitForSigner() {
    return this.waitForRemoteSigner();
  }
}

class FakeRemoteSigner implements RemoteSigner {
  accountName?: string;
  private available = false;
  callbacks = new Set<(available: boolean) => void>();

  sign(_request: RemoteSignerRequest): Promise<RemoteSignerResponse> {
    throw new Error('not implemented');
  }

  isAvailable(): boolean {
    return this.available;
  }

  onAvailabilityChange(callback: (available: boolean) => void): () => void {
    this.callbacks.add(callback);
    return () => {
      this.callbacks.delete(callback);
    };
  }

  setAvailable(available: boolean): void {
    this.available = available;
    this.callbacks.forEach((callback) => callback(available));
  }
}

describe('BaseExchange remote signer lifecycle', () => {
  it('unsubscribes availability listeners once signer becomes available', async () => {
    const signer = new FakeRemoteSigner();
    const exchange = new TestExchange({ key: 'key', secret: '', remoteSigner: signer });

    const waitPromise = exchange.waitForSigner();
    expect(signer.callbacks.size).toBe(1);

    signer.setAvailable(true);

    await expect(waitPromise).resolves.toBe(true);
    expect(signer.callbacks.size).toBe(0);
  });

  it('cleans up availability listeners when exchange is disposed', () => {
    const signer = new FakeRemoteSigner();
    const exchange = new TestExchange({ key: 'key', secret: '', remoteSigner: signer });

    void exchange.waitForSigner();
    expect(signer.callbacks.size).toBe(1);

    exchange.dispose();

    expect(signer.callbacks.size).toBe(0);
  });
});
