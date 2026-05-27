# Lighter SDK HTTP Client

The HTTP client module provides a complete API client implementation for the Lighter protocol, including transaction building, signing, and submission capabilities.

## Files

- **`http-types.ts`** - Type definitions for HTTP requests, responses, and errors
- **`api-client.ts`** - Main HTTP client with low-level API methods
- **`transaction-client.ts`** - High-level transaction client for signing and submitting transactions
- **`index.ts`** - Module exports

## Usage

### Basic API Client

```typescript
import { LighterApiClient } from '@tealstreet/lighter-sdk/client';

const client = new LighterApiClient({
  baseUrl: 'https://api.lighter.xyz',
  timeout: 30000,
  debug: true
});

// Get next nonce
const nonce = await client.getNextNonce(accountIndex, apiKeyIndex);

// Get API keys
const apiKeys = await client.getApiKeys(accountIndex, apiKeyIndex);

// Get transfer fee info
const feeInfo = await client.getTransferFeeInfo(
  accountIndex,
  toAccountIndex,
  authToken
);

// Send raw transaction
const txHash = await client.sendRawTx(txInfo);
```

### Transaction Client

```typescript
import { TransactionClient } from '@tealstreet/lighter-sdk/client';
import { loadPoseidonCrypto } from '@tealstreet/poseidon-crypto';

// Initialize crypto
const crypto = await loadPoseidonCrypto();

// Create transaction client
const txClient = new TransactionClient({
  httpClient: client,
  crypto,
  chainId: 1,
  privateKey: '0x...',
  accountIndex: 1n,
  apiKeyIndex: 0
});

// Create order
const orderTx = await txClient.getCreateOrderTransaction({
  orderId: 123n,
  marketId: 1,
  isBuy: true,
  price: 100000n,
  amount: 50000n,
  postOnly: true
});

// Submit directly
const txHash = await client.sendRawTx(orderTx);

// Or use convenience method
const txHash2 = await txClient.submitCreateOrder({
  orderId: 124n,
  marketId: 1,
  isBuy: false,
  price: 105000n,
  amount: 30000n
});
```

### Cancel Order

```typescript
const cancelTx = await txClient.getCancelOrderTransaction({
  orderId: 123n,
  marketId: 1
});

const txHash = await client.sendRawTx(cancelTx);

// Or use convenience method
const txHash2 = await txClient.submitCancelOrder({
  orderId: 124n,
  marketId: 1
});
```

### Transfer

```typescript
const transferTx = await txClient.getTransferTransaction({
  toAccountIndex: 2n,
  amount: 1000000n,
  tokenId: 0
});

const txHash = await client.sendRawTx(transferTx);

// Or use convenience method
const txHash2 = await txClient.submitTransfer({
  toAccountIndex: 3n,
  amount: 500000n
});
```

### Authentication Token

```typescript
// Generate auth token (valid for 1 hour)
const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);
const authToken = await txClient.getAuthToken(deadline);

// Use in API calls
const feeInfo = await client.getTransferFeeInfo(
  accountIndex,
  toAccountIndex,
  authToken
);
```

### Account API

```typescript
import { AccountApi } from '@tealstreet/lighter-sdk/client';

const accountApi = new AccountApi(client);

// Get account information
const account = await accountApi.getAccount(1n);
console.log('Account:', account.status, account.tier);

// Get account limits
const limits = await accountApi.getAccountLimits(1n);
console.log('Max open orders:', limits.maxOpenOrders);
console.log('Rate limits:', limits.rateLimits);

// Get account metadata
const metadata = await accountApi.getAccountMetadata(1n);
console.log('Alias:', metadata.alias);

// Get accounts by L1 address
const accounts = await accountApi.getAccountsByL1Address('0x1234...');
accounts.accounts.forEach(acc => {
  console.log('Account:', acc.accountIndex, acc.alias);
});

// Get API keys for an account
const apiKeys = await accountApi.getApiKeys(1n);
apiKeys.apiKeys.forEach(key => {
  console.log('Key:', key.index, key.label, key.isActive);
});

// Get liquidation history
const liquidations = await accountApi.getLiquidations(1n, {
  limit: 20,
  marketId: 1
});

// Get PnL data
const pnl = await accountApi.getPnl(1n, {
  period: '24h'
});
console.log('Total PnL:', pnl.summary?.totalPnl);

// Get PnL for specific market
const marketPnl = await accountApi.getPnl(1n, {
  period: '7d',
  marketId: 1
});
```

