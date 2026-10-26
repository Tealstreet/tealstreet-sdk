import type { Axios, AxiosRequestConfig } from 'axios';
import type { IApiCancelOrderRequest } from '@tealstreet/grvt-client/interfaces';
import type { Store } from '../../../store/store.interface';
import type { GrvtInstrumentLite, GrvtKline, GrvtOrderSide, GrvtOrderType, GrvtTimeInForce } from '../../../exchanges/grvt/grvt.types';

import rateLimit from 'axios-rate-limit';
import { ethers } from 'ethers';

import {
  domain as baseDomain,
  EGrvtEnvironment,
  GrvtClient,
  Signer as GrvtSigner,
  OrderWithBuilderFee as OrderWithBuilderFeeTypes,
} from '@tealstreet/grvt-ts-sdk';
import { sleep } from '@tealstreet/sleep/src/sleep';
import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  FetchedLeverage,
  LogSeverity,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  UpdateOrderOpts,
  ValidateAccountResult,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { loop } from '../../../utils/loop';
import { adjust } from '../../../utils/safe-math';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import type { ClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI, createEdgeAPI, createTradingAPI } from './grvt.api';
import {
  createGrvtBuilderInfo,
  DEFAULT_GRVT_BUILDER_FEE_RATE,
  getGrvtBuilderAddress,
  grvtBuilderFeeRateToRaw,
  GrvtBuilderInfo,
  GrvtBuilderUnauthorizedError,
} from '../../../exchanges/grvt/grvt.builder';
import { grvtNanoseconds, grvtScaledUint64, grvtUint64 } from './grvt.integer';
import { validatePrivateKey } from './grvt.signing';
import { ENDPOINTS, GRAFANA_ENDPOINTS_EVERY, GRAFANA_ENDPOINTS_PERIODIC, GrvtCandlestickInterval } from '../../../exchanges/grvt/grvt.types';
import { GrvtPrivateWebSocket } from './grvt.ws-private';
import { GrvtPublicWebSocket } from './grvt.ws-public';
import { getGrvtOrderTimeInForce, getGrvtTriggerMetadata } from './grvt.order-semantics';

type GrvtBuilderAuthState =
  | { status: 'unknown' }
  | { status: 'authorized' }
  | { status: 'unauthorized' }
  | { status: 'cap-too-low'; authorizedRaw?: number };

type GrvtInstrumentSigningMetadata = Required<Pick<GrvtInstrumentLite, 'ih' | 'bd' | 'qd'>>;

export class GrvtExchange extends BaseExchange {
  // GRVT client_order_id is a uint64; 2^63 and above are reserved for GRVT's own UI.
  protected readonly clientOrderIdRule: ClientOrderIdRule = {
    label: 'GRVT',
    requirement: 'be a canonical decimal integer between 1 and 9223372036854775807',
    isValid: (clientOrderId) => /^[1-9]\d{0,18}$/.test(clientOrderId) && BigInt(clientOrderId) < 2n ** 63n,
  };

  xhr: Axios; // Market Data Gateway (public)
  edgeXhr: Axios; // Edge Gateway (authentication)
  tradingXhr: Axios; // Trading Data Gateway (private)
  publicWebsocket: GrvtPublicWebSocket;
  privateWebsocket: GrvtPrivateWebSocket;
  wallet?: ethers.Wallet; // Ethereum wallet for order signing
  grvtClient?: GrvtClient; // Official GRVT SDK client
  grvtTradingAccountId?: string; // Trading account ID for sub-account queries
  apiExtras?: {
    sessionCookie?: string;
    accountId?: string;
    subAccountId?: string;
  };

  // Default leverage used when leverage data is not yet loaded from API
  private defaultLeverage = 10;

  // Maps nonce (client_order_id) → real GRVT order_id (hex).
  // The nonce is the canonical order ID in the store/chaser. The real GRVT
  // order_id is needed for cancelOrder API calls. Populated by WS/REST when
  // GRVT assigns a real oi (non-"0x00").
  _nonceToOrderId = new Map<string, string>();
  // Keep creation times with the canonical nonce lifecycle. `undefined`
  // means we cannot date a pre-existing order and must query safely broadly.
  _nonceCreationTimes = new Map<string, number | undefined>();
  private static readonly MAX_NONCE_MAPPINGS = 200;
  private static readonly CHASER_FILL_CLOCK_SKEW_MS = 5 * 60 * 1000;
  private static readonly CHASER_FILL_PAGE_SIZE = 1000;
  private static readonly MAX_CHASER_FILL_PAGES = 20;

  /**
   * Detailed builder authorization state. Tealstreet refuses to place GRVT orders
   * unless the user has authorized the builder with a cap that covers our current
   * fee, so each distinct failure mode gets its own status.
   *
   * - `unknown`     — never checked yet (or last check failed); will retry on demand
   * - `authorized`  — cap is sufficient, attach builder fee to orders
   * - `unauthorized` — no authorization for our builder address
   * - `cap-too-low` — authorization exists but the signed cap is below our required fee
   */
  private _builderAuthState: GrvtBuilderAuthState = { status: 'unknown' };

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    // Initialize apiExtras with subAccountId if provided
    if (opts.subaccount) {
      this.apiExtras = {
        subAccountId: opts.subaccount,
      };
    }

    // Initialize wallet if privateKey is provided
    if (opts.secret) {
      try {
        if (validatePrivateKey(opts.secret)) {
          this.wallet = new ethers.Wallet(opts.secret);
          this.log(`Wallet initialized: ${this.wallet.address}`, LogSeverity.Info);

          // Validate that provided signer address matches derived wallet address
          if (opts.grvtSignerAddress) {
            const providedAddress = opts.grvtSignerAddress.toLowerCase();
            const walletAddress = this.wallet.address.toLowerCase();

            if (providedAddress !== walletAddress) {
              this.log(
                `⚠️ Warning: Provided signer address (${opts.grvtSignerAddress}) does not match wallet address (${this.wallet.address}). ` +
                  `Make sure ${this.wallet.address} is tagged to your API key in the GRVT UI.`,
                LogSeverity.Warning,
              );
            } else {
              this.log(`Signer address verified: ${this.wallet.address}`, LogSeverity.Info);
            }
          } else {
            this.log(
              `⚠️ No signer address provided. Make sure ${this.wallet.address} is tagged to your API key in the GRVT UI.`,
              LogSeverity.Warning,
            );
          }
        } else {
          this.log('Invalid private key provided', LogSeverity.Warning);
        }
      } catch (err: any) {
        this.log(`Failed to initialize wallet: ${err.message}`, LogSeverity.Error);
      }
    }

    // Initialize trading account ID from options
    this.grvtTradingAccountId = opts.grvtTradingAccountId;

    // Rate limit to conservative 10 RPS
    this.xhr = rateLimit(createAPI(opts, this), { maxRPS: 10 });
    this.edgeXhr = rateLimit(createEdgeAPI(opts, this), { maxRPS: 10 });
    this.tradingXhr = rateLimit(createTradingAPI(opts, this), { maxRPS: 10 });

    // Initialize GRVT SDK client with custom axios instances for proxy support
    if (opts.key && opts.secret) {
      try {
        this.grvtClient = new GrvtClient({
          apiKey: opts.key,
          apiSecret: opts.secret,
          env: opts.testnet ? EGrvtEnvironment.TESTNET : EGrvtEnvironment.PRODUCTION,
          customAxios: this.tradingXhr, // Use our custom axios for TDG/MDG requests
          customEdgeAxios: this.edgeXhr, // Use our custom axios for Edge API (authentication)
        });
        this.log('GRVT SDK client initialized', LogSeverity.Info);
      } catch (err: any) {
        this.log(`Failed to initialize GRVT SDK client: ${err.message}`, LogSeverity.Warning);
      }
    }

