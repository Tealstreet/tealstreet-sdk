import type { AxiosInstance } from 'axios';
import type { ExchangeOptions } from '../../../types';

import retry from 'axios-retry';

import { safeCexAxiosCreate } from '../../../safeCexAxiosCreate';
import { LogSeverity } from '../../../types';
import { isGrvtRead } from './grvtRetry';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';
import { BASE_URLS } from '../../../exchanges/grvt/grvt.types';

export function createAPI(opts: ExchangeOptions, exchange: any): AxiosInstance {
  const isTestnet = opts.testnet || false;
  const baseURL = isTestnet ? BASE_URLS.TESTNET_MDG : BASE_URLS.PRODUCTION_MDG;

  const instance = safeCexAxiosCreate(
    {
      baseURL,
      headers: {
        'Content-Type': 'application/json',
      },
      timeout: 30000,
    },
    opts,
    exchange,
  );

  retry(instance, { retries: 5, retryCondition: networkErrorOnRead(isGrvtRead) });

  // NOTE: Market Data Gateway uses direct HTTP, not HOW proxy
  // Commented out to avoid routing public MDG requests through proxy
  // if (typeof window !== 'undefined') {
  //   instance.defaults.adapter = axiosHttpOverWebsocketAdapter;
  // }

  // NOTE: Market Data Gateway is PUBLIC - no authentication headers should be added
  // Session cookies and account IDs are only for private endpoints (TDG/Edge)

  // Response interceptor for error handling
  instance.interceptors.response.use(
    (response) => {
      const data = response.data;

      // Check for error in response body (GRVT can return HTTP 200 with error in body)
      if (data?.c && data?.m) {
        const error = new Error(`GRVT API Error: ${data.m} (code: ${data.c})`);
        return Promise.reject(error);
      }

      // Extract session cookie from response if present
      const setCookieRaw = response.headers['set-cookie'];
      if (setCookieRaw) {
        // Normalize to array (can be string or array depending on adapter/environment)
        const setCookie = Array.isArray(setCookieRaw) ? setCookieRaw : [setCookieRaw];
        const sessionCookie = setCookie.find((cookie: string) => cookie.startsWith('gravity='));
        if (sessionCookie) {
          const cookieValue = sessionCookie.split(';')[0].split('=')[1];
          if (!exchange.apiExtras) {
            exchange.apiExtras = {};
          }
          exchange.apiExtras.sessionCookie = cookieValue;
        }
      }

      // Extract account ID from response header if present
      const accountId = response.headers['x-grvt-account-id'];
      if (accountId) {
        if (!exchange.apiExtras) {
          exchange.apiExtras = {};
        }
        exchange.apiExtras.accountId = accountId;
      }

      return response;
    },
    (error) => {
      if (error.response) {
        const status = error.response.status;
        const data = error.response.data;

        let errorMessage = 'GRVT API Error';

        // Handle lite format error response (c, m, s fields)
        if (data?.c && data?.m) {
          errorMessage = `GRVT API Error: ${data.m} (code: ${data.c})`;
        }
        // Handle full format error response
        else if (data?.error?.message) {
          errorMessage = `GRVT API Error: ${data.error.message}`;
        } else if (data?.message) {
          errorMessage = `GRVT API Error: ${data.message}`;
        } else if (typeof data === 'string') {
          errorMessage = `GRVT API Error: ${data}`;
        }

        const customError = new Error(`${errorMessage} (${status})`);
        (customError as any).originalError = error;
        (customError as any).response = error.response;
        return Promise.reject(customError);
      } else if (error.request) {
        return Promise.reject(new Error('GRVT API Error: No response received'));
      }

      return Promise.reject(error);
    },
  );

  return instance;
}

