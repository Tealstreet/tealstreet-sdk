import type {
  CoinbaseStarbaseAuthResult,
  CoinbaseStarbaseJsonRpcErrorResponse,
  CoinbaseStarbaseJsonRpcRequest,
  CoinbaseStarbaseJsonRpcSuccess,
  CoinbaseStarbaseTestResult,
} from '../reference/coinbase/coinbase.starbase';

export const STARBASE_AUTH_REQUEST: CoinbaseStarbaseJsonRpcRequest<{
  grant_type: 'coinbase_cdp';
  token: string;
}> = {
  jsonrpc: '2.0',
  id: 1,
  method: 'public/auth',
  params: {
    grant_type: 'coinbase_cdp',
    token: '<fresh_signed_cdp_jwt>',
  },
};

export const STARBASE_AUTH_RESULT: CoinbaseStarbaseAuthResult = {
  access_token: 'official-schema-access-token-fixture',
  expires_in: 3600,
  scope: 'wallet:user:read wallet:accounts:read wallet:transactions:read wallet:buys:create',
  token_type: 'bearer',
};

export const STARBASE_AUTH_RESPONSE: CoinbaseStarbaseJsonRpcSuccess<CoinbaseStarbaseAuthResult> = {
  jsonrpc: '2.0',
  id: 1,
  result: STARBASE_AUTH_RESULT,
};

export const STARBASE_SET_HEARTBEAT_REQUEST: CoinbaseStarbaseJsonRpcRequest<{ interval: number }> = {
  jsonrpc: '2.0',
  id: 1,
  method: 'public/set_heartbeat',
  params: { interval: 30 },
};

export const STARBASE_OK_RESPONSE: CoinbaseStarbaseJsonRpcSuccess<'ok'> = {
  jsonrpc: '2.0',
  id: 1,
  result: 'ok',
};

export const STARBASE_PUBLIC_TEST_RESPONSE: CoinbaseStarbaseJsonRpcSuccess<CoinbaseStarbaseTestResult> & {
  testnet: boolean;
  usDiff: number;
  usIn: number;
  usOut: number;
} = {
  jsonrpc: '2.0',
  id: 2,
  result: { version: '1.2.26' },
  testnet: false,
  usDiff: 100,
  usIn: 1784113251374106,
  usOut: 1784113251374206,
};

export const STARBASE_ERROR_RESPONSE: CoinbaseStarbaseJsonRpcErrorResponse = {
  jsonrpc: '2.0',
  error: {
    code: -32601,
    message: 'Method not found',
    data: 'Method not found',
  },
};
