import type { RestAdapterVariant } from './transport/types';
import type { ExchangeName, ExchangeOptions, RequestOutcome } from './types';

import axios, { AxiosInstance, CreateAxiosDefaults } from 'axios';

import { BaseExchange } from './exchanges/base';
import { createRemoteSigningAdapter } from './remoteSigningAdapter';
import { getTransport } from './transport/registry';
import { isTealstreetProxyAuthFailureBody } from './transport/tealstreet/tealstreetTransport';
import { enhanceAxiosError } from './utils/axiosErrorEnhancer';
import { isDevelopment } from './utils/isDevelopment';
import { isSigningCaptureRequest } from './utils/signingCapture';
import { markRestRequestStart, noteRestFailure, noteRestSuccess } from './utils/venueRestDiagnostics';

/**
 * Tracks the "real" inner adapter for remote signing instances.
 * When exchange code sets `xhr.defaults.adapter = howAdapter`, it should
 * call `setInnerAdapterForRemoteSigning(xhr, howAdapter)` instead so the
 * remote signing adapter can chain to it.
 */
const innerAdapterSlots = new WeakMap<AxiosInstance, { adapter: any }>();

/**
 * Set the inner adapter (e.g. HOW) for an axios instance that uses remote signing.
 * Call this instead of setting `xhr.defaults.adapter` directly when remote signing is active.
 */
export const setInnerAdapterForRemoteSigning = (instance: AxiosInstance, adapter: any): void => {
  const slot = innerAdapterSlots.get(instance);
  if (slot) {
    slot.adapter = adapter;
  } else {
    // No remote signing on this instance, set directly
    instance.defaults.adapter = adapter;
  }
};

/**
 * Installs the transport's adapter for a tunnelled client, chained under remote
 * signing. A transport that returns none leaves the instance on axios's own adapter.
 */
export const installTransportAdapter = (
  instance: AxiosInstance,
  options: ExchangeOptions,
  exchange: ExchangeName,
  variant: RestAdapterVariant = 'default',
): void => {
  const adapter = getTransport(options).restAdapter({ exchange, options, variant });
  if (adapter) setInnerAdapterForRemoteSigning(instance, adapter);
};

/**
 * Install the remote signing adapter on an axios instance if remoteSigner is set.
 */
const maybeInstallRemoteSigningAdapter = (
  axiosInstance: AxiosInstance,
  options: ExchangeOptions,
  exchangeName: string,
): void => {
  if (!options.remoteSigner) return;

  const slot = { adapter: undefined as any };
  innerAdapterSlots.set(axiosInstance, slot);

  axiosInstance.defaults.adapter = createRemoteSigningAdapter(
    options.remoteSigner,
    exchangeName as any,
    () => slot.adapter,
    options.remoteSigner.accountName,
  );
};

/**
 * Rolling window HTTP request stats tracker
 * Tracks requests per minute using a sliding window
 */
class HttpRequestStatsTracker {
  private timestamps: number[] = [];
  private windowMs = 60000; // 1 minute window
  private logIntervalMs = 10000; // Log every 10 seconds
  private lastLogTime = 0;
  private totalRequests = 0;
  private errorCount = 0;
  private requestsByExchange: Map<string, number> = new Map();

  /**
   * Record a request being made
   */
  recordRequest(exchangeName: string): void {
    const now = Date.now();
    this.timestamps.push(now);
    this.totalRequests++;
    const current = this.requestsByExchange.get(exchangeName) || 0;
    this.requestsByExchange.set(exchangeName, current + 1);
    this.cleanup(now);
    this.maybeLog(now);
  }

  /**
   * Record a request error
   */
  recordError(): void {
    this.errorCount++;
  }

  /**
   * Get requests per minute (in rolling window)
   */
  getRequestsPerMinute(): number {
    this.cleanup(Date.now());
    return this.timestamps.length;
  }

