import type { ExchangeAccountConfig, ExchangeName } from '../../types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { normalizeExchangeAccountConfig } from '../../accounts/normalizeExchangeAccountConfig';
import { serializeCommandError } from '../../commandOutcome';
import { accountToExchangeOptions, createExchangeFromAccount } from '../../createExchangeFromAccount';
import { exchangePrettyNames, getExchangeMetadata, isExchangeSupportedOnMobile } from '../../exchangeMetadata';
import { buildPublicExchangeOptions, createExchange, createPublicExchange, preloadExchange } from '../../index';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';

// Retired venues (BitMEX closed 2026-09-23) are still saved on user accounts.
// Every lookup must refuse them with a typed unsupported/notSent error, before
// any clock, WASM or network work, and without disturbing the user's other accounts.
const state = vi.hoisted(() => {
  const calls: string[] = [];
  class Rust {
    constructor(name: string) {
      calls.push(name);
    }
  }
  return { calls, Rust };
});
vi.mock('../../exchanges/rust/rust.exchange', () => ({ RustEngineExchange: state.Rust }));
vi.mock('../../utils/virtualClock', () => ({ virtualClock: { start: vi.fn(), setServerTimeSource: vi.fn() } }));

const REMOVED = ['bitmex', 'bitmextestnet'] as unknown as ExchangeName[];
const unsupported = { kind: 'unsupported', outcome: 'notSent' };
const savedAccount = (exchange: ExchangeName, name = `${exchange}-main`): ExchangeAccountConfig =>
  ({ name, exchange, key: 'saved-key', secret: 'saved-secret', active: true }) as unknown as ExchangeAccountConfig;

const typedRefusal = (run: () => unknown): ExchangeError => {
  try {
    run();
  } catch (error) {
    expect(error).toBeInstanceOf(ExchangeError);
    expect(error).toMatchObject(unsupported);
    return error as ExchangeError;
  }
  throw new Error('expected a typed refusal');
};

// Metadata lookups stay free of the WASM host (the Rust source oracles load them),
// so their refusal carries the classification fields rather than the class.
const classifiedRefusal = (run: () => unknown): Error => {
  try {
    run();
  } catch (error) {
    expect(error).toMatchObject(unsupported);
    expect(serializeCommandError(error)).toMatchObject(unsupported);
    return error as Error;
  }
  throw new Error('expected a classified refusal');
};

beforeEach(() => {
  state.calls.length = 0;
  vi.mocked(virtualClock.start).mockClear();
});

describe('a removed venue beside live accounts', () => {
  it('fails only its own account', async () => {
    const accounts = [
      savedAccount('binance'),
      savedAccount(REMOVED[0]!),
      savedAccount('okx'),
      savedAccount(REMOVED[1]!),
    ];
    const results: PromiseSettledResult<unknown>[] = [];
    for (const account of accounts) results.push(...(await Promise.allSettled([createExchangeFromAccount(account)])));
    expect(results.map((result) => result.status)).toEqual(['fulfilled', 'rejected', 'fulfilled', 'rejected']);
    expect((results[0] as PromiseFulfilledResult<unknown>).value).toBeInstanceOf(state.Rust);
    expect((results[2] as PromiseFulfilledResult<unknown>).value).toBeInstanceOf(state.Rust);
    for (const index of [1, 3]) {
      expect((results[index] as PromiseRejectedResult).reason).toMatchObject(unsupported);
    }
    expect(state.calls).toEqual(['binance', 'okx']);
  });
});

describe.each(REMOVED)('removed venue %s', (name) => {
  it('refuses classified on every synchronous metadata lookup', () => {
    expect(classifiedRefusal(() => getExchangeMetadata(name)).message).toBe(`Unknown exchange metadata: ${name}`);
    classifiedRefusal(() => buildPublicExchangeOptions(name));
    classifiedRefusal(() => normalizeExchangeAccountConfig(savedAccount(name)));
    classifiedRefusal(() => accountToExchangeOptions(savedAccount(name)));
    expect(isExchangeSupportedOnMobile(name)).toBe(false);
    expect(exchangePrettyNames[name]).toBeUndefined();
  });

  it('refuses typed in every factory before clock or constructor work', async () => {
    await expect(createExchangeFromAccount(savedAccount(name))).rejects.toMatchObject({
      ...unsupported,
      message: `${name} is unsupported in this build`,
    });
    await expect(createExchange(name, { key: 'k', secret: 's' })).rejects.toBeInstanceOf(ExchangeError);
    await expect(createExchange(name, { key: 'k', secret: 's' })).rejects.toMatchObject(unsupported);
    await expect(createPublicExchange(name)).rejects.toMatchObject(unsupported);
    await expect(preloadExchange(name)).rejects.toMatchObject(unsupported);
    expect(virtualClock.start).not.toHaveBeenCalled();
    expect(state.calls).toEqual([]);
  });

  it('refuses typed in the direct Rust adapter constructor', async () => {
    const { RustEngineExchange } = await vi.importActual<typeof import('../../exchanges/rust/rust.exchange')>(
      '../../exchanges/rust/rust.exchange',
    );
    typedRefusal(() => new RustEngineExchange(name, { key: 'k', secret: 's' }, new DefaultStore()));
  });

  it('fails worker startup cleanly instead of rejecting out of start()', async () => {
    const { ExchangeWorker } = await import('../../webworker/ExchangeWorker');
    const messages: unknown[] = [];
    const worker = new ExchangeWorker(
      { postMessage: (message: unknown) => messages.push(message) } as unknown as Worker,
      `${name}-main`,
      name,
      { key: 'k', secret: 's' },
      {} as never,
      {} as never,
      'session',
    );
    await expect(worker.start()).resolves.toBe(false);
    expect(JSON.stringify(messages)).toContain(`${name} is unsupported in this build`);
    expect(state.calls).toEqual([]);
    worker.dispose();
  });
});
