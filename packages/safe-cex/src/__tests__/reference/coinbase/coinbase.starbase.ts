/**
 * Inert protocol foundation for Coinbase's Deribit-powered Starbase gateway.
 *
 * Nothing in the active Coinbase exchange imports this module yet. The future
 * cutover must explicitly wire transport, state mapping, and order lifecycle
 * after the remaining migration gates are satisfied.
 */

export const COINBASE_STARBASE_ENDPOINTS = Object.freeze({
  http: 'https://drb.coinbase.com/api/v2',
  websocket: 'wss://drb.coinbase.com/ws/api/v2',
});

export type CoinbaseStarbaseTransportKind = keyof typeof COINBASE_STARBASE_ENDPOINTS;

export const resolveCoinbaseStarbaseEndpoint = (kind: CoinbaseStarbaseTransportKind): string =>
  COINBASE_STARBASE_ENDPOINTS[kind];

export type CoinbaseStarbaseJsonRpcParams = Record<string, unknown>;

export type CoinbaseStarbaseJsonRpcRequest<Params extends CoinbaseStarbaseJsonRpcParams> = {
  jsonrpc: '2.0';
  id: number;
  method: string;
  params: Params;
};

export type CoinbaseStarbaseJsonRpcSuccess<Result> = {
  jsonrpc: '2.0';
  id: number;
  result: Result;
};

export type CoinbaseStarbaseJsonRpcErrorObject = {
  code: number;
  message: string;
  data?: unknown;
};

export type CoinbaseStarbaseJsonRpcErrorResponse = {
  jsonrpc: '2.0';
  id?: number;
  error: CoinbaseStarbaseJsonRpcErrorObject;
};

export type CoinbaseStarbaseJsonRpcResponse<Result> =
  CoinbaseStarbaseJsonRpcSuccess<Result> | CoinbaseStarbaseJsonRpcErrorResponse;

export type CoinbaseStarbaseTransportContext = {
  endpoint: string;
  kind: CoinbaseStarbaseTransportKind;
  authorization?: string;
};

export type CoinbaseStarbaseTransport = (
  request: CoinbaseStarbaseJsonRpcRequest<CoinbaseStarbaseJsonRpcParams>,
  context: CoinbaseStarbaseTransportContext,
) => Promise<unknown>;

export type CoinbaseStarbaseHttpRequest = {
  body: CoinbaseStarbaseJsonRpcRequest<CoinbaseStarbaseJsonRpcParams>;
  headers: Record<string, string>;
  method: 'POST';
  url: string;
};

export type CoinbaseStarbaseHttpSender = (request: CoinbaseStarbaseHttpRequest) => Promise<unknown>;

const COINBASE_STARBASE_METHOD_PATTERN = /^(public|private)\/[a-z][a-z0-9_]*$/;

export const requireCoinbaseStarbaseMethod = (method: unknown): string => {
  if (typeof method !== 'string' || !COINBASE_STARBASE_METHOD_PATTERN.test(method)) {
    throw new CoinbaseStarbaseProtocolError('Invalid Starbase JSON-RPC method');
  }
  return method;
};

export const createCoinbaseStarbaseHttpTransport = (send: CoinbaseStarbaseHttpSender): CoinbaseStarbaseTransport => {
  return async (request, context) => {
    if (context.kind !== 'http') {
      throw new CoinbaseStarbaseProtocolError('HTTP transport cannot send a WebSocket request');
    }

    const method = requireCoinbaseStarbaseMethod(request.method);
    return send({
      body: request,
      headers: {
        'Content-Type': 'application/json',
        ...(context.authorization ? { Authorization: context.authorization } : {}),
      },
      method: 'POST',
      url: `${context.endpoint.replace(/\/+$/, '')}/${method}`,
    });
  };
};

export class CoinbaseStarbaseProtocolError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'CoinbaseStarbaseProtocolError';
  }
}

export class CoinbaseStarbaseRpcError extends Error {
  readonly code: number;
  readonly data: unknown;
  readonly method: string;
  readonly requestId?: number;

  constructor(error: CoinbaseStarbaseJsonRpcErrorObject, method: string, requestId?: number) {
    super(error.message);
    this.name = 'CoinbaseStarbaseRpcError';
    this.code = error.code;
    this.data = error.data;
    this.method = method;
    this.requestId = requestId;
  }
}

type CoinbaseStarbaseClientOptions = {
  initialRequestId?: number;
  kind: CoinbaseStarbaseTransportKind;
};