  /**
   * Clean up old timestamps outside the window
   */
  private cleanup(now: number): void {
    const cutoff = now - this.windowMs;
    // Find first valid timestamp index, then slice (O(n) vs O(n²) with shift loop)
    if (this.timestamps.length > 0 && this.timestamps[0] < cutoff) {
      let firstValidIndex = 0;
      while (firstValidIndex < this.timestamps.length && this.timestamps[firstValidIndex] < cutoff) {
        firstValidIndex++;
      }
      if (firstValidIndex > 0) {
        this.timestamps = this.timestamps.slice(firstValidIndex);
      }
    }
  }

  /**
   * Log stats periodically (disabled - enable for debugging)
   */
  private maybeLog(now: number): void {
    if (now - this.lastLogTime >= this.logIntervalMs) {
      // const reqsPerMin = this.getRequestsPerMinute();
      // const reqsPerSec = (reqsPerMin / 60).toFixed(1);
      // const byExchange = Array.from(this.requestsByExchange.entries())
      //   .map(([name, count]) => `${name}:${count}`)
      //   .join(' ');
      // console.log(`[DEBUG] 🌐 HTTP stats: ${reqsPerMin}/min (${reqsPerSec}/sec) | total=${this.totalRequests} errors=${this.errorCount} | ${byExchange}`);
      this.lastLogTime = now;
    }
  }

  /**
   * Get current stats
   */
  getStats(): { perMinute: number; total: number; errors: number } {
    return {
      perMinute: this.getRequestsPerMinute(),
      total: this.totalRequests,
      errors: this.errorCount,
    };
  }
}

// Global HTTP request stats tracker
export const httpStats = new HttpRequestStatsTracker();

export { isNodeOrBunRuntime } from './utils/runtime';

const lastLogReq: { [key: string]: number } = {};
const lastLogRes: { [key: string]: number } = {};

export const LAST_LOG_PERIODICY = 30 * 1_000;

const stringifyResponseBody = (body: unknown): string =>
  typeof body === 'string' ? body : (JSON.stringify(body) ?? String(body));

/**
 * Classify a REST outcome from the HTTP status alone, then record it on the
 * exchange's store.
 *
 * Status-only is deliberate. `health/errorPatterns.ts` classifies far more
 * precisely, but it is a linear regex sweep over ~20 pattern groups against a
 * stringified message and it lives on the main thread inside
 * `ExchangeHealthMonitor` — unreachable from here and far too expensive for a
 * path that runs on every request.
 *
 * An error with NO response (timeout, DNS, CORS, tunnel drop) is `network`,
 * not `other`: the venue never answered, which is a different incident from a
 * venue answering 500, and collapsing them loses that permanently.
 *
 * Never throws. A counter must not be able to fail a request.
 */
const recordRequestOutcome = (
  exchange: BaseExchange,
  status: number | undefined,
  hadResponse: boolean,
  config: unknown,
) => {
  try {
    // A signing-capture round trip never touches the network — counting its
    // synthetic 200 would report one phantom `ok` per signature, and in hosted
    // mode that is every request twice over.
    if (isSigningCaptureRequest(config)) return;

    const outcome: RequestOutcome = !hadResponse
      ? 'network'
      : status === undefined
        ? 'other'
        : status === 429 || status === 418
          ? 'rateLimit'
          : status === 401 || status === 403
            ? 'auth'
            : status >= 200 && status < 300
              ? 'ok'
              : 'other';
    exchange.store?.recordRequestOutcome(outcome);
  } catch {
    /* counting must never break a request */
  }
};