    // Initialize WebSocket objects (but don't connect yet - will connect after markets are loaded)
    this.publicWebsocket = new GrvtPublicWebSocket(this);
    this.privateWebsocket = new GrvtPrivateWebSocket(this);
  }

  dispose = () => {
    this.isDisposed = true;
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
    // Note: loop() automatically stops when isDisposed is true
  };

  recordNonce(nonce: string, realOrderId?: string, creationTimeMs?: number) {
    const existingCreationTime = this._nonceCreationTimes.get(nonce);
    const hasExistingCreationTime = this._nonceCreationTimes.has(nonce);
    const validCreationTime =
      creationTimeMs !== undefined && Number.isFinite(creationTimeMs) && creationTimeMs > 0;

    if (!hasExistingCreationTime) {
      this._nonceCreationTimes.set(nonce, validCreationTime ? creationTimeMs : undefined);
    } else if (validCreationTime && (existingCreationTime === undefined || creationTimeMs < existingCreationTime)) {
      // Never narrow an existing nonce's reconciliation window.
      this._nonceCreationTimes.set(nonce, creationTimeMs);
    }

    if (realOrderId && realOrderId !== '0x00' && realOrderId !== '0x0') {
      this._nonceToOrderId.set(nonce, realOrderId);
    }

    while (this._nonceCreationTimes.size > GrvtExchange.MAX_NONCE_MAPPINGS) {
      const oldestNonce = this._nonceCreationTimes.keys().next().value!;
      this._nonceCreationTimes.delete(oldestNonce);
      this._nonceToOrderId.delete(oldestNonce);
    }
  }

  getOrderCreationTimeMs(metadata: any): number | undefined {
    const rawCreationTime = metadata?.create_time ?? metadata?.ct;
    if (typeof rawCreationTime !== 'string' && typeof rawCreationTime !== 'number') return undefined;
    const rawString = String(rawCreationTime);
    if (!/^\d+$/.test(rawString)) return undefined;

    try {
      const creationTimeMs = Number(BigInt(rawString) / 1_000_000n);
      return Number.isFinite(creationTimeMs) && creationTimeMs > 0 ? creationTimeMs : undefined;
    } catch {
      return undefined;
    }
  }

  /**
   * Periodic balance refresh tick
   *
   * Runs every 60 seconds to refresh balance from REST API.
   * This ensures wallet balance stays current with:
   * - Deposits and withdrawals
   * - Funding payments
   * - Realized P&L from closed positions
   *
   * Note: Unrealized P&L is updated via WebSocket position events,
   * so the adjusted balance (total + upnl) updates in real-time.
   *
   * Rate limit safety: 60s interval = ~0.17 req/10s, well below
   * GRVT's limit of 25-225 req/10s (tier-based).
   */
  tick = async () => {
    if (!this.isDisposed) {
      if (this.isPublicDataOnly) return;

      try {
        const balance = await this.fetchBalance();
        if (this.isDisposed) return;

        this.store.update({
          balance,
          loaded: { ...this.store.loaded, balance: true },
        });

        // Re-derive upnl from live ticker prices so the REST API's
        // potentially stale total_upnl doesn't freeze the display.
        this.syncBalanceUpnlFromPositions();

        this.log('Balance refreshed via tick()', LogSeverity.Debug);
      } catch (err: any) {
        this.log(`Error in tick: ${err?.message}`, LogSeverity.Warning);
        // Silent failure - will retry on next tick
      }

      // Schedule next tick in 60 seconds
      // loop() automatically checks isDisposed before executing
      loop(() => this.tick(), 60_000);
    }
  };

  async getPositionMode(): Promise<PositionMode> {
    // GRVT supports both one-way and hedge mode
    // Default to one-way for now
    return PositionMode.OneWay;
  }

  /**
   * Convert uppercased symbol to GRVT's original format
   * BTC_USDT_PERP → BTC_USDT_Perp
   * ETH_USDT_PERP → ETH_USDT_Perp
   */
  private symbolToInstrumentId(symbol: string): string {
    // Replace _PERP (uppercase) with _Perp (title case)
    return symbol.replace(/_PERP$/i, '_Perp');
  }

  /**
   * Map time-in-force string to GRVT enum value
   * 0 = GOOD_TILL_TIME, 1 = ALL_OR_NONE, 2 = IMMEDIATE_OR_CANCEL, 3 = FILL_OR_KILL
   */
  private mapTimeInForceToEnum(timeInForce: GrvtTimeInForce): number {
    // Python SDK mapping (grvt_raw_signing.py)
    const mapping: Record<GrvtTimeInForce, number> = {
      GOOD_TILL_TIME: 1, // SignTimeInForce.GOOD_TILL_TIME
      ALL_OR_NONE: 2, // SignTimeInForce.ALL_OR_NONE
      IMMEDIATE_OR_CANCEL: 3, // SignTimeInForce.IMMEDIATE_OR_CANCEL
      FILL_OR_KILL: 4, // SignTimeInForce.FILL_OR_KILL
    };
    return mapping[timeInForce] ?? 1;
  }

  private calculatePositionUpnlFromLastPrice = (position: Position): number | null => {
    const ticker = this.store.tickersBySymbol[position.symbol];
    const lastPrice = ticker?.last;

    if (!lastPrice || lastPrice <= 0 || !position.entryPrice || position.entryPrice <= 0 || !position.contracts) {
      return null;
    }

    const direction = position.side === PositionSide.Long ? 1 : -1;
    const derivedUpnl = direction * (lastPrice - position.entryPrice) * Math.abs(position.contracts);

    return Number.isFinite(derivedUpnl) ? derivedUpnl : null;
  };

  private calculateUpnlFromPositions = (positions: Position[]): number => {
    return positions.reduce((total, position) => {
      if (!position || Math.abs(position.contracts || 0) === 0) {
        return total;
      }

      const derivedUpnl = this.calculatePositionUpnlFromLastPrice(position);
      if (derivedUpnl !== null) {
        return total + derivedUpnl;
      }

      const positionUpnl = Number(position.unrealizedPnl);
      return Number.isFinite(positionUpnl) ? total + positionUpnl : total;
    }, 0);
  };

  syncBalanceUpnlFromPositions = (positions: Position[] = this.store.positions) => {
    const positionUpdates: Array<[Pick<Position, 'id'>, Partial<Position>]> = [];
    let totalUpnl = 0;

    for (const position of positions) {
      if (!position || Math.abs(position.contracts || 0) === 0) {
        continue;
      }

      const derivedUpnl = this.calculatePositionUpnlFromLastPrice(position);
      if (derivedUpnl !== null) {
        totalUpnl += derivedUpnl;
        if (Math.abs((position.unrealizedPnl || 0) - derivedUpnl) > 1e-9) {
          positionUpdates.push([position, { unrealizedPnl: derivedUpnl }]);
        }
      } else {
        const positionUpnl = Number(position.unrealizedPnl);
        totalUpnl += Number.isFinite(positionUpnl) ? positionUpnl : 0;
      }
    }

    if (!Number.isFinite(totalUpnl)) {
      return;
    }

    if (positionUpdates.length > 0) {
      this.store.updatePositions(positionUpdates);
    }

    if (Math.abs((this.store.balance.upnl || 0) - totalUpnl) > 1e-9) {
      this.store.update({
        balance: {
          ...this.store.balance,
          upnl: totalUpnl,
        },
      });
    }
  };

  /**
   * Authenticate with GRVT and obtain session cookie
   * Made public to allow re-authentication from API interceptors on error 1000
   */
  authenticate = async (): Promise<boolean> => {
    try {
      // Note: Browser security prevents setting Cookie header directly
      // The rm=true (remember me) functionality is handled by withCredentials
      // and the proxy will forward cookies automatically
      await this.edgeXhr.post(
        ENDPOINTS.AUTH,
        {
          api_key: this.options.key,
        },
        {
          // Use custom header for proxy to add rm=true cookie if needed
          headers: {
            'X-Grvt-Remember-Me': 'true',
          },
        },
      );

      // Session cookie and account ID are extracted automatically by the Edge API interceptor
      if (this.apiExtras?.sessionCookie && this.apiExtras?.accountId) {
        this.log('Successfully authenticated with GRVT', LogSeverity.Info);

        // Sync session to the SDK client so it doesn't try to re-authenticate independently.
        // Without this, the SDK's refreshCookie() would make its own auth call through edgeAxios,
        // which can fail if the proxy is unavailable, causing unauthorized errors.
        if (this.grvtClient) {
          (this.grvtClient as any).cookie = {
            gravity: this.apiExtras.sessionCookie,
            expires: Date.now() / 1000 + 3600, // 1 hour
            XGrvtAccountId: this.apiExtras.accountId,
          };
        }

        return true;
      }

      this.log('Authentication response did not include required cookies/headers', LogSeverity.Error);
      return false;
    } catch (err: any) {
      this.log(`Authentication failed: ${err.message}`, LogSeverity.Error);
      throw err;
    }
  };

  validateAccount = async (): Promise<ValidateAccountResult> => {
    try {
      // Authenticate and get session
      await this.authenticate();

      if (!this.grvtClient) {
        throw new Error('GRVT SDK client not initialized');
      }

      // Verify session by fetching funding account balance
      // This is more reliable than getSubAccountSummary for validation
      const response = await this.grvtClient.getFundingAccountSummary();

      if (response) {
        this.log('Account validated successfully', LogSeverity.Info);
        return { error: '' };
      }

      return { error: 'Failed to validate account' };
    } catch (err: any) {
      // The error message has already been formatted by the API interceptor
      const errorMsg = err.message || 'Invalid API credentials';

      this.emitAndLogError(errorMsg);
      return { error: errorMsg };
    }
  };

  /**
   * Fetch and cache builder authorization status from GRVT API.
   * Called during exchange startup (non-blocking background call).
   * Only checks if we have an API key (for authenticated connections).
   *
   * Delegates to `POST /full/v1/get_authorized_builders` to determine whether the
   * Tealstreet builder is authorized AND whether the on-chain `maxFutureFeeRate`
   * cap covers the fee we're about to send (`DEFAULT_GRVT_BUILDER_FEE_RATE`).
   *
   * Result is stored in `_builderAuthState`. The state machine has four members:
   *   - `authorized`     → cap covers our fee, attach builder fee to orders
   *   - `cap-too-low`    → user authorized but cap is below required (re-auth needed)
   *   - `unauthorized`   → user has not authorized our builder for this sub-account
   *   - `unknown`        → check failed; will be retried lazily on next order placement
   *
   * Tealstreet refuses to place GRVT orders without a valid builder fee, so any
   * non-authorized state causes `getBuilderInfo` (and therefore `placeOrder`) to
   * throw a `GrvtBuilderUnauthorizedError`.
   */
  private fetchBuilderAuthorization = async (): Promise<void> => {
    // Only check for authenticated connections with API key
    if (!this.options.key) {
      this._builderAuthState = { status: 'unauthorized' };
      return;
    }

    // Ensure we have a valid session before making authenticated API calls
    if (!this.apiExtras?.sessionCookie) {
      this.log('No session cookie found, authenticating before builder authorization check...', LogSeverity.Debug);
      const authenticated = await this.authenticate();
      if (!authenticated) {
        this.log('Authentication failed, cannot check builder authorization', LogSeverity.Warning);
        this._builderAuthState = { status: 'unknown' };
        return;
      }
    }

    const maxRetries = 3;
    const builderAddress = getGrvtBuilderAddress(this.options.testnet ?? false);

    for (let attempt = 1; attempt <= maxRetries; attempt++) {
      try {
        this.log(
          `Checking builder authorization for ${builderAddress} (attempt ${attempt}/${maxRetries})`,
          LogSeverity.Debug,
        );

        const response = await this.tradingXhr.post(ENDPOINTS.GET_AUTHORIZED_BUILDERS, {});

        const data = response.data;
        // Response format: { results: [...] } (full) or { r: [...] } (lite)
        const builders = data?.results || data?.r || [];

        this.log(
          `get_authorized_builders returned ${builders.length} builder(s)`,
          LogSeverity.Debug,
        );

        const requiredRaw = grvtBuilderFeeRateToRaw(DEFAULT_GRVT_BUILDER_FEE_RATE);
        let nextState: GrvtBuilderAuthState = { status: 'unauthorized' };
        for (const builderInfo of builders) {
          // Full format: builder_account_id, Lite format: ba
          const addr = (builderInfo.builder_account_id || builderInfo.ba || '').toLowerCase();
          if (addr !== builderAddress.toLowerCase()) continue;

          // GRVT returns the percentage *string* (e.g. "0.03" for 0.03% = 3 bps),
          // NOT the EIP-712 uint32. Convert it via `grvtBuilderFeeRateToRaw` so we
          // can compare against `requiredRaw` (which uses the `percent * 10000`
          // convention shared with order signing). If the existing cap is below
          // what we're about to send, GRVT would reject the order — flag as
          // cap-too-low so the UI can prompt the user to re-authorize.
          const rawFeeRateString = builderInfo.max_futures_fee_rate ?? builderInfo.max_fee_rate ?? builderInfo.mfr;
          const authorizedRaw =
            rawFeeRateString != null
              ? (() => {
                  try {
                    return grvtBuilderFeeRateToRaw(String(rawFeeRateString));
                  } catch {
                    return undefined;
                  }
                })()
              : undefined;
          if (authorizedRaw !== undefined && authorizedRaw < requiredRaw) {
            this.log(
              `Builder ${builderAddress} authorized cap (${authorizedRaw}) is below required (${requiredRaw}). Re-authorization is required before orders can be placed.`,
              LogSeverity.Warning,
            );
            nextState = { status: 'cap-too-low', authorizedRaw };
            break;
          }
          nextState = { status: 'authorized' };
          break;
        }

        this._builderAuthState = nextState;
        this.log(
          `Builder authorization status: ${nextState.status}`,
          nextState.status === 'authorized' ? LogSeverity.Info : LogSeverity.Warning,
        );
        return; // Success — exit retry loop
      } catch (err) {
        this.log(
          `Failed to check builder authorization (attempt ${attempt}/${maxRetries}): ${err}`,
          LogSeverity.Warning,
        );
        if (attempt < maxRetries) {
          await sleep(2000 * attempt); // 2s, 4s backoff
        } else {
          this._builderAuthState = { status: 'unknown' }; // Keep as unknown after all retries exhausted
        }
      }
    }
  };

  /**
   * Get builder info for orders. Throws `GrvtBuilderUnauthorizedError` if the user
   * has not authorized the builder, or has authorized it with a cap below the fee
   * Tealstreet now sends. Tealstreet refuses to place GRVT orders without a valid
   * `builder` / `builder_fee` pair so the consumer must catch this error and prompt
   * the user to (re-)authorize.
   *
   * If the cached status is `unknown` (e.g. startup probe failed), this method will
   * trigger a fresh check before deciding.
   *
   * When authorized, returns a GrvtBuilderInfo object used in two places:
   *   1. **EIP-712 signed message** — uses `OrderWithBuilderFee` type template instead of `Order`:
   *      - `builder`: builder address (lowercase), e.g. "0x79a52a6658c23cf7e7fe4fe43ff91c5195eb86ca"
   *      - `builderFee`: uint32 = percent * 10000 (e.g., "0.03" → 300)
   *   2. **API payload** — included in both full and lite format order bodies:
   *      - Full: `{ builder: "0x...", builder_fee: "0.03" }`
   *      - Lite: `{ b: "0x...", bf: "0.03" }`
   */
  private getBuilderInfo = async (): Promise<GrvtBuilderInfo> => {
    // If status is unknown, fetch it before deciding
    if (this._builderAuthState.status === 'unknown') {
      await this.fetchBuilderAuthorization();
    }

    const requiredRaw = grvtBuilderFeeRateToRaw(DEFAULT_GRVT_BUILDER_FEE_RATE);

    if (this._builderAuthState.status === 'authorized') {
      return createGrvtBuilderInfo(this.options.testnet ?? false);
    }

    // `unknown` after a fresh fetch means the check itself failed — treat as
    // unauthorized so the user is prompted rather than silently dropping the fee.
    if (this._builderAuthState.status === 'cap-too-low') {
      throw new GrvtBuilderUnauthorizedError({
        reason: 'cap-too-low',
        requiredRaw,
        authorizedRaw: this._builderAuthState.authorizedRaw,
      });
    }
    throw new GrvtBuilderUnauthorizedError({
      reason: 'unauthorized',
      requiredRaw,
    });
  };

  getAccount = async () => {
    try {
      await this.tradingXhr.post(ENDPOINTS.ACCOUNT_SUMMARY, {
        sub_account_id: this.apiExtras?.subAccountId || '', // empty string for default sub-account
      });

      return {
        userId: this.apiExtras?.accountId || '',
        affiliateId: null,
      };
    } catch (err) {
      this.log(`Error fetching account: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  start = async () => {
    try {
      this.log('Starting GRVT exchange...', LogSeverity.Info);

      const useExternalPublicData = await this.initExternalPublicData();
      const publicDataOnly = this.isPublicDataOnly;

      // Authenticate if not already done (skip for public-only mode)
      if (!publicDataOnly && !this.apiExtras?.sessionCookie) {
        await this.authenticate();
      }

      if (!useExternalPublicData) {
        // Fetch and set markets in store with loaded flag
        const markets = await this.fetchMarkets();

        this.store.update({ markets, loaded: { ...this.store.loaded, markets: true } });

        // Connect WebSocket now that markets are loaded
        // This will auto-subscribe to all ticker streams via onOpen handler
        this.publicWebsocket.connectAndSubscribe();

        // Fetch and set tickers in store with loaded flag
        const tickers = await this.fetchTickers();
        this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });
      }

      if (publicDataOnly) {
        this.store.update({
          loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
        });
        this.log('GRVT exchange started in public-only mode', LogSeverity.Info);
        return;
      }

      // Fetch balance and set loaded flag
      // Note: fetchBalance() also auto-detects grvtTradingAccountId if not provided
      try {
        const balance = await this.fetchBalance();
        this.store.update({ balance, loaded: { ...this.store.loaded, balance: true } });
      } catch (err) {
        this.log(`Error fetching balance: ${err}`, LogSeverity.Warning);
        // Set loaded flag even if fetch fails (empty balance)
        this.store.update({ loaded: { ...this.store.loaded, balance: true } });
      }

      // fetchBalance() auto-detects the trading account for ordinary GRVT
      // accounts. Do not query per-account leverage before that detection:
      // doing so silently leaves the placeholder max of 100 in the store.
      // A leverage failure is already contained by updateMarketLeverageLimits,
      // so startup and the balance-loaded safety contract stay intact.
      if (!useExternalPublicData && this.grvtTradingAccountId) {
        const markets = await this.updateMarketLeverageLimits(this.store.markets);
        this.store.update({ markets, loaded: { ...this.store.loaded, markets: true } });
      }

      // Check builder authorization status (uses tradingXhr, so it's fast)
      // This is done after fetchBalance() which auto-detects grvtTradingAccountId
      try {
        await this.fetchBuilderAuthorization();
      } catch (err) {
        this.log(`Builder authorization check failed: ${err}`, LogSeverity.Debug);
      }

      // Connect private WebSocket AFTER fetchBalance() which auto-detects grvtTradingAccountId
      // This ensures the trading account ID is available for subscribing to order/position streams
      if (this.privateWebsocket && !this.privateWebsocket.isConnected) {
        this.log(
          `Connecting private WebSocket with trading account ID: ${this.grvtTradingAccountId}`,
          LogSeverity.Info,
        );
        this.privateWebsocket.connectAndSubscribe();
      }

      // Fetch positions and set loaded flag
      try {
        const positions = await this.fetchPositions();
        this.store.update({ positions, loaded: { ...this.store.loaded, positions: true } });
        this.syncBalanceUpnlFromPositions(positions);
      } catch (err) {
        this.log(`Error fetching positions: ${err}`, LogSeverity.Warning);
        // Set loaded flag even if fetch fails (empty positions)
        this.store.update({ loaded: { ...this.store.loaded, positions: true } });
      }

      // Fetch open orders and set loaded flag
      try {
        const orders = await this.fetchOpenOrders();
        this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
      } catch (err) {
        this.log(`Error fetching orders: ${err}`, LogSeverity.Warning);
        // Set loaded flag even if fetch fails (empty orders)
        this.store.update({ loaded: { ...this.store.loaded, orders: true } });
      }

      // Start balance polling tick
      // This runs every 60s to refresh balance data (deposits, withdrawals, funding)
      // WebSocket handles position P&L updates, but wallet balance needs periodic refresh
      this.tick();

      this.log('GRVT exchange started successfully', LogSeverity.Info);
    } catch (err) {
      this.log(`Error starting exchange: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchMarkets = async (): Promise<Market[]> => {
    try {
      const response = await this.xhr.post<{ r: any[] }>(ENDPOINTS.INSTRUMENTS, {});

      if (!response.data?.r) {
        return [];
      }

      const markets: Market[] = response.data.r.map((inst: any) => {
        const isSpot = false; // GRVT is derivatives only
        const isPerpetual = inst.k === 'PERPETUAL';

        // Lite API format uses shortened field names
        const minSize = parseFloat(inst.ms || '0.001');
        const tickSize = parseFloat(inst.ts || '0.1');

        return {
          id: inst.i, // instrument_id (keep original case for API calls)
          symbol: inst.i.toUpperCase(), // uppercase for internal use
          base: inst.b, // base
          quote: inst.q, // quote
          active: inst.ia !== false, // is_active (assume true if not specified)
          precision: {
            amount: minSize, // min_size as step size (e.g., 0.001)
            price: tickSize, // tick_size (e.g., 0.1)
          },
          limits: {
            amount: {
              min: minSize, // min_size
              max: parseFloat(inst.mp || inst.mxs || '999999999'), // max_position_size or max_size
            },
            leverage: {
              min: 1,
              max: 100, // TODO: Get from API
            },
          },
          isSpot,
          isPerpetual,
          info: {
            ih: inst.ih,
            bd: inst.bd,
            qd: inst.qd,
          },
        };
      });

      this.log(`Fetched ${markets.length} GRVT markets`, LogSeverity.Debug);
      return markets;
    } catch (err) {
      this.log(`Error fetching markets: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  /**
   * GRVT signing metadata is part of the market snapshot. Replace markets even
   * when instrument membership is unchanged so a metadata-only venue refresh
   * cannot leave placeOrder signing with stale ih/bd/qd values.
   */
  refreshMarkets = async (): Promise<{ added: Market[]; removed: Market[] }> => {
    const oldMarkets = new Map(this.store.markets.map((market) => [market.id, market]));
    const newMarkets = await this.fetchMarkets();
    const added = newMarkets.filter((market) => !oldMarkets.has(market.id));
    const removed = Array.from(oldMarkets.values()).filter(
      (market) => !newMarkets.some((newMarket) => newMarket.id === market.id),
    );

    this.store.update({ markets: newMarkets });

    if (added.length > 0 || removed.length > 0) {
      this.emitter.emit('markets:changed', { added, removed });
      this.log(
        `Markets refreshed: +${added.length} -${removed.length} (total: ${newMarkets.length})`,
        LogSeverity.Info,
      );
    }

    return { added, removed };
  };

  private getInstrumentSigningMetadata = (market: Market): GrvtInstrumentSigningMetadata => {
    const info = market.info as Partial<GrvtInstrumentSigningMetadata> | undefined;
    const instrumentHash = info?.ih;
    const baseDecimals = info?.bd;
    const quoteDecimals = info?.qd;
    const hasValidHash =
      typeof instrumentHash === 'string' && /^0x[0-9a-fA-F]+$/.test(instrumentHash) && instrumentHash.length <= 66;

    // GRVT decimal exponents are signed integers: +3 means 0.001, -3 means
    // 1000, and 0 means 1. Do not reject valid negative exponents.
    if (
      !hasValidHash ||
      typeof baseDecimals !== 'number' ||
      !Number.isSafeInteger(baseDecimals) ||
      typeof quoteDecimals !== 'number' ||
      !Number.isSafeInteger(quoteDecimals)
    ) {
      throw new Error(
        `Market ${market.id} does not have valid GRVT signing metadata (ih/bd/qd); refresh markets before placing an order`,
      );
    }

    return { ih: instrumentHash, bd: baseDecimals, qd: quoteDecimals };
  };

  /**
   * Fetch leverage limits from API and update markets
   * Called after authentication to get actual max leverage per instrument
   */
  private updateMarketLeverageLimits = async (markets: Market[]): Promise<Market[]> => {
    try {
      if (!this.grvtClient) {
        return markets;
      }

      // Skip if no trading account ID is provided - API requires a valid sub_account_id
      if (!this.grvtTradingAccountId) {
        this.log('Skipping leverage limits fetch - no trading account ID provided', LogSeverity.Debug);
        return markets;
      }

      // Fetch all leverage settings for the sub-account
      const response = await (this.grvtClient as any).tdgClient.getAllInitialLeverage(
        {
          sub_account_id: this.grvtTradingAccountId,
        },
        await (this.grvtClient as any).authenticatedEndpoint(),
      );

      // Handle both lite format (r) and full format (results)
      const results = response.results || response.r;
      if (!results || results.length === 0) {
        this.log('No leverage data returned from API', LogSeverity.Debug);
        return markets;
      }

      // Create a map of instrument -> max leverage
      const leverageMap = new Map<string, number>();
      for (const item of results) {
        const instrument = (item.instrument || item.i)?.toUpperCase();
        // max_leverage (full) or ml1 (lite)
        const maxLeverage = parseFloat(item.max_leverage || item.ml1 || '100');
        if (instrument && maxLeverage > 0) {
          leverageMap.set(instrument, maxLeverage);
        }
      }

      // Update markets with actual max leverage
      const updatedMarkets = markets.map((market) => {
        const maxLeverage = leverageMap.get(market.id.toUpperCase()) || leverageMap.get(market.symbol);
        if (maxLeverage) {
          return {
            ...market,
            limits: {
              ...market.limits,
              leverage: {
                ...market.limits.leverage,
                max: maxLeverage,
              },
            },
          };
        }
        return market;
      });

      this.log(`Updated leverage limits for ${leverageMap.size} instruments`, LogSeverity.Debug);
      return updatedMarkets;
    } catch (err) {
      this.log(`Error fetching leverage limits: ${err}`, LogSeverity.Warning);
      return markets; // Return original markets if fetch fails
    }
  };

  fetchTickers = async (): Promise<Ticker[]> => {
    try {
      // GRVT doesn't have a single API to fetch all tickers
      // Simulate tickers from markets so screener can work
      // Real values will be populated via WebSocket ticker updates
      const tickers: Ticker[] = this.store.markets.map((market) => ({
        id: market.id,
        symbol: market.symbol,
        bid: 0,
        ask: 0,
        last: 0,
        mark: 0,
        index: 0,
        percentage: 0,
        openInterest: 0,
        fundingRate: 0,
        fundingTime: 0,
        volume: 0,
        quoteVolume: 0,
        info: {},
      }));
      this.log(`Returning ${tickers.length} simulated GRVT tickers`, LogSeverity.Debug);
      return tickers;
    } catch (err) {
      this.log(`Error fetching tickers: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchBalance = async (): Promise<Balance> => {
    try {
      this.log('fetchBalance - START', LogSeverity.Debug);

      const currencies: { [asset: string]: { free: number; used: number; total: number; upnl: number } } = {};
      let totalEquity = 0;
      let availableBalance = 0;
      let unrealizedPnl = 0;

      // Fetch aggregated account summary - this includes both funding and trading accounts
      try {
        this.log('fetchBalance - fetching aggregated account summary', LogSeverity.Debug);
        const response = await this.tradingXhr.post('/lite/v1/aggregated_account_summary', {});
        const aggData = response.data?.r || response.data?.result;

        if (aggData) {
          // Parse spot balances
          const spotBalances = aggData.sb || aggData.spot_balances || [];
          for (const spotBalance of spotBalances) {
            const balance = parseFloat(spotBalance.b || spotBalance.balance || '0');
            const currency = spotBalance.c || spotBalance.currency || 'UNKNOWN';
            if (!currencies[currency]) {
              currencies[currency] = {
                free: 0,
                used: 0,
                total: 0,
                upnl: 0,
              };
            }
            currencies[currency].free += balance;
            currencies[currency].total += balance;
          }

          // Parse total values
          totalEquity = parseFloat(aggData.te || aggData.total_equity || '0');
          // Available balance from aggregated summary
          availableBalance = parseFloat(
            aggData.ts2 ?? aggData.total_sub_account_available_balance ?? aggData.te ?? aggData.total_equity ?? '0',
          );
          const totalUpnlRaw = aggData.tu ?? aggData.total_upnl;
          if (totalUpnlRaw !== undefined && totalUpnlRaw !== null) {
            const parsedUpnl = parseFloat(totalUpnlRaw);
            if (Number.isFinite(parsedUpnl)) {
              unrealizedPnl = parsedUpnl;
            }
          } else {
            unrealizedPnl = this.calculateUpnlFromPositions(this.store.positions);
          }

          // Auto-detect trading account ID from sub-account summaries if not already set
          if (!this.grvtTradingAccountId) {
            const subAccountSummaries = aggData.sas || aggData.sub_account_summaries || [];
            if (subAccountSummaries.length > 0) {
              // Use the first sub-account ID for trading
              const firstSubAccount = subAccountSummaries[0];
              const subAccountId = firstSubAccount.si || firstSubAccount.sub_account_id;
              if (subAccountId) {
                this.grvtTradingAccountId = String(subAccountId);
                this.log(`Auto-detected trading account ID: ${this.grvtTradingAccountId}`, LogSeverity.Info);
              }
            }
          }
        }
        this.log(`fetchBalance - received: equity=${totalEquity}, available=${availableBalance}`, LogSeverity.Debug);
      } catch (err: any) {
        this.log(`Error fetching aggregated account summary: ${err.message}`, LogSeverity.Error);
        throw err;
      }

      return {
        total: totalEquity,
        free: availableBalance,
        used: totalEquity - availableBalance,
        upnl: unrealizedPnl,
        currencies: currencies as any,
      };
    } catch (err) {
      this.log(`Error fetching balance: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  getFakePositions = () => {
    // GRVT is one-way mode only (no hedging), so create one fake position per market.
    // Preserve existing position side to avoid a visible side-flip when a position closes
    // and transitions from real to fake (e.g., Long -> Short flash on chart).
    const existingBySymbol = new Map(this.store.positions.map((p) => [p.symbol, p]));

    const fakePositions = this.store.markets.map((market) => {
      const fetchedLeverage = this.store.fetchedLeverage[market.symbol];
      const leverage =
        fetchedLeverage && !fetchedLeverage.isFetching ? fetchedLeverage.buyLeverage : this.defaultLeverage;

      const existing = existingBySymbol.get(market.symbol);

      return {
        id: createPositionId(market.symbol),
        symbol: market.symbol,
        side: existing?.side ?? PositionSide.Long,
        entryPrice: 0,
        notional: 0,
        leverage,
        unrealizedPnl: 0,
        contracts: 0,
        liquidationPrice: 0,
        positionMode: PositionMode.OneWay,
        marginType: MarginType.Isolated,
      };
    });
    return fakePositions;
  };

  fetchPositions = async (symbols?: string[]): Promise<Position[]> => {
    try {
      if (!this.grvtClient) {
        throw new Error('GRVT SDK client not initialized');
      }

      // Skip if no trading account ID is provided - API requires a valid sub_account_id
      if (!this.grvtTradingAccountId) {
        this.log('Skipping positions fetch - no trading account ID provided', LogSeverity.Debug);
        return this.getFakePositions();
      }

      // Convert uppercased symbols to GRVT format (e.g., BTC_USDT_PERP → BTC_USDT_Perp)
      const instrumentIds = symbols?.map((s) => this.symbolToInstrumentId(s));

      // Use the SDK's TDG client to fetch positions
      const response = await (this.grvtClient as any).tdgClient.positions(
        {
          sub_account_id: this.grvtTradingAccountId,
          ...(instrumentIds && instrumentIds.length > 0 ? { instrument_ids: instrumentIds } : {}),
        },
        await (this.grvtClient as any).authenticatedEndpoint(),
      );

      // Handle both lite format (r) and full format (result)
      const positionsData = response.r || response.result;
      this.log(`GRVT positions response: ${positionsData?.length || 0} rows`, LogSeverity.Debug);

      if (!positionsData || positionsData.length === 0) {
        return this.getFakePositions();
      }

      const positionMode = await this.getPositionMode();
      const positions: Position[] = [];

      for (const grvtPos of positionsData) {
        // Handle both lite and full API formats
        // Lite format uses shortened field names: i, s, ep, up, rp, n, l, el
        // Full format uses: instrument, size, entry_price, unrealized_pnl, realized_pnl, notional, leverage, est_liquidation_price
        const instrument = grvtPos.i || grvtPos.instrument;
        const size = parseFloat(grvtPos.s || grvtPos.size);

        // Skip zero-size positions
        if (size === 0) {
          continue;
        }

        // Lookup market to get uppercased symbol
        const market = this.store.marketsById[instrument];
        if (!market) {
          this.log(`Market not found for instrument ${instrument}`, LogSeverity.Warning);
          continue;
        }

        const side = size > 0 ? PositionSide.Long : PositionSide.Short;
        const contracts = Math.abs(size);
        const entryPrice = parseFloat(grvtPos.ep ?? grvtPos.entry_price ?? '0');
        const unrealizedPnl = parseFloat(grvtPos.up || grvtPos.unrealized_pnl || '0');
        const realizedPnl = parseFloat(grvtPos.rp || grvtPos.realized_pnl || '0');

        // Get leverage from position data, with fallback to fetched leverage or default
        let leverage = parseFloat(grvtPos.l || grvtPos.leverage || '0');
        if (!leverage || leverage === 0 || !isFinite(leverage)) {
          const fetchedLeverage = this.store.fetchedLeverage[market.symbol];
          if (fetchedLeverage && !fetchedLeverage.isFetching) {
            leverage = side === PositionSide.Long ? fetchedLeverage.buyLeverage : fetchedLeverage.sellLeverage;
          } else {
            leverage = this.defaultLeverage; // Fallback to default leverage
          }
        }

        const liquidationPrice = parseFloat(
          grvtPos.el || grvtPos.est_liquidation_price || grvtPos.liquidation_price || '0',
        );
        const notional = Math.abs(parseFloat(grvtPos.n || grvtPos.notional || '0'));

        positions.push({
          id: createPositionId(market.symbol),
          symbol: market.symbol, // Use uppercased market.symbol
          side,
          contracts,
          entryPrice,
          notional,
          leverage,
          unrealizedPnl,
          realizedPnl,
          liquidationPrice,
          marginType: (grvtPos.mt ?? grvtPos.margin_type) === 'CROSS' ? MarginType.Cross : MarginType.Isolated,
          positionMode,
          lastUpdate: virtualClock.getCurrentTimeMs(),
          info: grvtPos,
        });
      }

      // Merge real positions with fake positions for markets without open positions
      // Filter out fake positions for symbols where we already have real positions
      const realSymbols = new Set(positions.map((p) => p.symbol));
      const fakePositions = this.getFakePositions().filter((fp) => !realSymbols.has(fp.symbol));
      const allPositions = [...positions, ...fakePositions];

      this.log(`Fetched ${positions.length} GRVT positions (${allPositions.length} including fake)`, LogSeverity.Debug);
      return allPositions;
    } catch (err) {
      this.log(`Error fetching positions: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchOpenOrders = async (symbol?: string): Promise<Order[]> => {
    try {
      if (!this.grvtClient) {
        throw new Error('GRVT SDK client not initialized');
      }

      // Skip if no trading account ID is provided - API requires a valid sub_account_id
      if (!this.grvtTradingAccountId) {
        this.log('Skipping orders fetch - no trading account ID provided', LogSeverity.Debug);
        return [];
      }

      // Convert uppercased symbol to GRVT format (e.g., BTC_USDT_PERP → BTC_USDT_Perp)
      const instrumentId = symbol ? this.symbolToInstrumentId(symbol) : undefined;

      // Use the SDK's TDG client to fetch open orders
      const response = await (this.grvtClient as any).tdgClient.openOrders(
        {
          sub_account_id: this.grvtTradingAccountId,
          ...(instrumentId ? { instrument_id: instrumentId } : {}),
        },
        await (this.grvtClient as any).authenticatedEndpoint(),
      );

      // Handle both lite format (r) and full format (result)
      const ordersData = response.r || response.result;
      this.log(`GRVT open-orders response: ${ordersData?.length || 0} rows`, LogSeverity.Debug);

      // DEBUG: Log the raw response structure
      this.log(
        `fetchOpenOrders: response keys=${Object.keys(response).join(', ')}, ordersData=${ordersData ? 'array' : 'undefined'}`,
        LogSeverity.Info,
      );

      if (!ordersData) {
        this.log('fetchOpenOrders: no orders data in response', LogSeverity.Warning);
        return [];
      }

      if (ordersData.length === 0) {
        this.log(`fetchOpenOrders: API returned empty orders array`, LogSeverity.Info);
      }

      const orders: Order[] = ordersData.map((o: any) => this.mapOrder(o));

      this.log(`Fetched ${orders.length} open orders`, LogSeverity.Info);
      return orders;
    } catch (err) {
      this.log(`Error fetching open orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  private mapOrder = (o: any): Order => {
    // SDK returns orders with different structures depending on API format
    // Full format: legs, state, metadata, order_id, is_market, reduce_only
    // Lite format: l, s1, m, oi, im, ro

    // Handle both full and lite formats for legs
    const leg = o.legs?.[0] || o.l?.[0] || {};

    // Handle both full and lite formats for state
    // Note: In lite format, 's' is signature, 's1' is state
    const state = o.state || o.s1 || {};

    // Handle both full and lite formats for metadata
    const metadata = o.metadata || o.m || {};

    // Use nonce (client_order_id) as canonical ID — GRVT's oi may be "0x00".
    // Falls back to oi only if no client_order_id is available.
    const clientOrderId = metadata.client_order_id || metadata.co;
    const rawOrderId = o.order_id || o.oi;
    const orderId = clientOrderId ? String(clientOrderId) : rawOrderId;

    // REST snapshots can contain orders placed before this worker started. An
    // absent/invalid upstream creation time stays explicitly unknown so the
    // chaser uses a broad safety query rather than a guessed recent window.
    if (clientOrderId) {
      this.recordNonce(String(clientOrderId), rawOrderId, this.getOrderCreationTimeMs(metadata));
    }

    // Lookup market to get uppercased symbol
    // Handle both full (instrument) and lite (i) formats
    const instrument = leg.instrument || leg.i;
    if (!instrument) {
      this.log('mapOrder: no instrument found in order', LogSeverity.Warning);
    }

    const market = instrument
      ? this.store.marketsById[instrument] ||
        this.store.marketsById[instrument.toUpperCase()] ||
        this.store.markets.find((m) => m.id.toUpperCase() === instrument.toUpperCase())
      : undefined;
    const symbol = market ? market.symbol : instrument?.toUpperCase() || 'UNKNOWN';

    // Handle both full and lite formats for sizes
    // Full: book_size, traded_size | Lite: bs, ts
    const bookSize = parseFloat(state.book_size?.[0] || state.bs?.[0] || '0');
    const tradedSize = parseFloat(state.traded_size?.[0] || state.ts?.[0] || '0');

    // Handle both full and lite formats for is_market and reduce_only
    const isMarket = o.is_market !== undefined ? o.is_market : o.im;
    const reduceOnly = o.reduce_only !== undefined ? o.reduce_only : o.ro;

    // Determine order type - check for trigger metadata first
    // Handle both full (trigger.trigger_type) and lite (t.tt) formats
    const trigger = getGrvtTriggerMetadata(o);
    const type =
      trigger?.type === 'STOP_LOSS'
        ? OrderType.StopLoss
        : trigger?.type === 'TAKE_PROFIT'
          ? OrderType.TakeProfit
          : isMarket
            ? OrderType.Market
            : OrderType.Limit;

    // Handle both full and lite formats for side
    // Full: is_buying_asset | Lite: ib
    const isBuying = leg.is_buying_asset !== undefined ? leg.is_buying_asset : leg.ib;

    // Handle both full and lite formats for price and size
    // Full: limit_price, size | Lite: lp, s
    const limitPrice = parseFloat(leg.limit_price || leg.lp || '0');
    // In safe-cex, a standalone TP/SL order's price is its trigger price.
    const price = trigger?.triggerPrice ?? limitPrice;
    const amount = parseFloat(leg.size || leg.s || '0');

    // Handle both full and lite formats for status
    // Full: status | Lite: s
    const orderStatus = state.status || state.s;

    // Handle both full and lite formats for update_time
    // Full: update_time | Lite: ut
    const updateTime = state.update_time || state.ut;

    return {
      id: orderId,
      ...(clientOrderId ? { clientOrderId: String(clientOrderId) } : {}),
      symbol: symbol,
      type,
      side: isBuying ? OrderSide.Buy : OrderSide.Sell,
      price,
      amount,
      filled: tradedSize,
      remaining: bookSize,
      status: this.mapOrderStatus(orderStatus),
      reduceOnly: reduceOnly || false,
      timeInForce: getGrvtOrderTimeInForce(o),
      ...(trigger?.close !== undefined ? { close: trigger.close } : {}),
      ...(type === OrderType.StopLoss && trigger?.triggerType ? { slTriggerType: trigger.triggerType } : {}),
      ...(type === OrderType.TakeProfit && trigger?.triggerType ? { tpTriggerType: trigger.triggerType } : {}),
      lastUpdate: updateTime ? parseInt(updateTime) / 1000000 : Date.now(),
      info: o,
    };
  };

  private mapOrderStatus = (status: string): OrderStatus => {
    switch (status) {
      case 'PENDING':
        // PENDING orders are actually open orders on GRVT (sent to matching engine)
        return OrderStatus.Open;
      case 'OPEN':
        return OrderStatus.Open;
      case 'FILLED':
        return OrderStatus.Closed;
      case 'CANCELLED':
        return OrderStatus.Canceled;
      case 'REJECTED':
        return OrderStatus.Canceled;
      default:
        return OrderStatus.Open;
    }
  };

  fetchOrders = async (symbols?: string[]): Promise<Order[]> => {
    // Delegate to fetchOpenOrders - GRVT doesn't distinguish between open and all orders in this context
    if (symbols && symbols.length === 1) {
      return this.fetchOpenOrders(symbols[0]);
    }
    return this.fetchOpenOrders();
  };

  placeOrder = async (opts: PlaceOrderOpts, execution?: { isMarket: boolean; limitPrice: number }): Promise<string[]> => {
    const callerClientOrderId = this.resolveCallerClientOrderId(opts);
    if ((opts.type === OrderType.Market || opts.type === OrderType.Limit) && (opts.takeProfit !== undefined || opts.stopLoss !== undefined)) {
      for (const price of [opts.takeProfit, opts.stopLoss]) {
        if (price !== undefined && (!Number.isFinite(price) || price <= 0)) throw new Error('Invalid GRVT bracket trigger price');
      }
      const { takeProfit, stopLoss, tpTriggerType, slTriggerType, ...entry } = opts;
      const ids = await this.placeOrder(entry);
      const side = opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy;
      try {
        for (const [type, price] of [[OrderType.StopLoss, stopLoss], [OrderType.TakeProfit, takeProfit]] as const) {
          if (price === undefined) continue;
          ids.push(...await this.placeOrder({ symbol: opts.symbol, type, side, amount: opts.amount, price,
            triggerPrice: price, reduceOnly: true, close: false, tpTriggerType, slTriggerType }));
        }
      } catch {
        throw new Error(`GRVT bracket placement failed after creating ${ids.join(',')}; reconcile these orders`);
      }
      return ids;
    }
    try {
      // Wallet is required for order signing
      if (!this.wallet) {
        throw new Error('Private key required for order placement - wallet not initialized');
      }

      // Trading account ID - use 0 as default (main account)
      // If not set, we'll try with 0 which should work for the default sub-account
      if (!this.grvtTradingAccountId) {
        this.log('No trading account ID configured, using 0 (default sub-account)', LogSeverity.Warning);
        this.grvtTradingAccountId = '0';
      }

      const market = this.store.marketsBySymbol[opts.symbol];
      if (!market) {
        throw new Error(`Market ${opts.symbol} not found`);
      }

      const {
        ih: instrumentHash,
        bd: baseDecimals,
        qd: quoteDecimals,
      } = this.getInstrumentSigningMetadata(market);

      this.log(
        `Instrument details: hash=${instrumentHash}, baseDecimals=${baseDecimals}, quoteDecimals=${quoteDecimals}`,
        LogSeverity.Debug,
      );

      // Map safe-cex types to GRVT types
      const side: GrvtOrderSide = opts.side === OrderSide.Buy ? 'BUY' : 'SELL';
      const type: GrvtOrderType = execution ? (execution.isMarket ? 'MARKET' : 'LIMIT') : [OrderType.Market, OrderType.StopLoss, OrderType.TakeProfit].includes(opts.type) ? 'MARKET' : 'LIMIT';

      // Map time in force
      let timeInForce: GrvtTimeInForce = 'GOOD_TILL_TIME';
      if (opts.timeInForce === OrderTimeInForce.ImmediateOrCancel) {
        timeInForce = 'IMMEDIATE_OR_CANCEL';
      } else if (opts.timeInForce === OrderTimeInForce.FillOrKill) {
        timeInForce = 'FILL_OR_KILL';
      }

      // market.precision.amount and market.precision.price are step sizes (e.g., 0.001)
      // Round to the nearest multiple of the step size
      const executionPrice = execution?.limitPrice ?? opts.price;
      const price = type === 'LIMIT' && executionPrice ? adjust(executionPrice, market.precision.price) : 0;
      const size = adjust(opts.amount, market.precision.amount);

      this.log(`Precision - price: ${market.precision.price}, amount: ${market.precision.amount}`, LogSeverity.Debug);
      this.log(`Adjusted - price: ${price}, size: ${size} (from ${opts.amount})`, LogSeverity.Debug);

      // Validate size is not zero
      if (size <= 0) {
        throw new Error(
          `Invalid order size: ${size} (original: ${opts.amount}, precision: ${market.precision.amount})`,
        );
      }

      // Generate nonce and expiration for signature (following SDK pattern)
      const nonce = Math.floor(Math.random() * 1e9); // Random nonce up to 1 billion
      const expirationStr = grvtNanoseconds(Date.now() + 3600 * 1000); // 1 hour from now in nanoseconds
      // Keep expiration as string - too large for JS number (exceeds MAX_SAFE_INTEGER)
      const expiration = expirationStr;

      // Get private key with 0x prefix
      const privateKey = this.options.secret?.startsWith('0x') ? this.options.secret : `0x${this.options.secret}`;

      // Prepare EIP-712 domain (following Python SDK)
      // Production uses chainId 325, testnet uses 326
      // Use same logic as API endpoint selection (truthy = testnet, falsy = production)
      const chainId = this.options.testnet ? 326 : 325;
      const domain = {
        ...baseDomain,
        chainId,
      };

      // Tealstreet refuses to place GRVT orders without a valid builder fee, so this
      // throws `GrvtBuilderUnauthorizedError` if the user has not (sufficiently)
      // authorized the builder. Must be fetched BEFORE signing, as builder fee is part
      // of the EIP-712 signed message.
      const builderInfo = await this.getBuilderInfo();
      this.log(`Including builder info: ${builderInfo.builder} (fee: ${builderInfo.builderFee})`, LogSeverity.Debug);

      // Create EIP-712 message for signing (camelCase format matching Order type definition)
      // Following Python SDK implementation EXACTLY:
      // - assetID: Pass instrument_hash as HEX STRING (Python does this!)
      // - contractSize: size * 10^base_decimals
      // - limitPrice: price * PRICE_MULTIPLIER (always 1e9)
      // The signing library (eth-sig-util) will auto-convert hex string to uint256

      const eip712Message: Record<string, any> = {
        subAccountID: grvtUint64(this.grvtTradingAccountId ?? '0'),
        isMarket: type === 'MARKET',
        timeInForce: this.mapTimeInForceToEnum(timeInForce),
        postOnly: opts.timeInForce === OrderTimeInForce.PostOnly,
        reduceOnly: opts.reduceOnly || false,
        legs: [
          {
            assetID: instrumentHash, // HEX STRING! Python SDK passes "0x030501" directly
            contractSize: grvtScaledUint64(size.toString(), baseDecimals),
            limitPrice: type === 'LIMIT' && price > 0 ? grvtScaledUint64(price.toString(), 9) : '0',
            isBuyingContract: side === 'BUY',
          },
        ],
        // Always include builder fields in the signed message — `getBuilderInfo`
        // throws if the user hasn't authorized us, so we never reach this point
        // without a valid `builderInfo`. The EIP-712 type is therefore always
        // "OrderWithBuilderFee" rather than "Order".
        // @see https://github.com/gravity-technologies/builder-examples
        builder: builderInfo.builder,
        builderFee: builderInfo.builderFeeInt, // uint32: percent * 10000 (e.g., "0.03" → 300)
        nonce,
        expiration,
      };

      const orderTypeTemplate = OrderWithBuilderFeeTypes;

      // Prepare typed data for signing
      const typedData = {
        ...orderTypeTemplate,
        domain,
        message: eip712Message,
      };

      // Sign the order using SDK's Signer
      const rawSignature = GrvtSigner.sign(privateKey, typedData as any);
      const { r, s, v } = GrvtSigner.decode(rawSignature);

      this.log(`Placing order: ${opts.symbol} ${side} ${size} @ ${price || 'market'}`, LogSeverity.Info);
      this.log(
        `Order opts: stopLoss=${opts.stopLoss}, takeProfit=${opts.takeProfit}, reduceOnly=${opts.reduceOnly}`,
        LogSeverity.Debug,
      );

      // Use the SDK's TDG client to place order with proper authentication
      if (!this.grvtClient) {
        throw new Error('GRVT SDK client not initialized');
      }

      // Convert to API format (NOT the EIP-712 format - that's only for signing!)
      // API uses regular decimal values as strings, NOT multiplied
      // Generate unique client order ID (simple uint32 like Python SDK) and create time
      const clientOrderId = callerClientOrderId ?? nonce.toString(); // Generated: nonce as client_order_id (uint32)
      const createTime = grvtNanoseconds(Date.now()); // Current time in nanoseconds

      // Prepare trigger for TP/SL if provided
      // This handles both:
      // 1. Standalone stop loss/take profit orders (opts.type === OrderType.StopLoss/TakeProfit)
      // 2. Attached TP/SL to regular orders (opts.stopLoss/opts.takeProfit)
      let trigger: any = undefined;

      // Map trigger type to GRVT format (LAST, MARK, INDEX)
      const getTriggerBy = (triggerType?: OrderStopTriggerType) => {
        if (triggerType === OrderStopTriggerType.MarkPrice) return 'MARK';
        if (triggerType === OrderStopTriggerType.IndexPrice) return 'INDEX';
        return 'LAST'; // Default to LAST
      };

      // Check for standalone stop loss/take profit order types first
      const isStopLossOrder = opts.type === OrderType.StopLoss;
      const isTakeProfitOrder = opts.type === OrderType.TakeProfit;

      if (isStopLossOrder || isTakeProfitOrder) {
        // Standalone SL/TP order - use triggerPrice or price as the trigger
        const triggerPrice = opts.triggerPrice || opts.price;
        if (!triggerPrice) {
          throw new Error(`Trigger price is required for ${opts.type} orders`);
        }

        const triggerType = isStopLossOrder ? 'STOP_LOSS' : 'TAKE_PROFIT';
        const triggerBy = getTriggerBy(isStopLossOrder ? opts.slTriggerType : opts.tpTriggerType);

        this.log(
          `Creating ${triggerType} order with trigger price ${triggerPrice}, trigger by ${triggerBy}`,
          LogSeverity.Debug,
        );

        trigger = {
          trigger_type: triggerType,
          tpsl: {
            trigger_by: triggerBy,
            trigger_price: triggerPrice.toString(),
            close_position: opts.close || false,
            is_split_position: false,
          },
        };
      }

      const fullFormatOrder = {
        sub_account_id: this.grvtTradingAccountId,
        is_market: type === 'MARKET',
        time_in_force: timeInForce,
        post_only: opts.timeInForce === OrderTimeInForce.PostOnly,
        reduce_only: opts.reduceOnly || false,
        legs: [
          {
            instrument: market.id,
            size: size.toString(),
            limit_price: type === 'MARKET' || price === 0 ? undefined : price.toString(),
            is_buying_asset: side === 'BUY',
          },
        ],
        signature: {
          signer: this.wallet.address,
          r: r,
          s: s,
          v: v,
          expiration: expirationStr, // API expects string format
          nonce: nonce,
          chain_id: chainId.toString(),
        },
        metadata: {
          client_order_id: clientOrderId,
          create_time: createTime,
          ...(trigger && { trigger }), // Add trigger if TP/SL is provided
        },
        // Builder info must come after signature and metadata to match GRVT API field ordering
        builder: builderInfo.builder,
        builder_fee: builderInfo.builderFee,
      };

      // Use SDK's TDG client to create order (handles authentication via authenticatedEndpoint)
      const response = await (this.grvtClient as any).tdgClient.createOrder(
        { order: fullFormatOrder },
        await (this.grvtClient as any).authenticatedEndpoint(),
      );

      // Handle error responses - check multiple formats:
      // 1. Lite format at top level: { c: code, m: message, s: status }
      // 2. Full format: { error: { message: '...' } }
      // 3. Lite format nested: { data: { c: code, m: message, s: status } }
      // 4. HTTP error status
      const errorMessage =
        response.m ||
        response.error?.message ||
        response.data?.m ||
        response.data?.message ||
        (response.status >= 400 ? `HTTP ${response.status}` : null);

      if (errorMessage) {
        const errorCode = response.c || response.error?.code || response.data?.c || response.status;
        const fullError = errorCode ? `${errorMessage} (code: ${errorCode})` : errorMessage;
        this.log(`Order creation failed: ${fullError}`, LogSeverity.Error);
        throw new Error(fullError);
      }

      // Check for error in lite format
      if (response.c && response.m) {
        const fullError = `${response.m} (code: ${response.c})`;
        this.log(`Order creation failed: ${fullError}`, LogSeverity.Error);
        throw new Error(fullError);
      }

      // GRVT returns oi="0x00" for new orders — the real order_id is assigned
      // asynchronously. Use client_order_id (the nonce unless the caller supplied
      // one) as the canonical ID: it is consistent across REST, WS and fetches.
      const nonceStr = clientOrderId;
      this.log(`Order created successfully: client_order_id=${nonceStr}`, LogSeverity.Info);

      // Record the locally-created canonical nonce even while GRVT has not
      // assigned a real ID, so later reconciliation reaches its true start.
      const grvtOrderId =
        response.result?.order_id || response.r?.oi || response.order_id || response.data?.result?.order_id;
      this.recordNonce(nonceStr, grvtOrderId, Date.now());

      // Emit builder fee event so the UI can show a toast
      const responseOrder = response.result?.order || response.result || response.r;
      const builderFeeFromResponse = responseOrder?.builder_fee || responseOrder?.bf || builderInfo.builderFee;
      this.emitter.emit('builderFee', {
        orderId: nonceStr,
        builderFee: builderFeeFromResponse,
        builder: builderInfo.builder,
      });

      return [nonceStr];
    } catch (err: any) {
      // Builder authorization errors are typed and consumed by the UI to prompt
      // for re-authorization — preserve the instance instead of wrapping it in a
      // generic Error (which would erase the instanceof check at the call site).
      if (err instanceof GrvtBuilderUnauthorizedError) {
        this.emitAndLogError(`Order failed: ${err.message}`);
        throw err;
      }

      // Extract error message from axios response (SDK converts lite c/m/s to full code/message/status)
      const errorData = err.response?.data;
      const errorMessage = errorData?.message || errorData?.m || err.message;
      const errorCode = errorData?.code || errorData?.c;
      const fullError = errorCode ? `${errorMessage} (code: ${errorCode})` : errorMessage;

      this.emitAndLogError(`Order failed: ${fullError}`);
      throw new Error(fullError);
    }
  };

  cancelOrder = async (order: Order): Promise<void> => {
    try {
      if (!this.grvtClient) {
        throw new Error('GRVT SDK client not initialized');
      }

      // Trading account ID is required for cancellation
      if (!this.grvtTradingAccountId) {
        throw new Error('Trading account ID required to cancel orders');
      }

      // order.id is the nonce (client_order_id). Try the real GRVT order_id
      // from the mapping first, fall back to client_order_id for cancel.
      const realOrderId = this._nonceToOrderId.get(order.id);
      const cancelPayload = realOrderId
        ? { sub_account_id: this.grvtTradingAccountId, order_id: realOrderId }
        : { sub_account_id: this.grvtTradingAccountId, client_order_id: order.id };

      this.log(`Cancelling order: storeId=${order.id} realId=${realOrderId || 'none'}`, LogSeverity.Info);

      // Use the SDK's TDG client to cancel order
      await (this.grvtClient as any).tdgClient.cancelOrder(
        cancelPayload,
        await (this.grvtClient as any).authenticatedEndpoint(),
      );

      this.log(`Order ${order.id} cancelled successfully`, LogSeverity.Info);
    } catch (err: any) {
      const errorData = err.response?.data;
      const errorMessage = errorData?.message || errorData?.m || err.message;
      const errorCode = errorData?.code || errorData?.c;
      const fullError = errorCode ? `${errorMessage} (code: ${errorCode})` : errorMessage;

      this.emitAndLogError(`Cancel failed: ${fullError}`);
      throw new Error(fullError);
    }
  };

  cancelOrders = async (orders: Order[]): Promise<void> => {
    // GRVT doesn't have a batch cancel endpoint, so we cancel sequentially
    for (const order of orders) {
      try {
        await this.cancelOrder(order);
      } catch (_err) {
        this.log(`Failed to cancel order ${order.id}, continuing...`, LogSeverity.Warning);
      }
    }
  };

  cancelAllOrders = async (): Promise<void> => {
    try {
      if (!this.grvtClient) {
        throw new Error('GRVT SDK client not initialized');
      }

      // Trading account ID is required for cancellation
      if (!this.grvtTradingAccountId) {
        throw new Error('Trading account ID required to cancel orders');
      }

      // Use the SDK's TDG client to cancel all orders
      await (this.grvtClient as any).tdgClient.cancelAllOrders(
        {
          sub_account_id: this.grvtTradingAccountId,
        },
        await (this.grvtClient as any).authenticatedEndpoint(),
      );

      this.log('All orders cancelled successfully', LogSeverity.Info);
    } catch (err: any) {
      const errorData = err.response?.data;
      const errorMessage = errorData?.message || errorData?.m || err.message;
      const errorCode = errorData?.code || errorData?.c;
      const fullError = errorCode ? `${errorMessage} (code: ${errorCode})` : errorMessage;

      this.emitAndLogError(`Cancel all failed: ${fullError}`);
      throw new Error(fullError);
    }
  };

  cancelSymbolOrders = async (symbol: string): Promise<void> => {
    try {
      if (!this.grvtClient) {
        throw new Error('GRVT SDK client not initialized');
      }

      // Trading account ID is required for cancellation
      if (!this.grvtTradingAccountId) {
        throw new Error('Trading account ID required to cancel orders');
      }

      const market = this.store.marketsBySymbol[symbol];
      if (!market) throw new Error('Unknown GRVT cancellation market');
      const account = grvtUint64(this.grvtTradingAccountId);
      const client = this.grvtClient as unknown as {
        authenticatedEndpoint(): Promise<AxiosRequestConfig>;
      };
      let accountIdentity = this.apiExtras?.accountId;
      const ensureActive = () => {
        if (this.isDisposed) throw new Error('GRVT exchange disposed during symbol cancellation');
        if (this.grvtTradingAccountId !== account ||
            (accountIdentity !== undefined && this.apiExtras?.accountId !== accountIdentity)) {
          throw new Error('GRVT cancellation account changed');
        }
      };
      ensureActive();
      const orderVersion = this.store.updateIds.orders;
      const nativeAtStart = new Map(this._nonceToOrderId);
      const createdAtStart = new Map(this._nonceCreationTimes);
      const ensureClientIdentity = (id: string | undefined) => {
        if (id === undefined) return;
        if (
          this.store.updateIds.orders !== orderVersion ||
          this._nonceToOrderId.get(id) !== nativeAtStart.get(id) ||
          this._nonceCreationTimes.get(id) !== createdAtStart.get(id)
        ) throw new Error('GRVT client cancellation identity changed during snapshot or authentication');
      };
      const readConfig = await client.authenticatedEndpoint();
      ensureActive();
      accountIdentity ??= this.apiExtras?.accountId;
      // open_orders is a complete snapshot, without instrument/paging filters.
      // Keep its raw envelope: the SDK schema mapper discards unknown field names.
      const response = await this.tradingXhr.post<unknown>(ENDPOINTS.OPEN_ORDERS, { sa: account }, readConfig);
      ensureActive();
      const malformed = () => new Error('Incomplete or malformed GRVT cancellation snapshot');
      const object = (value: unknown): Record<string, unknown> => {
        if (!value || typeof value !== 'object' || Array.isArray(value)) throw malformed();
        return value as Record<string, unknown>;
      };
      const field = (row: Record<string, unknown>, full: string, lite: string): unknown => {
        if (full in row && lite in row) throw malformed();
        return full in row ? row[full] : row[lite];
      };
      const envelope = object(response.data);
      if (
        response.status !== 200 ||
        Object.keys(envelope).some((key) => !['r', 'result', 'ri', 'request_id'].includes(key))
      ) {
        throw malformed();
      }
      const rows = field(envelope, 'result', 'r');
      if (!Array.isArray(rows)) throw malformed();
      const payloads: IApiCancelOrderRequest[] = [];
      const nativeIds = new Set<string>();
      const clientIds = new Set<string>();
      for (const value of rows) {
        const row = object(value);
        if (['e', 'error', 'c', 'code'].some((key) => key in row)) throw malformed();
        if (field(row, 'sub_account_id', 'sa') !== account) throw malformed();
        const legs = field(row, 'legs', 'l');
        if (!Array.isArray(legs) || legs.length !== 1) throw malformed();
        const instruments = legs.map((leg) => field(object(leg), 'instrument', 'i'));
        if (instruments.some((instrument) => typeof instrument !== 'string' || !instrument)) throw malformed();
        const matches = instruments.includes(market.id);
        const rawNative = field(row, 'order_id', 'oi');
        if (
          rawNative !== undefined &&
          (typeof rawNative !== 'string' || !/^(?:0|0x[0-9a-f]{1,32})$/i.test(rawNative))
        ) {
          throw malformed();
        }
        const native = typeof rawNative === 'string' && !/^(?:0|0x0+)$/i.test(rawNative) ? rawNative : undefined;
        const nativeKey = native ? BigInt(native).toString() : undefined;
        const metadata = field(row, 'metadata', 'm');
        const rawClient = metadata === undefined ? undefined : field(object(metadata), 'client_order_id', 'co');
        let clientId: string | undefined;
        if (rawClient !== undefined) {
          if (typeof rawClient !== 'string') throw malformed();
          const parsed = grvtUint64(rawClient);
          clientId = parsed === '0' ? undefined : parsed;
        }
        if (!native && clientId === undefined) throw malformed();
        if ((nativeKey && nativeIds.has(nativeKey)) || (clientId !== undefined && clientIds.has(clientId))) {
          throw malformed();
        }
        if (nativeKey) nativeIds.add(nativeKey);
        if (clientId !== undefined) clientIds.add(clientId);
        if (matches) {
          payloads.push({
            sub_account_id: account,
            ...(native ? { order_id: native } : { client_order_id: clientId! }),
          });
        }
      }
      // Validate every row and fallback lifecycle before the first write.
      for (const payload of payloads) ensureClientIdentity(payload.client_order_id);
      for (const payload of payloads) {
        ensureActive();
        ensureClientIdentity(payload.client_order_id);
        const config = await client.authenticatedEndpoint();
        ensureActive();
        ensureClientIdentity(payload.client_order_id);
        const writeConfig = {
          ...config,
          grvtCancellationScope: { subAccountId: account, accountId: accountIdentity },
          ...(payload.client_order_id !== undefined ? { grvtDisableReauthRetry: true } : {}),
        };
        const acknowledgement = await this.tradingXhr.post<unknown>(
          ENDPOINTS.CANCEL_ORDER,
          {
            sa: account,
            ...(payload.order_id !== undefined ? { oi: payload.order_id } : { co: payload.client_order_id }),
          },
          writeConfig,
        );
        const invalidAck = () => new Error('Invalid GRVT cancellation acknowledgement');
        const ackEnvelope = object(acknowledgement.data);
        if (
          acknowledgement.status !== 200 ||
          Object.keys(ackEnvelope).some((key) => !['r', 'result', 'ri', 'request_id'].includes(key))
        ) throw invalidAck();
        const ack = object(field(ackEnvelope, 'result', 'r'));
        if (['e', 'error', 'c', 'code'].some((key) => key in ack)) throw invalidAck();
        if ('ack' in ack || 'a' in ack) {
          const accepted = field(ack, 'ack', 'a');
          if (
            Object.keys(ack).some((key) => !['a', 'ack'].includes(key)) ||
            (accepted !== true && accepted !== 'true')
          ) {
            throw invalidAck();
          }
        } else {
          // Official cancel_order may return the cancelled Order instead of Ack.
          const ackLegs = field(ack, 'legs', 'l');
          if (
            field(ack, 'sub_account_id', 'sa') !== account ||
            !Array.isArray(ackLegs) || ackLegs.length !== 1 ||
            field(object(ackLegs[0]), 'instrument', 'i') !== market.id
          ) throw invalidAck();
          if (payload.order_id !== undefined) {
            const native = field(ack, 'order_id', 'oi');
            if (
              typeof native !== 'string' || !/^0x[0-9a-f]{1,32}$/i.test(native) ||
              BigInt(native) !== BigInt(payload.order_id)
            ) throw invalidAck();
          } else {
            const metadata = object(field(ack, 'metadata', 'm'));
            if (field(metadata, 'client_order_id', 'co') !== payload.client_order_id) throw invalidAck();
          }
        }
      }
      ensureActive();

      this.log(`All orders for ${symbol} cancelled successfully`, LogSeverity.Info);
    } catch (err: any) {
      const errorData = err.response?.data;
      const errorMessage = errorData?.message || errorData?.m || err.message;
      const errorCode = errorData?.code || errorData?.c;
      const fullError = errorCode ? `${errorMessage} (code: ${errorCode})` : errorMessage;

      this.emitAndLogError(`Cancel ${symbol} orders failed: ${fullError}`);
      throw new Error(fullError);
    }
  };

  placeOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    this.assertCallerClientOrderIds(orders);
    // GRVT doesn't have batch order placement, so we place sequentially
    const orderIds: string[] = [];
    for (const order of orders) {
      try {
        const ids = await this.placeOrder(order);
        orderIds.push(...ids);
      } catch (err) {
        // Builder authorization errors are user-actionable — bubble up so the UI
        // can prompt the user to re-authorize. There's no point in continuing the
        // batch since every subsequent order would fail the same way.
        if (err instanceof GrvtBuilderUnauthorizedError) {
          throw err;
        }
        this.log(`Failed to place order for ${order.symbol}, continuing...`, LogSeverity.Warning);
      }
    }
    return orderIds;
  };

  updateOrder = async (opts: UpdateOrderOpts): Promise<string[]> => {
    try {
      const { order, update } = opts;

      this.log(`Updating order ${order.id} (fieldCount=${Object.keys(update).length})`, LogSeverity.Info);

      // Build and validate the full replacement before cancelling. A GRVT
      // cancel-and-replace cannot restore an attribute the upstream payload did
      // not expose, so fail closed while the original order is still live.
      if (!order.timeInForce) {
        throw new Error(`Cannot safely update GRVT order ${order.id}: missing time-in-force`);
      }

      let newOrderOpts: PlaceOrderOpts = {
        symbol: order.symbol,
        type: order.type,
        side: order.side,
        amount: ('amount' in update ? update.amount : order.amount) - order.filled,
        price: 'price' in update ? update.price : order.price,
        reduceOnly: order.reduceOnly,
        timeInForce: order.timeInForce,
      };

      let replacementExecution: { isMarket: boolean; limitPrice: number } | undefined;
      if (!Number.isFinite(newOrderOpts.amount) || newOrderOpts.amount <= 0) throw new Error('GRVT replacement has no remaining quantity');
      if (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit) {
        const info = order.info;
        const legs = info?.legs ?? info?.l;
        const isMarket = info?.is_market ?? info?.im ?? false;
        const limitPrice = Number(legs?.[0]?.limit_price ?? legs?.[0]?.lp ?? 0);
        if (typeof isMarket !== 'boolean' || !Array.isArray(legs) || legs.length !== 1 || !Number.isFinite(limitPrice)
          || (!isMarket && limitPrice <= 0) || (isMarket && order.timeInForce === OrderTimeInForce.PostOnly)) {
          throw new Error('Cannot safely update GRVT order: invalid native execution semantics');
        }
        replacementExecution = { isMarket, limitPrice: isMarket ? 0 : limitPrice };
        const trigger = getGrvtTriggerMetadata(order.info);
        if (!trigger || trigger.type !== (order.type === OrderType.StopLoss ? 'STOP_LOSS' : 'TAKE_PROFIT')) {
          throw new Error(`Cannot safely update GRVT order ${order.id}: missing trigger metadata`);
        }
        if (!Number.isFinite(trigger.triggerPrice) || trigger.triggerPrice! <= 0) {
          throw new Error(`Cannot safely update GRVT order ${order.id}: missing trigger price`);
        }
        if (!trigger.triggerType || trigger.close === undefined || trigger.isSplitPosition === undefined) {
          throw new Error(`Cannot safely update GRVT order ${order.id}: incomplete trigger metadata`);
        }
        // PlaceOrderOpts has no split-position field, so a cancel-and-replace
        // would otherwise turn a split TP/SL into a non-split trigger.
        if (trigger.isSplitPosition) {
          throw new Error(`Cannot safely update GRVT order ${order.id}: split-position trigger is not supported`);
        }

        const triggerPrice = 'price' in update ? update.price : trigger.triggerPrice;
        newOrderOpts = {
          ...newOrderOpts,
          price: triggerPrice,
          triggerPrice,
          close: trigger.close,
          ...(order.type === OrderType.StopLoss
            ? { slTriggerType: trigger.triggerType }
            : { tpTriggerType: trigger.triggerType }),
        };
      }

      const market = this.store.marketsBySymbol[newOrderOpts.symbol];
      if (!market) throw new Error('Unknown GRVT replacement market');
      const { bd: baseDecimals } = this.getInstrumentSigningMetadata(market);
      const replacementSize = adjust(newOrderOpts.amount, market.precision.amount);
      if (!Number.isFinite(replacementSize) || replacementSize <= 0 || replacementSize < market.limits.amount.min || (market.limits.amount.max > 0 && replacementSize > market.limits.amount.max)) {
        throw new Error('GRVT replacement amount is outside market precision or limits');
      }
      grvtScaledUint64(replacementSize.toString(), baseDecimals);
      const replacementIsMarket = replacementExecution?.isMarket ?? (newOrderOpts.type === OrderType.Market || newOrderOpts.type === OrderType.StopLoss || newOrderOpts.type === OrderType.TakeProfit);
      if (!replacementIsMarket) {
        const replacementPrice = adjust(replacementExecution?.limitPrice ?? newOrderOpts.price ?? 0, market.precision.price);
        if (!Number.isFinite(replacementPrice) || replacementPrice <= 0) throw new Error('Invalid GRVT replacement limit price');
        grvtScaledUint64(replacementPrice.toString(), 9);
      }
      if (newOrderOpts.triggerPrice !== undefined) {
        if (!Number.isFinite(newOrderOpts.triggerPrice) || newOrderOpts.triggerPrice <= 0) throw new Error('Invalid GRVT replacement trigger price');
        grvtScaledUint64(newOrderOpts.triggerPrice.toString(), 9);
      }

      // Step 1: Cancel the existing order only after the replacement is safe.
      await this.cancelOrder(order);
      this.log(`Order ${order.id} cancelled for update`, LogSeverity.Debug);

      // Step 2: Create the semantically equivalent replacement.
      const newOrderIds = replacementExecution ? await this.placeOrder(newOrderOpts, replacementExecution) : await this.placeOrder(newOrderOpts);
      this.log(
        `Order updated successfully: cancelled ${order.id}, created ${newOrderIds.join(', ')}`,
        LogSeverity.Info,
      );

      return newOrderIds;
    } catch (err: any) {
      const errorData = err.response?.data;
      const errorMessage = errorData?.message || errorData?.m || err.message;
      const errorCode = errorData?.code || errorData?.c;
      const fullError = errorCode ? `${errorMessage} (code: ${errorCode})` : errorMessage;

      this.emitAndLogError(`Update order failed: ${fullError}`);
      throw new Error(fullError);
    }
  };

  /**
   * Convert safe-cex interval to GRVT CandlestickInterval
   */
  private convertInterval(interval: string): GrvtCandlestickInterval {
    const mapping: Record<string, GrvtCandlestickInterval> = {
      '1m': GrvtCandlestickInterval.CI_1_M,
      '3m': GrvtCandlestickInterval.CI_3_M,
      '5m': GrvtCandlestickInterval.CI_5_M,
      '15m': GrvtCandlestickInterval.CI_15_M,
      '30m': GrvtCandlestickInterval.CI_30_M,
      '1h': GrvtCandlestickInterval.CI_1_H,
      '2h': GrvtCandlestickInterval.CI_2_H,
      '4h': GrvtCandlestickInterval.CI_4_H,
      '6h': GrvtCandlestickInterval.CI_6_H,
      '12h': GrvtCandlestickInterval.CI_12_H,
      '1d': GrvtCandlestickInterval.CI_1_D,
      '1w': GrvtCandlestickInterval.CI_1_W,
    };

    return mapping[interval] || GrvtCandlestickInterval.CI_1_M;
  }

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    try {
      const market = this.store.marketsBySymbol[opts.symbol];

      if (!market) {
        this.log(`Market ${opts.symbol} not found`, LogSeverity.Warning);
        return [];
      }

      const interval = this.convertInterval(opts.interval);
      // Use lite API format field names
      const requestBody: any = {
        i: market.id, // instrument (lite format)
        i1: interval, // interval (lite format)
        t: 'TRADE', // type: TRADE or MARK (default to TRADE for trading volume)
      };

      if (opts.from) {
        // Convert milliseconds to nanoseconds (GRVT uses nanosecond timestamps)
        requestBody.st = (opts.from * 1000000).toString(); // start_time (lite format)
      }
      if (opts.to) {
        // Convert milliseconds to nanoseconds
        requestBody.et = (opts.to * 1000000).toString(); // end_time (lite format)
      }
      if (opts.limit) {
        requestBody.l = opts.limit; // limit (lite format)
      }

      this.log(`Fetching OHLCV for ${opts.symbol} with interval ${interval}`, LogSeverity.Debug);

      const response = await this.xhr.post<{ r: GrvtKline[] }>(ENDPOINTS.KLINES, requestBody);

      if (!response.data?.r) {
        this.log('OHLCV API returned invalid data', LogSeverity.Warning);
        return [];
      }

      const candles: Candle[] = response.data.r.map((k: any) => {
        // Handle both lite and full formats
        // Lite format uses: ot (open_time), ct (close_time), o (open), c (close), h (high), l (low), vb (volume_b)
        // Full format uses: open_time, close_time, open, close, high, low, volume_b
        const timestamp = k.ot || k.open_time;
        const timestampSeconds =
          typeof timestamp === 'string'
            ? parseInt(timestamp) / 1000000000 // Convert nanoseconds to seconds
            : timestamp / 1000; // If already in milliseconds, convert to seconds

        return {
          timestamp: timestampSeconds,
          open: parseFloat(k.o || k.open || '0'),
          high: parseFloat(k.h || k.high || '0'),
          low: parseFloat(k.l || k.low || '0'),
          close: parseFloat(k.c || k.close || '0'),
          volume: parseFloat(k.vb || k.volume_b || k.volume || '0'),
        };
      });

      // GRVT returns candles in reverse chronological order (newest first)
      // Reverse to match standard chronological order (oldest first)
      candles.reverse();

      this.log(`Fetched ${candles.length} candles for ${opts.symbol}`, LogSeverity.Info);
      return candles;
    } catch (err) {
      this.log(`Error fetching OHLCV: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  fetchOrderBook = async (symbol: string): Promise<OrderBook> => {
    try {
      const market = this.store.marketsBySymbol[symbol];

      if (!market) {
        this.log(`Market ${symbol} not found`, LogSeverity.Warning);
        return { bids: [], asks: [] };
      }

      const requestBody = {
        instrument_id: market.id,
      };

      this.log(`Fetching orderbook for ${symbol}`, LogSeverity.Debug);

      const response = await this.xhr.post<{ r: any }>(ENDPOINTS.ORDERBOOK, requestBody);

      if (!response.data?.r) {
        this.log('Orderbook API returned invalid data', LogSeverity.Warning);
        return { bids: [], asks: [] };
      }

      const data = response.data.r;

      // Parse bids and asks from GRVT format to safe-cex format
      const bids = (data.bids || []).map((bid: any, index: number, arr: any[]) => {
        const price = parseFloat(bid.price);
        const amount = parseFloat(bid.size);
        // Calculate cumulative total (sum of all amounts up to this level)
        let total = amount;
        for (let i = 0; i < index; i++) {
          total += parseFloat(arr[i].size);
        }
        return { price, amount, total };
      });

      const asks = (data.asks || []).map((ask: any, index: number, arr: any[]) => {
        const price = parseFloat(ask.price);
        const amount = parseFloat(ask.size);
        // Calculate cumulative total (sum of all amounts up to this level)
        let total = amount;
        for (let i = 0; i < index; i++) {
          total += parseFloat(arr[i].size);
        }
        return { price, amount, total };
      });

      this.log(`Fetched orderbook for ${symbol} - Bids: ${bids.length}, Asks: ${asks.length}`, LogSeverity.Info);
      return { bids, asks };
    } catch (err) {
      this.log(`Error fetching orderbook: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  listenOrderBook = (symbol: string, callback: (orderbook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  /**
   * Fetch leverage for a specific symbol
   * Uses SDK's getAllInitialLeverage and returns the leverage for the specified symbol
   */
  fetchLeverageForSymbol = async (symbol: string): Promise<FetchedLeverage | null> => {
    try {
      if (!this.grvtClient) {
        throw new Error('GRVT SDK client not initialized');
      }

      const market = this.store.marketsBySymbol[symbol];
      if (!market) {
        this.log(`Market ${symbol} not found`, LogSeverity.Warning);
        return null;
      }

      // Skip if no trading account ID is provided - API requires a valid sub_account_id
      if (!this.grvtTradingAccountId) {
        this.log(`Skipping leverage fetch for ${symbol} - no trading account ID provided`, LogSeverity.Debug);

        // Return default leverage when no trading account ID
        const defaultLeverageObj: FetchedLeverage = {
          buyLeverage: this.defaultLeverage,
          sellLeverage: this.defaultLeverage,
        };

        this.store.update({
          fetchedLeverage: {
            ...this.store.fetchedLeverage,
            [symbol]: defaultLeverageObj,
          },
        });

        return defaultLeverageObj;
      }

      // Fetch all leverage settings for the sub-account
      const response = await (this.grvtClient as any).tdgClient.getAllInitialLeverage(
        {
          sub_account_id: this.grvtTradingAccountId,
        },
        await (this.grvtClient as any).authenticatedEndpoint(),
      );

      // Handle both lite format (r) and full format (results)
      const results = response.results || response.r;
      if (!results || results.length === 0) {
        this.log(`No leverage data found for ${symbol}, using default ${this.defaultLeverage}`, LogSeverity.Debug);

        // Return default leverage when no data from API
        const defaultLeverageObj: FetchedLeverage = {
          buyLeverage: this.defaultLeverage,
          sellLeverage: this.defaultLeverage,
        };

        this.store.update({
          fetchedLeverage: {
            ...this.store.fetchedLeverage,
            [symbol]: defaultLeverageObj,
          },
        });

        return defaultLeverageObj;
      }

      // Find leverage for the specific instrument
      // Need to match case-insensitively since our symbol is uppercase
      const leverageData = results.find(
        (r: any) => (r.instrument || r.i)?.toUpperCase() === symbol || (r.instrument || r.i) === market.id,
      );

      if (!leverageData) {
        this.log(
          `No leverage data for instrument ${market.id}, using default ${this.defaultLeverage}`,
          LogSeverity.Debug,
        );

        // Return default leverage when no data for specific instrument
        const defaultLeverageObj: FetchedLeverage = {
          buyLeverage: this.defaultLeverage,
          sellLeverage: this.defaultLeverage,
        };

        this.store.update({
          fetchedLeverage: {
            ...this.store.fetchedLeverage,
            [symbol]: defaultLeverageObj,
          },
        });

        return defaultLeverageObj;
      }

      const leverage = parseFloat(leverageData.leverage || leverageData.l || '1');

      this.log(`Fetched leverage for ${symbol}: ${leverage}`, LogSeverity.Debug);

      const leverageObj: FetchedLeverage = {
        buyLeverage: leverage,
        sellLeverage: leverage,
      };

      // Store the fetched leverage in the store for UI access
      this.store.update({
        fetchedLeverage: {
          ...this.store.fetchedLeverage,
          [symbol]: leverageObj,
        },
      });

      return leverageObj;
    } catch (err: any) {
      this.log(
        `Error fetching leverage for ${symbol}: ${err.message}, using default ${this.defaultLeverage}`,
        LogSeverity.Warning,
      );

      // Return default leverage on error
      const defaultLeverageObj: FetchedLeverage = {
        buyLeverage: this.defaultLeverage,
        sellLeverage: this.defaultLeverage,
      };

      this.store.update({
        fetchedLeverage: {
          ...this.store.fetchedLeverage,
          [symbol]: defaultLeverageObj,
        },
      });

      return defaultLeverageObj;
    }
  };

  /**
   * Set leverage for a specific symbol
   * Uses SDK's setInitialLeverage method
   */
  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide): Promise<void> => {
    try {
      if (!this.grvtClient) {
        throw new Error('GRVT SDK client not initialized');
      }

      // Require trading account ID for leverage operations
      if (!this.grvtTradingAccountId) {
        throw new Error('Trading account ID required to set leverage');
      }

      const market = this.store.marketsBySymbol[symbol];
      if (!market) {
        throw new Error(`Market ${symbol} not found`);
      }

      // Clamp leverage to market limits
      const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

      this.log(`Setting leverage for ${symbol} to ${leverage}`, LogSeverity.Info);

      // Use SDK's setInitialLeverage method
      const response = await (this.grvtClient as any).tdgClient.setInitialLeverage(
        {
          sub_account_id: this.grvtTradingAccountId,
          instrument: market.id, // Use original case for API
          leverage: leverage.toString(),
        },
        await (this.grvtClient as any).authenticatedEndpoint(),
      );

      // Check for success
      const success = response.success || response.s;
      if (!success) {
        const errorMsg = response.error?.message || response.e?.m || 'Failed to set leverage';
        throw new Error(errorMsg);
      }

      this.log(`Leverage set successfully for ${symbol}: ${leverage}`, LogSeverity.Info);

      // Update the fetchedLeverage cache
      this.store.update({
        fetchedLeverage: {
          ...this.store.fetchedLeverage,
          [symbol]: {
            buyLeverage: leverage,
            sellLeverage: leverage,
          },
        },
      });

      // Update the position's leverage in the store
      const position = this.store.positions.find((p) => p.symbol === symbol);
      if (position) {
        this.store.updatePosition(position, { leverage });
      }
    } catch (err: any) {
      this.log(`Error setting leverage for ${symbol}: ${err.message}`, LogSeverity.Error);
      throw err;
    }
  };

  /**
   * Fetch fills for specific order IDs from REST API.
   * Used by chaser to verify order completion before placing new orders.
   * @param orderIds Array of order IDs to fetch fills for
   * @returns Map of order ID to total filled amount
   */
  fetchFillsForOrderIds = async (orderIds: string[]): Promise<Map<string, number>> => {
    const fillsByOrderId = new Map<string, number>();

    if (orderIds.length === 0) {
      return fillsByOrderId;
    }

    try {
      const subAccountId = this.grvtTradingAccountId || this.apiExtras?.subAccountId;
      if (!subAccountId) {
        return fillsByOrderId;
      }

      const requestedOrderIds = new Set(orderIds.map((orderId) => String(orderId)));
      const creationTimes = [...requestedOrderIds].map((orderId) => this._nonceCreationTimes.get(orderId));
      // A guessed recent start can omit fills. Unknown timestamps deliberately
      // use the broad start and rely on the finite page cap to fail closed.
      const oldestCreationTime = creationTimes.every((time) => time !== undefined)
        ? Math.min(...(creationTimes as number[]))
        : undefined;
      const startTimeMs =
        oldestCreationTime === undefined
          ? 0
          : Math.max(0, oldestCreationTime - GrvtExchange.CHASER_FILL_CLOCK_SKEW_MS);
      const nowNs = String(Date.now() * 1_000_000);
      const startTimeNs = String(startTimeMs * 1_000_000);

      const requestedNonceByOrderId = new Map<string, string>();
      for (const requestedOrderId of requestedOrderIds) {
        const realOrderId = this._nonceToOrderId.get(requestedOrderId);
        if (realOrderId) requestedNonceByOrderId.set(String(realOrderId), requestedOrderId);
      }

      let cursor: string | undefined;
      const seenCursors = new Set<string>();

      for (let page = 0; page < GrvtExchange.MAX_CHASER_FILL_PAGES; page++) {
        const { data } = await this.tradingXhr.post(ENDPOINTS.FILL_HISTORY, {
          sa: subAccountId,
          st: startTimeNs,
          et: nowNs,
          l: GrvtExchange.CHASER_FILL_PAGE_SIZE,
          ...(cursor ? { c: cursor } : {}),
        });
        if (!data || typeof data !== 'object') {
          throw new Error(`GRVT fill history page ${page + 1} has an invalid response shape`);
        }

        const fills = data.r ?? data.result;
        if (!Array.isArray(fills)) {
          throw new Error(`GRVT fill history page ${page + 1} has no fill array`);
        }

        // Client IDs are GRVT's canonical identity for Tealstreet orders. Only
        // fall back to oi/order_id when the fill did not carry one: treating a
        // contradictory client ID as a mapping hit could credit another chaser.
        for (const fill of fills) {
          const metadata = fill.metadata ?? fill.m;
          const clientOrderIds = new Set(
            [fill.co, fill.client_order_id, metadata?.co, metadata?.client_order_id]
              .filter((id) => id !== undefined && id !== null && id !== '')
              .map((id) => String(id)),
          );
          let orderId: string | undefined;

          if (clientOrderIds.size === 1) {
            const [clientOrderId] = clientOrderIds;
            if (requestedOrderIds.has(clientOrderId)) orderId = clientOrderId;
          } else if (clientOrderIds.size === 0) {
            const matchingOrderIds = new Set<string>();
            for (const realOrderId of [fill.oi, fill.order_id]) {
              if (realOrderId === undefined || realOrderId === null || realOrderId === '') continue;
              const normalizedOrderId = String(realOrderId);
              if (requestedOrderIds.has(normalizedOrderId)) matchingOrderIds.add(normalizedOrderId);
              const requestedNonce = requestedNonceByOrderId.get(normalizedOrderId);
              if (requestedNonce) matchingOrderIds.add(requestedNonce);
            }
            if (matchingOrderIds.size === 1) orderId = [...matchingOrderIds][0];
          }

          if (!orderId) continue;

          const rawAmount = fill.s ?? fill.size;
          const amount =
            typeof rawAmount === 'number'
              ? rawAmount
              : typeof rawAmount === 'string' && /^(?:\d+(?:\.\d+)?|\.\d+)$/.test(rawAmount)
                ? Number(rawAmount)
                : Number.NaN;
          if (!Number.isFinite(amount) || amount <= 0) continue;

          fillsByOrderId.set(orderId, (fillsByOrderId.get(orderId) || 0) + amount);
        }

        const hasN = Object.prototype.hasOwnProperty.call(data, 'n');
        const hasNext = Object.prototype.hasOwnProperty.call(data, 'next');
        if (!hasN && !hasNext) {
          throw new Error(`GRVT fill history page ${page + 1} is missing its cursor`);
        }
        const rawNext = hasN ? data.n : data.next;
        if (typeof rawNext !== 'string') {
          throw new Error(`GRVT fill history page ${page + 1} has an invalid cursor`);
        }
        if (!rawNext) return fillsByOrderId;
        if (seenCursors.has(rawNext)) {
          throw new Error(`GRVT fill history pagination repeated cursor on page ${page + 1}`);
        }
        seenCursors.add(rawNext);
        cursor = rawNext;
      }

      throw new Error(`GRVT fill history exceeded ${GrvtExchange.MAX_CHASER_FILL_PAGES} pages`);
    } catch (e) {
      this.log(`fetchFillsForOrderIds error: ${e}`, LogSeverity.Warning);
      throw e;
    }
  };
}