type CoinbaseStarbaseRequestOptions = {
  authorization?: string;
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const isSafeRequestId = (value: unknown): value is number => Number.isSafeInteger(value) && Number(value) > 0;

const parseRpcError = (value: unknown): CoinbaseStarbaseJsonRpcErrorObject | undefined => {
  if (!isRecord(value) || !Number.isInteger(value.code) || typeof value.message !== 'string') {
    return undefined;
  }

  return {
    code: Number(value.code),
    message: value.message,
    ...(Object.prototype.hasOwnProperty.call(value, 'data') ? { data: value.data } : {}),
  };
};

export class CoinbaseStarbaseJsonRpcClient {
  readonly kind: CoinbaseStarbaseTransportKind;

  private nextRequestId: number;
  private readonly transport: CoinbaseStarbaseTransport;

  constructor(transport: CoinbaseStarbaseTransport, options: CoinbaseStarbaseClientOptions) {
    const initialRequestId = options.initialRequestId ?? 1;
    if (!isSafeRequestId(initialRequestId)) {
      throw new CoinbaseStarbaseProtocolError('Initial JSON-RPC request ID must be a positive safe integer');
    }

    this.kind = options.kind;
    this.nextRequestId = initialRequestId;
    this.transport = transport;
  }

  buildRequest<Params extends CoinbaseStarbaseJsonRpcParams>(
    method: string,
    params: Params,
  ): CoinbaseStarbaseJsonRpcRequest<Params> {
    const validatedMethod = requireCoinbaseStarbaseMethod(method);
    if (!isRecord(params)) {
      throw new CoinbaseStarbaseProtocolError('JSON-RPC params must be an object');
    }
    if (!Number.isSafeInteger(this.nextRequestId)) {
      throw new CoinbaseStarbaseProtocolError('JSON-RPC request ID space exhausted');
    }

    const request: CoinbaseStarbaseJsonRpcRequest<Params> = {
      jsonrpc: '2.0',
      id: this.nextRequestId,
      method: validatedMethod,
      params,
    };
    this.nextRequestId += 1;
    return request;
  }

  async request<Result, Params extends CoinbaseStarbaseJsonRpcParams>(
    method: string,
    params: Params,
    options: CoinbaseStarbaseRequestOptions = {},
  ): Promise<Result> {
    const request = this.buildRequest(method, params);
    const response = await this.transport(request, {
      authorization: options.authorization,
      endpoint: resolveCoinbaseStarbaseEndpoint(this.kind),
      kind: this.kind,
    });

    return this.parseResponse<Result>(response, request);
  }

  private parseResponse<Result>(
    response: unknown,
    request: CoinbaseStarbaseJsonRpcRequest<CoinbaseStarbaseJsonRpcParams>,
  ): Result {
    if (!isRecord(response) || response.jsonrpc !== '2.0') {
      throw new CoinbaseStarbaseProtocolError(`Invalid JSON-RPC response for ${request.method}`);
    }

    const hasResult = Object.prototype.hasOwnProperty.call(response, 'result');
    const hasError = Object.prototype.hasOwnProperty.call(response, 'error');
    if (hasResult === hasError) {
      throw new CoinbaseStarbaseProtocolError(
        `JSON-RPC response for ${request.method} must contain exactly one of result or error`,
      );
    }

    if (hasError) {
      const error = parseRpcError(response.error);
      if (!error) {
        throw new CoinbaseStarbaseProtocolError(`Invalid JSON-RPC error for ${request.method}`);
      }
      if (response.id !== undefined && response.id !== request.id) {
        throw new CoinbaseStarbaseProtocolError(`Mismatched JSON-RPC response ID for ${request.method}`);
      }
      throw new CoinbaseStarbaseRpcError(error, request.method, isSafeRequestId(response.id) ? response.id : undefined);
    }

    if (response.id !== request.id) {
      throw new CoinbaseStarbaseProtocolError(`Mismatched JSON-RPC response ID for ${request.method}`);
    }

    return response.result as Result;
  }
}

export type CoinbaseStarbaseAuthResult = {
  access_token: string;
  expires_in: number;
  scope: string;
  token_type: 'bearer';
};

export type CoinbaseStarbaseCdpJwtProvider = () => Promise<string>;

export const COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS = Object.freeze({
  http: 15 * 60 * 1000,
  // Coinbase says to refresh before the approximately 50-minute WS expiry.
  websocket: 49 * 60 * 1000,
});

type CoinbaseStarbaseAuthSessionOptions = {
  clock?: () => number;
  refreshAfterMs?: number;
};

const validateAuthResult = (result: CoinbaseStarbaseAuthResult): void => {
  if (
    !isRecord(result) ||
    typeof result.access_token !== 'string' ||
    !result.access_token ||
    !Number.isInteger(result.expires_in) ||
    result.expires_in <= 0 ||
    typeof result.scope !== 'string' ||
    result.token_type !== 'bearer'
  ) {
    throw new CoinbaseStarbaseProtocolError('Invalid public/auth result');
  }
};

export class CoinbaseStarbaseAuthSession {
  private authenticatedAt = 0;
  private authResult?: CoinbaseStarbaseAuthResult;
  private readonly client: CoinbaseStarbaseJsonRpcClient;
  private readonly clock: () => number;
  private readonly createCdpJwt: CoinbaseStarbaseCdpJwtProvider;
  private readonly refreshAfterMs: number;

  constructor(
    client: CoinbaseStarbaseJsonRpcClient,
    createCdpJwt: CoinbaseStarbaseCdpJwtProvider,
    options: CoinbaseStarbaseAuthSessionOptions = {},
  ) {
    const refreshAfterMs = options.refreshAfterMs ?? COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS[client.kind];
    if (!Number.isFinite(refreshAfterMs) || refreshAfterMs <= 0) {
      throw new CoinbaseStarbaseProtocolError('Auth refresh interval must be positive');
    }

    this.client = client;
    this.clock = options.clock ?? Date.now;
    this.createCdpJwt = createCdpJwt;
    this.refreshAfterMs = refreshAfterMs;
  }

  async authenticate(force = false): Promise<CoinbaseStarbaseAuthResult> {
    const now = this.clock();
    const responseExpiresAt = this.authResult ? this.authenticatedAt + this.authResult.expires_in * 1000 : 0;
    const policyExpiresAt = this.authenticatedAt + this.refreshAfterMs;
    const refreshAt = Math.min(responseExpiresAt, policyExpiresAt);

    if (!force && this.authResult && now < refreshAt) {
      return this.authResult;
    }

    const token = await this.createCdpJwt();
    if (!token) {
      throw new CoinbaseStarbaseProtocolError('CDP JWT provider returned an empty token');
    }

    const result = await this.client.request<CoinbaseStarbaseAuthResult, { grant_type: 'coinbase_cdp'; token: string }>(
      'public/auth',
      { grant_type: 'coinbase_cdp', token },
    );
    validateAuthResult(result);

    this.authResult = result;
    this.authenticatedAt = now;
    return result;
  }

  async getHttpAuthorization(forceRefresh = false): Promise<string> {
    if (this.client.kind !== 'http') {
      throw new CoinbaseStarbaseProtocolError('Bearer authorization headers apply only to Starbase HTTP');
    }

    const result = await this.authenticate(forceRefresh);
    return `Bearer ${result.access_token}`;
  }
}

export type CoinbaseStarbaseHeartbeatSignal = 'heartbeat' | 'test_request';

export type CoinbaseStarbaseTestResult = {
  version: string;
};

export class CoinbaseStarbaseHeartbeatController {
  private readonly client: CoinbaseStarbaseJsonRpcClient;
  private readonly intervalSeconds: number;

  constructor(client: CoinbaseStarbaseJsonRpcClient, intervalSeconds: number) {
    if (client.kind !== 'websocket') {
      throw new CoinbaseStarbaseProtocolError('Starbase heartbeat is WebSocket-only');
    }
    if (!Number.isFinite(intervalSeconds) || intervalSeconds < 10) {
      throw new CoinbaseStarbaseProtocolError('Starbase heartbeat interval must be at least 10 seconds');
    }

    this.client = client;
    this.intervalSeconds = intervalSeconds;
  }

  async enable(): Promise<void> {
    const result = await this.client.request<'ok', { interval: number }>('public/set_heartbeat', {
      interval: this.intervalSeconds,
    });
    if (result !== 'ok') {
      throw new CoinbaseStarbaseProtocolError('Unexpected public/set_heartbeat result');
    }
  }

  async disable(): Promise<void> {
    const result = await this.client.request<'ok', CoinbaseStarbaseJsonRpcParams>('public/disable_heartbeat', {});
    if (result !== 'ok') {
      throw new CoinbaseStarbaseProtocolError('Unexpected public/disable_heartbeat result');
    }
  }

  async handleSignal(signal: CoinbaseStarbaseHeartbeatSignal): Promise<CoinbaseStarbaseTestResult | undefined> {
    if (signal === 'heartbeat') {
      return undefined;
    }

    return this.client.request<CoinbaseStarbaseTestResult, CoinbaseStarbaseJsonRpcParams>('public/test', {});
  }
}