export const safeCexAxiosCreateNoProxy = (
  config: CreateAxiosDefaults,
  options: ExchangeOptions,
  exchange: BaseExchange,
): AxiosInstance => {
  const axiosInstance = axios.create({
    ...config,
    timeout: 20_000,
  });

  // Track HTTP request stats
  axiosInstance.interceptors.request.use(
    (config) => {
      httpStats.recordRequest(exchange.name);
      markRestRequestStart(config);
      return config;
    },
    (error) => Promise.reject(error),
  );

  // Add error interceptor for enhanced error messages
  axiosInstance.interceptors.response.use(
    (response) => {
      recordRequestOutcome(exchange, response.status, true, response.config);
      // Mirrors the signing-capture guard in recordRequestOutcome: the capture
      // adapter's synthetic 200 is not a failure so it would clear rather than
      // record, which is correct — but keep the guard explicit so the two stay
      // in step if either changes.
      // Optional-called: `exchange` is duck-typed in several call paths and in
      // tests, and an interceptor that throws on a missing method would fail the
      // REQUEST. Telemetry must never be able to do that.
      if (!isSigningCaptureRequest(response.config)) exchange.noteHttpResponse?.(response, response.config?.url);
      if (!isSigningCaptureRequest(response.config)) noteRestSuccess(exchange.name, response);
      return response;
    },
    (error) => {
      httpStats.recordError();
      recordRequestOutcome(exchange, error?.response?.status, Boolean(error?.response), error?.config);
      if (!isSigningCaptureRequest(error?.config)) noteRestFailure(exchange.name, error);
      // Enhance the error with more context
      const enhancedError = enhanceAxiosError(error, exchange.name);
      return Promise.reject(enhancedError);
    },
  );

  maybeInstallRemoteSigningAdapter(axiosInstance, options, exchange.name);

  return axiosInstance;
};

