import type { Exchange } from '../../exchanges/base';
import type { AppSettingsWorkerType } from '../../webworker/AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../../webworker/ExchangeMetadataWorkerType';

import { vi } from 'vitest';

import { OrderStopTriggerType } from '../../types';

export const NO_ALGOS = 'was not run: this bybit engine build has no algos';

/** A worker exchange for the algo handlers: `rust` runs the account's algos, `refuse` is a build without them. */
export const rustAlgoExchange = (route: 'rust' | 'refuse' = 'rust') => {
  const listeners = new Map<string, (data: unknown) => void>();
  const exchange = {
    name: 'bybit',
    emitter: { emit: vi.fn() },
    runsRustAlgos: route === 'rust',
    algoOp: vi.fn(),
    routeAlgoOp: vi.fn((_account: string, op: string, rust: () => void, refuse: (message: string) => void) =>
      route === 'rust' ? rust() : refuse(`${op} ${NO_ALGOS}`),
    ),
    on: vi.fn((event: string, listener: (data: unknown) => void) => listeners.set(event, listener)),
    off: vi.fn((event: string) => listeners.delete(event)),
    listeners,
  };
  return exchange as typeof exchange & Exchange;
};

/** An exchange that is not a Rust engine account (no `routeAlgoOp`). */
export const plainExchange = () => ({ name: 'bybit', emitter: { emit: vi.fn() } }) as unknown as Exchange;

export const handlerArgs = (exchange: Exchange, ctx: Worker = { postMessage: vi.fn() } as unknown as Worker) =>
  [
    'test-account',
    'bybit',
    exchange,
    ctx,
    {
      placeOrder: { tpTriggerType: OrderStopTriggerType.MarkPrice, slTriggerType: OrderStopTriggerType.LastPrice },
    } as AppSettingsWorkerType,
    {
      supportedTPSLTriggerTypes: [],
      privateProxyIps: [],
      defaultTPTriggerType: undefined,
      defaultSLTriggerType: undefined,
    } as ExchangeMetadataWorkerType,
  ] as const;