### Request/Response Interceptors

```typescript
// Add request interceptor
client.addRequestInterceptor((config) => {
  console.log('Making request:', config.url);
  return config;
});

// Add response interceptor
client.addResponseInterceptor((response) => {
  console.log('Received response:', response.status);
  return response;
});

// Add error interceptor
client.addErrorInterceptor((error) => {
  console.error('Request failed:', error.message);
  return error;
});
```

### Error Handling

```typescript
import { LighterApiError } from '@tealstreet/lighter-sdk/client';

try {
  const txHash = await client.sendRawTx(txInfo);
} catch (error) {
  if (error instanceof LighterApiError) {
    console.error('API Error:', {
      code: error.code,
      message: error.message,
      details: error.details,
      statusCode: error.statusCode,
      requestId: error.requestId
    });
  }
}
```

## API Methods

### LighterApiClient

- `getNextNonce(accountIndex, apiKeyIndex)` - Get next available nonce
- `getApiKeys(accountIndex, apiKeyIndex)` - Get all API keys for an account
- `sendRawTx(txInfo)` - Send a signed raw transaction
- `getTransferFeeInfo(accountIndex, toAccountIndex, auth)` - Get transfer fee information
- `get(path, config)` - Make custom GET request
- `post(path, data, config)` - Make custom POST request
- `put(path, data, config)` - Make custom PUT request
- `delete(path, config)` - Make custom DELETE request

### TransactionClient

- `getCreateOrderTransaction(params)` - Build and sign create order transaction
- `getCancelOrderTransaction(params)` - Build and sign cancel order transaction
- `getTransferTransaction(params)` - Build and sign transfer transaction
- `getAuthToken(deadline)` - Generate authentication token
- `submitCreateOrder(params)` - Create, sign, and submit order
- `submitCancelOrder(params)` - Create, sign, and submit cancel
- `submitTransfer(params)` - Create, sign, and submit transfer
- `resetNonce()` - Reset cached nonce

### AccountApi

- `getAccount(accountIndex)` - Get account information by account index
- `getAccountLimits(accountIndex)` - Get account limits (rate limits, position limits, etc.)
- `getAccountMetadata(accountIndex)` - Get account metadata (alias, labels, custom fields)
- `getAccountsByL1Address(l1Address)` - Get all accounts associated with an L1 address
- `getApiKeys(accountIndex)` - Get all API keys for an account
- `getLiquidations(accountIndex, options?)` - Get liquidation history for an account
- `getPnl(accountIndex, options?)` - Get profit and loss (PnL) data for an account

## Types

All types are exported from the main module:

```typescript
import type {
  // Account API types
  AccountInfo,
  AccountLimits,
  AccountMetadata,
  AccountsByL1Address,
  AccountApiKeys,
  ApiKeyInfo,
  Liquidation,
  Liquidations,
  PnL,
  PnLDataPoint,
  GetLiquidationsOptions,
  GetPnlOptions,

  // HTTP client types
  ApiKey,
  ApiResponse,
  ApiErrorResponse,
  TransferFeeInfo,
  TxInfo,
  L2CreateOrderTxInfo,
  L2CancelOrderTxInfo,
  L2TransferTxInfo,
  CreateOrderParams,
  CancelOrderParams,
  TransferParams,
  LighterApiClientConfig,
  // ... and more
} from '@tealstreet/lighter-sdk/client';
```

## Transaction Format

All transactions are serialized as hex-encoded strings with the following formats:

### Create Order
- Chain ID (4 bytes)
- Account Index (32 bytes)
- Nonce (32 bytes)
- Order ID (32 bytes)
- Market ID (4 bytes)
- Is Buy (1 byte)
- Price (32 bytes)
- Amount (32 bytes)
- Flags (1 byte): bit 0 = postOnly, bit 1 = reduceOnly

### Cancel Order
- Chain ID (4 bytes)
- Account Index (32 bytes)
- Nonce (32 bytes)
- Order ID (32 bytes)
- Market ID (4 bytes)

### Transfer
- Chain ID (4 bytes)
- Account Index (32 bytes)
- Nonce (32 bytes)
- To Account Index (32 bytes)
- Amount (32 bytes)
- Token ID (4 bytes)

All transactions are signed using Schnorr signatures via the Poseidon crypto module.