export const safeCexAxiosCreate = (
  config: CreateAxiosDefaults,
  options: ExchangeOptions,
  exchange: BaseExchange,
): AxiosInstance => {
  const restTransport = getTransport(options).rest({ exchange: exchange.name, options, config });
  const isPrivateProxyEnabled = restTransport.proxied;
  const axiosInstance = axios.create({
    ...config,
    // 20_000 is more than reasonable just in case
    timeout: 20_000,
    baseURL: restTransport.baseURL,
  });

  // Track HTTP request stats (first interceptor)
  axiosInstance.interceptors.request.use(
    (config) => {
      httpStats.recordRequest(exchange.name);
      markRestRequestStart(config);
      return config;
    },
    (error) => Promise.reject(error),
  );

  axiosInstance.interceptors.request.use(
    async (config) => {
      if (!isPrivateProxyEnabled) {
        return config;
      }
      await restTransport.authorize?.(config);
      return config;
    },
    (error) => {
      return Promise.reject(error);
    },
  );
  // if (exchange.name === 'woofi') {
  //   axiosInstance.interceptors.request.use((config) => {
  //     if (config.method === 'DELETE') {
  //       config.headers['Content-Type'] = 'application/x-www-form-urlencoded';
  //     }
  //     return config;
  //   });
  // }
  axiosInstance.interceptors.request.use(async (config) => {
    let shouldLog = isDevelopment;
    if (!shouldLog) {
      for (const endpoint of exchange.grafanaEndpointsEvery) {
        if (config.url?.includes(endpoint)) {
          shouldLog = true;
          break;
        }
      }
      for (const endpoint of exchange.grafanaEndpointsPeriodic) {
        if (config.url?.includes(endpoint)) {
          if (!lastLogReq[endpoint] || Date.now() - lastLogReq[endpoint] >= LAST_LOG_PERIODICY) {
            shouldLog = true;
            lastLogReq[endpoint] = Date.now();
          }
          break;
        }
      }
    }
    // TODO: These events are NOT sent to Grafana (handler on main thread skips grafanaLog).
    // HOW exchanges already log HTTP traffic to Grafana via grafana-pws/grafana-pws-sub.
    // Decide whether non-HOW exchanges should get parity — would increase Grafana log volume.
    if (shouldLog) {
      try {
        const reqSummary = JSON.stringify({
          method: config.method?.toUpperCase(),
          url: config.url,
          body: config.data,
          params: config.params,
        });
        exchange.emitter.emit(
          'grafana-http-request',
          'Request ' + config.method?.toUpperCase() + ' ' + config.url + ' ' + reqSummary,
        );
      } catch (e) {
        console.error('Error emitting grafana-http-request', e);
      }
    }
    return config;
  });
  axiosInstance.interceptors.response.use(
    async (response) => {
      recordRequestOutcome(exchange, response.status, true, response.config);
      // Mirrors the signing-capture guard in recordRequestOutcome: the capture
      // adapter's synthetic 200 is not a failure so it would clear rather than
      // record, which is correct — but keep the guard explicit so the two stay
      // in step if either changes.
      // Optional-called: `exchange` is duck-typed in several call paths and in
      // tests, and an interceptor that throws on a missing method would fail the
      // REQUEST. Telemetry must never be able to do that.
      if (!isSigningCaptureRequest(response.config)) exchange.noteHttpResponse?.(response, response.config?.url);
      if (!isSigningCaptureRequest(response.config)) noteRestSuccess(exchange.name, response);
      let shouldLog = isDevelopment;
      /* Logging */
      if (!shouldLog) {
        for (const endpoint of exchange.grafanaEndpointsEvery) {
          if (response.config.url?.includes(endpoint)) {
            shouldLog = true;
            break;
          }
        }
        for (const endpoint of exchange.grafanaEndpointsPeriodic) {
          if (response.config.url?.includes(endpoint)) {
            if (!lastLogRes[endpoint] || Date.now() - lastLogRes[endpoint] >= LAST_LOG_PERIODICY) {
              shouldLog = true;
              lastLogRes[endpoint] = Date.now();
            }
            break;
          }
        }
      }
      // TODO: Same as grafana-http-request above — not sent to Grafana yet for non-HOW exchanges.
      if (shouldLog) {
        try {
          const resSummary = JSON.stringify({
            status: response.status,
            body: response.data,
          });
          exchange.emitter.emit(
            'grafana-http-response',
            'Response ' +
              response.config.method?.toUpperCase() +
              ' ' +
              response.config.url +
              ' ' +
              response.status +
              ' ' +
              resSummary,
          );
        } catch (e) {
          console.error('Error emitting grafana-http-response', e);
        }
      }
      /* END of Logging */
      return response;
    },
    function (error) {
      // Enhance the error with more context
      const enhancedError = enhanceAxiosError(error, exchange.name);

      const status = error?.response?.status;

      recordRequestOutcome(exchange, status, Boolean(error?.response), error?.config);
      if (!isSigningCaptureRequest(error?.config)) noteRestFailure(exchange.name, error);

      // Detect proxy/tunnel failures — surface them clearly instead of letting
      // them cascade into cryptic downstream errors or silent broken state.
      if (isPrivateProxyEnabled && status) {
        const body = error?.response?.data;
        const bodyStr = stringifyResponseBody(body);
        if (status === 403 || status === 401) {
          const authFailureSource = isTealstreetProxyAuthFailureBody(body)
            ? 'Tealstreet proxy auth failed'
            : 'Upstream exchange auth failed via proxy';
          const msg = `[${exchange.name}] ${authFailureSource} (${status}): ${bodyStr}`;
          console.error(msg);
          exchange.emitter.emit('error', msg);
        } else if (status >= 500) {
          const msg = `[${exchange.name}] Proxy server error (${status}): ${bodyStr}`;
          console.error(msg);
          exchange.emitter.emit('error', msg);
        }
      }

      if (global?.window) {
        // Grafana logging — browser/main-thread only to avoid flooding from server-side exchanges
        try {
          const isRateLimit = status === 429 || status === 418;
          const prefix = isRateLimit ? '[RATE LIMITED] ' : '';
          exchange.emitter.emit(
            'grafana-error',
            prefix +
              'Response ' +
              error?.config?.method?.toUpperCase() +
              ' ' +
              error?.config?.url +
              ' ' +
              (status ? `HTTP ${status}` : 'Error') +
              ': ' +
              JSON.stringify({
                message: enhancedError?.message,
                context: enhancedError?.context,
                responseBody: error?.response?.data,
              }),
          );
        } catch (e) {
          console.error('Error emitting grafana-error', e);
        }
      }

      return Promise.reject(enhancedError);
    },
  );

  maybeInstallRemoteSigningAdapter(axiosInstance, options, exchange.name);

  return axiosInstance;
};
