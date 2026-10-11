import type { InternalAxiosRequestConfig } from 'axios';

export type VenueReply = { status: number; data: unknown };
export type VenueRequest = {
  method: string;
  path: string;
  /** `'post /path'`, the key routes and keyed faults use. */
  key: string;
  body: unknown;
  params: unknown;
  config: InternalAxiosRequestConfig;
};
export type VenueRoute = (request: VenueRequest) => VenueReply;

const parseBody = (data: unknown): unknown => {
  if (typeof data !== 'string') return data;
  try {
    return JSON.parse(data);
  } catch {
    return data;
  }
};

export const toVenueRequest = (config: InternalAxiosRequestConfig): VenueRequest => {
  const method = (config.method ?? 'get').toLowerCase();
  const path = (config.url ?? '').split('?')[0];
  return { method, path, key: `${method} ${path}`, body: parseBody(config.data), params: config.params, config };
};

/**
 * A venue that only knows the routes a test gives it, keyed `'post /path'`.
 * `received` is every request that reached it — a request lost on the way out
 * never appears, one whose RESPONSE was lost does, which is the distinction a
 * bad link turns on. `record` alone is a request the venue saw and refused.
 */
export const createFakeVenue = (routes: Record<string, VenueRoute>) => {
  const received: VenueRequest[] = [];

  const record = (request: VenueRequest) => {
    received.push(request);
  };

  const handle = (request: VenueRequest): VenueReply => {
    record(request);
    const route = routes[request.key];
    return route ? route(request) : { status: 404, data: { message: `no route ${request.key}` } };
  };

  return { received, record, handle };
};

export type FakeVenue = ReturnType<typeof createFakeVenue>;