export function createEdgeAPI(opts: ExchangeOptions, exchange: any): AxiosInstance {
  const isTestnet = opts.testnet || false;
  const baseURL = isTestnet ? BASE_URLS.TESTNET_EDGE : BASE_URLS.PRODUCTION_EDGE;

  const instance = safeCexAxiosCreate(
    {
      baseURL,
      headers: {
        'Content-Type': 'application/json',
      },
      timeout: 30000,
      withCredentials: true,
    },
    opts,
    exchange,
  );

  retry(instance, { retries: 5, retryCondition: networkErrorOnRead(isGrvtRead) });

  // Only use HTTP-over-WebSocket adapter in browser environment
  // In Node.js/CLI, use native HTTP
  // if (typeof window !== 'undefined') {
  //   instance.defaults.adapter = axiosHttpOverWebsocketAdapter;
  // }

  // Response interceptor for extracting cookies
  instance.interceptors.response.use(
    (response) => {
      const data = response.data;

      // Check for error in response body (GRVT can return HTTP 200 with error in body)
      if (data?.c && data?.m) {
        const error = new Error(`GRVT Edge API Error: ${data.m} (code: ${data.c})`);
        return Promise.reject(error);
      }

      // Try to get session cookie from custom header first (exposed by proxy for CORS)
      // Browser security prevents reading Set-Cookie directly, so the proxy copies it
      // to X-Ts-Grvt-Session header which is exposed via Access-Control-Expose-Headers
      const proxySessionCookie = response.headers['x-ts-grvt-session'];
      if (proxySessionCookie) {
        if (!exchange.apiExtras) {
          exchange.apiExtras = {};
        }
        exchange.apiExtras.sessionCookie = proxySessionCookie;
        exchange.log?.('GRVT session cookie refreshed from proxy response', LogSeverity.Debug);
      } else {
        // Fallback: try Set-Cookie (works in Node.js/CLI, not in browser)
        const setCookieRaw = response.headers['set-cookie'];
        if (setCookieRaw) {
          // Normalize to array (can be string or array depending on adapter/environment)
          const setCookie = Array.isArray(setCookieRaw) ? setCookieRaw : [setCookieRaw];
          const sessionCookie = setCookie.find((cookie: string) => cookie.startsWith('gravity='));
          if (sessionCookie) {
            const cookieValue = sessionCookie.split(';')[0].split('=')[1];
            if (!exchange.apiExtras) {
              exchange.apiExtras = {};
            }
            exchange.apiExtras.sessionCookie = cookieValue;
            exchange.log?.('GRVT session cookie refreshed from Set-Cookie response', LogSeverity.Debug);
          }
        }
      }

      const accountId = response.headers['x-grvt-account-id'];
      if (accountId) {
        if (!exchange.apiExtras) {
          exchange.apiExtras = {};
        }
        exchange.apiExtras.accountId = accountId;
        exchange.log?.('GRVT account ID refreshed from authentication response', LogSeverity.Debug);
      }

      return response;
    },
    (error) => {
      if (error.response) {
        const status = error.response.status;
        const data = error.response.data;

        let errorMessage = 'GRVT Edge API Error';

        // Handle lite format error response (c, m, s fields)
        if (data?.c && data?.m) {
          errorMessage = `GRVT Edge API Error: ${data.m} (code: ${data.c})`;
        }
        // Handle full format error response
        else if (data?.error?.message) {
          errorMessage = `GRVT Edge API Error: ${data.error.message}`;
        } else if (data?.message) {
          errorMessage = `GRVT Edge API Error: ${data.message}`;
        } else if (typeof data === 'string') {
          errorMessage = `GRVT Edge API Error: ${data}`;
        }

        const customError = new Error(`${errorMessage} (${status})`);
        (customError as any).originalError = error;
        (customError as any).response = error.response;
        return Promise.reject(customError);
      } else if (error.request) {
        return Promise.reject(new Error('GRVT Edge API Error: No response received'));
      }

      return Promise.reject(error);
    },
  );

  return instance;
}

