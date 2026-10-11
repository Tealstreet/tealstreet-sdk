import type { AxiosAdapter, AxiosInstance, AxiosResponse, InternalAxiosRequestConfig } from 'axios';
import type { FakeVenue } from './fakeVenue';

import { AxiosError } from 'axios';

import { setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import { toVenueRequest } from './fakeVenue';

/**
 * What the link does to ONE request; an empty script delivers.
 * - `dropBeforeSend`: reset before the venue sees it.
 * - `loseResponse`: the venue receives and acts on it, then the reply is lost.
 * - `timeout`: the venue acts on it and the client gives up (ECONNABORTED).
 * - `latencyMs`: delay before the venue sees it.
 * - `hold`: the venue acts on it NOW; the reply waits for the promise (ignores `config.timeout`).
 * - `reject`: the venue refuses it with an HTTP error, changing nothing.
 */
export type Fault =
  | 'deliver'
  | 'dropBeforeSend'
  | 'loseResponse'
  | 'timeout'
  | { latencyMs: number }
  | { hold: Promise<unknown> }
  | { reject: number; data?: unknown };

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

// The request argument is what makes enhanceAxiosError call it a network error.
const linkError = (message: string, code: string, config: InternalAxiosRequestConfig) =>
  new AxiosError(message, code, config, {});

const toResponse = (config: InternalAxiosRequestConfig, status: number, data: unknown): AxiosResponse => ({
  data,
  status,
  statusText: String(status),
  headers: {},
  config,
});

// An adapter applies validateStatus itself; axios does not do it for us.
const settle = (response: AxiosResponse): AxiosResponse => {
  if (response.status >= 200 && response.status < 300) return response;
  const code = response.status >= 500 ? AxiosError.ERR_BAD_RESPONSE : AxiosError.ERR_BAD_REQUEST;
  throw new AxiosError(`Request failed with status code ${response.status}`, code, response.config, {}, response);
};

/**
 * Faults are consumed per route first (`on('post /path', ...)`), then from the
 * unkeyed script (`next(...)`). Key them whenever requests run concurrently:
 * which request takes an unkeyed fault then depends on microtask order. A write
 * often follows a read (position mode, a GET retry), so key a fault meant for the
 * write, and assert `pending() === 0` so a fault nobody took cannot pass silently.
 * Under fake timers, advance time for retries — axios-retry waits with setTimeout even at 0ms.
 */
export const createFaultAdapter = (venue: FakeVenue) => {
  const script: Fault[] = [];
  const keyed = new Map<string, Fault[]>();

  const take = (key: string): Fault => keyed.get(key)?.shift() ?? script.shift() ?? 'deliver';

  const adapter: AxiosAdapter = async (config) => {
    const request = toVenueRequest(config);
    const fault = take(request.key);

    if (fault === 'dropBeforeSend') throw linkError('socket hang up', 'ECONNRESET', config);
    if (typeof fault === 'object' && 'latencyMs' in fault) await sleep(fault.latencyMs);
    if (typeof fault === 'object' && 'reject' in fault) {
      venue.record(request);
      return settle(toResponse(config, fault.reject, fault.data ?? {}));
    }

    const reply = venue.handle(request);
    if (fault === 'loseResponse') throw linkError('socket hang up', 'ECONNRESET', config);
    if (fault === 'timeout') throw linkError(`timeout of ${config.timeout ?? 0}ms exceeded`, 'ECONNABORTED', config);
    if (typeof fault === 'object' && 'hold' in fault) await fault.hold;
    return settle(toResponse(config, reply.status, reply.data));
  };

  return {
    adapter,
    /** Queues what the link does to the next requests, in order. */
    next: (...faults: Fault[]) => {
      script.push(...faults);
    },
    /** Queues faults for one route, e.g. `on('post /api/v5/trade/batch-orders', 'loseResponse')`. */
    on: (key: string, ...faults: Fault[]) => {
      keyed.set(key, [...(keyed.get(key) ?? []), ...faults]);
    },
    pending: () => script.length + [...keyed.values()].reduce((sum, faults) => sum + faults.length, 0),
    /**
     * Routes a real venue factory's requests through the bench, beneath any
     * remote-signing layer. With a secondary key `exchange.xhr` is a Proxy;
     * install on `_primaryXhr` / `_secondaryXhr` instead.
     */
    install: (xhr: AxiosInstance) => setInnerAdapterForRemoteSigning(xhr, adapter),
  };
};

export type FaultBench = ReturnType<typeof createFaultAdapter>;