export function createTradingAPI(opts: ExchangeOptions, exchange: any): AxiosInstance {
  const isTestnet = opts.testnet || false;
  const baseURL = isTestnet ? BASE_URLS.TESTNET_TDG : BASE_URLS.PRODUCTION_TDG;

  const instance = safeCexAxiosCreate(
    {
      baseURL,
      headers: {
        'Content-Type': 'application/json',
      },
      timeout: 30000,
      withCredentials: true, // Enable cookie sending
    },
    opts,
    exchange,
  );

  retry(instance, { retries: 5, retryCondition: networkErrorOnRead(isGrvtRead) });

  // Track if we're currently re-authenticating to prevent loops
  let isReauthenticating = false;

  // Only use HTTP-over-WebSocket adapter in browser environment
  // In Node.js/CLI, use native HTTP
  // if (typeof window !== 'undefined') {
  //   instance.defaults.adapter = axiosHttpOverWebsocketAdapter;
  // }

  // Request interceptor to add GRVT session and account ID
  instance.interceptors.request.use(
    (config) => {
      if (exchange.apiExtras?.sessionCookie) {
        // Send session via custom header - Lambda will convert to Cookie
        // Browser security prevents setting Cookie header directly
        if (typeof config.headers?.set === 'function') {
          config.headers.set('X-Ts-Grvt-Session', exchange.apiExtras.sessionCookie);
        } else {
          config.headers['X-Ts-Grvt-Session'] = exchange.apiExtras.sessionCookie;
        }
        // Store in custom property for HTTP-over-WebSocket adapter to read
        (config as any).grvtCookie = `rm=true; gravity=${exchange.apiExtras.sessionCookie}`;
      }

      if (exchange.apiExtras?.accountId) {
        if (typeof config.headers?.set === 'function') {
          config.headers.set('X-Grvt-Account-Id', exchange.apiExtras.accountId);
        } else {
          config.headers['X-Grvt-Account-Id'] = exchange.apiExtras.accountId;
        }
        // Store in custom property for HTTP-over-WebSocket adapter
        (config as any).grvtAccountId = exchange.apiExtras.accountId;
      }

      return config;
    },
    (error) => {
      return Promise.reject(error);
    },
  );

  instance.interceptors.response.use(
    (response) => {
      const data = response.data;

      // Check for error in response body (GRVT can return HTTP 200 with error in body)
      if (data?.c && data?.m) {
        const error = new Error(`GRVT Trading API Error: ${data.m} (code: ${data.c})`);
        return Promise.reject(error);
      }

      const setCookieRaw = response.headers['set-cookie'];
      if (setCookieRaw) {
        // Normalize to array (can be string or array depending on adapter/environment)
        const setCookie = Array.isArray(setCookieRaw) ? setCookieRaw : [setCookieRaw];
        const sessionCookie = setCookie.find((cookie: string) => cookie.startsWith('gravity='));
        if (sessionCookie) {
          const cookieValue = sessionCookie.split(';')[0].split('=')[1];
          if (!exchange.apiExtras) {
            exchange.apiExtras = {};
          }
          exchange.apiExtras.sessionCookie = cookieValue;
        }
      }

      const accountId = response.headers['x-grvt-account-id'];
      if (accountId) {
        if (!exchange.apiExtras) {
          exchange.apiExtras = {};
        }
        exchange.apiExtras.accountId = accountId;
      }

      return response;
    },
    async (error) => {
      if (error.response) {
        const status = error.response.status;
        const data = error.response.data;
        const errorCode = data?.c || data?.code || data?.error?.code;

        // Check for authentication error (code 1000) and try to re-authenticate
        // Use == to handle both string "1000" and number 1000
        // A staged client-ID cancellation cannot be resent across an unguarded
        // authentication await: that client ID may now belong to another order.
        if (errorCode == 1000 && !isReauthenticating && error.config &&
            !error.config._retry && !error.config.grvtDisableReauthRetry) {
          isReauthenticating = true;
          exchange.log?.('GRVT authentication error 1000; refreshing session', LogSeverity.Warning);

          try {
            // Clear existing session to force re-auth
            if (exchange.apiExtras) {
              exchange.apiExtras.sessionCookie = undefined;
              exchange.apiExtras.accountId = undefined;
            }

            // Clear SDK's cached cookie if available
            if (exchange.grvtClient?.cookie) {
              exchange.grvtClient.cookie = null;
            }

            // Call the exchange's authenticate method
            if (typeof exchange.authenticate === 'function') {
              const authResult = await exchange.authenticate();
              const scope = error.config.grvtCancellationScope;
              if (scope && (exchange.isDisposed || exchange.grvtTradingAccountId !== scope.subAccountId ||
                  !scope.accountId || exchange.apiExtras?.accountId !== scope.accountId)) {
                throw new Error('GRVT exact cancellation disposed or account changed during authentication');
              }
              if (authResult) {
                exchange.log?.('GRVT session refresh succeeded; retrying request', LogSeverity.Debug);
                if (exchange.privateWebsocket?.isConnected) exchange.privateWebsocket.reconnect();

                // Mark the request as a retry to prevent infinite loops
                error.config._retry = true;

                // Update headers with new session
                if (exchange.apiExtras?.sessionCookie) {
                  // Use custom header - Lambda will convert to Cookie
                  error.config.headers['X-Ts-Grvt-Session'] = exchange.apiExtras.sessionCookie;
                  const cookie = `rm=true; gravity=${exchange.apiExtras.sessionCookie}`;
                  if (typeof error.config.headers.set === 'function') error.config.headers.set('Cookie', cookie);
                  else error.config.headers.Cookie = cookie;
                  error.config.grvtCookie = `rm=true; gravity=${exchange.apiExtras.sessionCookie}`;
                }
                if (exchange.apiExtras?.accountId) {
                  error.config.headers['X-Grvt-Account-Id'] = exchange.apiExtras.accountId;
                  error.config.grvtAccountId = exchange.apiExtras.accountId;
                }

                isReauthenticating = false;
                return instance(error.config);
              }
            }
          } catch (authError) {
            exchange.log?.(`GRVT session refresh failed: ${authError}`, LogSeverity.Warning);
          } finally {
            isReauthenticating = false;
          }
        }

        let errorMessage = 'GRVT Trading API Error';

        // Handle lite format error response (c, m, s fields)
        if (data?.c && data?.m) {
          errorMessage = `GRVT Trading API Error: ${data.m} (code: ${data.c})`;
        }
        // Handle full format error response
        else if (data?.error?.message) {
          errorMessage = `GRVT Trading API Error: ${data.error.message}`;
          if (data.error.code) {
            errorMessage += ` (code: ${data.error.code})`;
          }
        } else if (data?.message) {
          errorMessage = `GRVT Trading API Error: ${data.message}`;
        } else if (typeof data === 'string') {
          errorMessage = `GRVT Trading API Error: ${data}`;
        }

        const customError = new Error(`${errorMessage} (${status})`);
        (customError as any).originalError = error;
        (customError as any).response = error.response;
        return Promise.reject(customError);
      } else if (error.request) {
        return Promise.reject(new Error('GRVT Trading API Error: No response received'));
      }

      return Promise.reject(error);
    },
  );

  return instance;
}
