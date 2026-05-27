# @tealstreet/lighter-sdk

A TypeScript SDK for interacting with the Lighter protocol - a high-performance perpetual futures DEX built on Layer 2.

## Features

- **Complete Transaction Support**: Create, cancel, and manage orders with full L2 transaction capabilities
- **Cryptographic Signing**: Built-in Poseidon2 hashing and Schnorr signature support over ECgFp5 curve
- **Type-Safe**: Full TypeScript support with comprehensive type definitions
- **HTTP API Client**: High-level client for REST API interactions with interceptor support
- **Authentication**: Secure authentication token generation for API requests
- **Transaction Builder**: Simplified transaction building and signing workflow
- **Key Management**: Utilities for generating and managing cryptographic keys
- **Error Handling**: Comprehensive error types and messages for debugging

## Installation

```bash
yarn add @tealstreet/lighter-sdk
# or
npm install @tealstreet/lighter-sdk
```

## Quick Start

### Basic Setup

```typescript
import { LighterApiClient, TransactionClient } from '@tealstreet/lighter-sdk/client';
import { getCryptoApi } from '@tealstreet/poseidon-crypto';

// Initialize the HTTP client
const apiClient = new LighterApiClient({
  baseUrl: 'https://api.lighter.xyz',
  timeout: 30000,
  debug: true // Enable debug logging
});

// Initialize crypto module
const crypto = getCryptoApi();
await crypto.initialize();

// Create transaction client
const txClient = new TransactionClient({
  httpClient: apiClient,
  crypto,
  chainId: 1,
  privateKey: 'your-private-key-hex',
  accountIndex: 1n,
  apiKeyIndex: 0
});
```

### Placing an Order

```typescript
import { CreateOrderParams } from '@tealstreet/lighter-sdk/client';

// Build the order transaction
const orderTx = await txClient.getCreateOrderTransaction({
  orderId: 123n,
  marketId: 1,
  isBuy: true,
  price: 100000n,  // Price in base units
  amount: 50000n,  // Amount in base units
  postOnly: true,
  reduceOnly: false
});

// Submit the transaction
const txHash = await apiClient.sendRawTx(orderTx);
console.log('Order placed! Transaction hash:', txHash);
```

### Canceling an Order

```typescript
// Build the cancel transaction
const cancelTx = await txClient.getCancelOrderTransaction({
  orderId: 123n,
  marketId: 1
});

// Submit the cancellation
const txHash = await apiClient.sendRawTx(cancelTx);
console.log('Order canceled! Transaction hash:', txHash);
```

### Direct Order Submission

```typescript
// Submit order in one step
const txHash = await txClient.submitCreateOrder({
  orderId: 124n,
  marketId: 1,
  isBuy: false,
  price: 101000n,
  amount: 25000n
});

console.log('Order submitted:', txHash);
```

## API Reference

### LighterApiClient

The main HTTP client for interacting with the Lighter API.

```typescript
class LighterApiClient {
  constructor(config: LighterApiClientConfig);

  // Get next available nonce for an account
  async getNextNonce(accountIndex: bigint, apiKeyIndex: number): Promise<bigint>;

  // Get all API keys for an account
  async getApiKeys(accountIndex: bigint, apiKeyIndex: number): Promise<ApiKey[]>;

  // Send a signed transaction
  async sendRawTx(txInfo: TxInfo): Promise<string>;

  // Get transfer fee information
  async getTransferFeeInfo(
    accountIndex: bigint,
    toAccountIndex: bigint,
    auth: string
  ): Promise<TransferFeeInfo>;

  // Custom HTTP methods
  async get<T>(path: string, config?: AxiosRequestConfig): Promise<T>;
  async post<T>(path: string, data?: any, config?: AxiosRequestConfig): Promise<T>;
  async put<T>(path: string, data?: any, config?: AxiosRequestConfig): Promise<T>;
  async delete<T>(path: string, config?: AxiosRequestConfig): Promise<T>;

  // Interceptor management
  addRequestInterceptor(interceptor: RequestInterceptor): void;
  addResponseInterceptor(interceptor: ResponseInterceptor): void;
  addErrorInterceptor(interceptor: ErrorInterceptor): void;

  // Configuration
  getBaseUrl(): string;
  getConfig(): Readonly<Required<LighterApiClientConfig>>;
}
```

**Configuration Options:**

```typescript
interface LighterApiClientConfig {
  baseUrl: string;           // API base URL
  timeout?: number;          // Request timeout in ms (default: 30000)
  headers?: Record<string, string>;  // Custom headers
  debug?: boolean;           // Enable debug logging (default: false)
}
```

### TransactionClient

Handles transaction creation, signing, and submission.

```typescript
class TransactionClient {
  constructor(config: {
    httpClient: LighterApiClient;
    crypto: PoseidonCrypto;
    chainId: number;
    privateKey: string;
    accountIndex: bigint;
    apiKeyIndex: number;
  });

  // Build transactions
  async getCreateOrderTransaction(params: CreateOrderParams): Promise<L2CreateOrderTxInfo>;
  async getCancelOrderTransaction(params: CancelOrderParams): Promise<L2CancelOrderTxInfo>;
  async getTransferTransaction(params: TransferParams): Promise<L2TransferTxInfo>;

  // Submit transactions directly
  async submitCreateOrder(params: CreateOrderParams): Promise<string>;
  async submitCancelOrder(params: CancelOrderParams): Promise<string>;
  async submitTransfer(params: TransferParams): Promise<string>;

  // Authentication
  async getAuthToken(deadline: bigint): Promise<string>;

  // Utilities
  resetNonce(): void;
  getAccountIndex(): bigint;
  getApiKeyIndex(): number;
  getChainId(): number;
}
```

### TransactionSigner

Low-level signing functionality for Lighter protocol transactions.

```typescript
class TransactionSigner {
  // Initialize the signer (must be called before use)
  async initialize(): Promise<void>;

  // Sign transactions
  async signCreateOrder(tx: L2CreateOrderTxInfo, privateKey: string, chainId: number): Promise<TransactionSignature>;
  async signCancelOrder(tx: L2CancelOrderTxInfo, privateKey: string, chainId: number): Promise<TransactionSignature>;
  async signTransfer(tx: L2TransferTxInfo, privateKey: string, chainId: number): Promise<TransactionSignature>;
  async signWithdraw(tx: L2WithdrawTxInfo, privateKey: string, chainId: number): Promise<TransactionSignature>;

  // Sign raw message
  async signMessage(messageHex: string, privateKey: string): Promise<TransactionSignature>;

  // Create auth token
  async createAuthToken(
    accountIndex: number,
    apiKeyIndex: number,
    deadline: bigint,
    privateKey: string,
    chainId: number
  ): Promise<AuthToken>;

  // Verify signature
  async verifySignature(messageHex: string, publicKey: string, signature: string): Promise<boolean>;
}

// Convenience function to create and initialize a signer
async function createSigner(): Promise<TransactionSigner>;
```

### Key Types

```typescript
// Order parameters
interface CreateOrderParams {
  orderId: bigint;
  marketId: number;
  isBuy: boolean;
  price: bigint;
  amount: bigint;
  postOnly?: boolean;
  reduceOnly?: boolean;
}

interface CancelOrderParams {
  orderId: bigint;
  marketId: number;
}

interface TransferParams {
  toAccountIndex: bigint;
  amount: bigint;
  tokenId?: number;
}

// Transaction signature result
interface TransactionSignature {
  txHash: string;      // 64 hex characters
  signature: string;   // 160 hex characters
}

// Authentication token
interface AuthToken {
  token: string;
  message: string;
  messageHash: string;
  accountIndex: number;
  apiKeyIndex: number;
  deadline: bigint;
}
```

## Authentication

### Generating API Keys

API keys must be registered on-chain through the Lighter protocol smart contract. Each account can have multiple API keys (indexed 0-255).

### Creating Auth Tokens

Auth tokens are required for authenticated API endpoints:

```typescript
import { TransactionClient } from '@tealstreet/lighter-sdk/client';

// Create a token valid for 1 hour
const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);
const authToken = await txClient.getAuthToken(deadline);

// Use in API requests
const response = await apiClient.get('/api/v1/orders', {
  headers: {
    'Authorization': `Bearer ${authToken}`
  }
});
```

### Nonce Management

The TransactionClient automatically manages nonces with caching:

- First transaction fetches the nonce from the server
- Subsequent transactions increment the cached nonce
- Call `resetNonce()` to force a refresh from the server

```typescript
// Reset nonce after an error or chain reorganization
txClient.resetNonce();
```

## Examples

### Creating Different Order Types

#### Limit Order (Post-Only)

```typescript
const txHash = await txClient.submitCreateOrder({
  orderId: generateOrderId(),
  marketId: 1,
  isBuy: true,
  price: 50000n * 10n**8n,  // $50,000 with 8 decimal precision
  amount: 1n * 10n**8n,      // 1 unit with 8 decimal precision
  postOnly: true,
  reduceOnly: false
});
```

#### Market Order (Immediate Execution)

```typescript
const txHash = await txClient.submitCreateOrder({
  orderId: generateOrderId(),
  marketId: 1,
  isBuy: true,
  price: 0n,  // Price ignored for market orders
  amount: 1n * 10n**8n,
  postOnly: false,
  reduceOnly: false
});
```

#### Reduce-Only Order

```typescript
const txHash = await txClient.submitCreateOrder({
  orderId: generateOrderId(),
  marketId: 1,
  isBuy: false,
  price: 51000n * 10n**8n,
  amount: 5n * 10n**7n,  // 0.5 units
  postOnly: false,
  reduceOnly: true  // Only reduces position, won't flip
});
```

### Transfers and Withdrawals

#### Internal Transfer

```typescript
const transferTx = await txClient.submitTransfer({
  toAccountIndex: 2n,
  amount: 1000000n,  // Amount in base token units
  tokenId: 0  // USDC or default token
});
```

#### Check Transfer Fees

```typescript
// Get auth token first
const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);
const authToken = await txClient.getAuthToken(deadline);

// Get fee info
const feeInfo = await apiClient.getTransferFeeInfo(
  1n,  // From account
  2n,  // To account
  authToken
);

console.log('Transfer fee:', feeInfo.fee);
console.log('Fee percentage:', feeInfo.feePercentage);
console.log('Min/Max amount:', feeInfo.minAmount, feeInfo.maxAmount);
```

### Error Handling

```typescript
import { LighterApiError } from '@tealstreet/lighter-sdk/client';
import { SignerError } from '@tealstreet/lighter-sdk/signer';

try {
  const txHash = await txClient.submitCreateOrder(orderParams);
  console.log('Success:', txHash);
} catch (error) {
  if (error instanceof LighterApiError) {
    console.error('API Error:', error.message);
    console.error('Error code:', error.code);
    console.error('HTTP status:', error.httpStatus);
    console.error('Details:', error.details);
    console.error('Request ID:', error.requestId);
  } else if (error instanceof SignerError) {
    console.error('Signing error:', error.message);
    console.error('Cause:', error.cause);
  } else {
    console.error('Unexpected error:', error);
  }
}
```

### Using Interceptors

Interceptors allow you to modify requests/responses or handle errors globally:

```typescript
// Add request interceptor to log all requests
apiClient.addRequestInterceptor((config) => {
  console.log(`${config.method?.toUpperCase()} ${config.url}`);
  return config;
});

// Add response interceptor to extract data
apiClient.addResponseInterceptor((response) => {
  if (response.data?.success) {
    console.log('Response data:', response.data.data);
  }
  return response;
});

// Add error interceptor for retry logic
apiClient.addErrorInterceptor((error) => {
  if (error.response?.status === 429) {
    console.warn('Rate limited, consider implementing retry logic');
  }
  return error;
});
```

## Transaction Signing

The SDK uses Poseidon2 hash function and Schnorr signatures over the ECgFp5 elliptic curve for transaction signing.

### How Signing Works

1. **Transaction Data Serialization**: Transaction parameters are encoded into a hex string
2. **Field Element Conversion**: Data is converted to field elements compatible with the Poseidon2 hash
3. **Hashing**: The message is hashed using Poseidon2 hash function
4. **Signing**: The hash is signed using Schnorr signature scheme with the private key

### Manual Signing Example

```typescript
import { createSigner } from '@tealstreet/lighter-sdk/signer';
import { TxType } from '@tealstreet/lighter-sdk/types';

const signer = await createSigner();

// Create transaction info
const tx = {
  txType: TxType.L2CreateOrder,
  marketId: "0x123abc",
  orderType: 0,  // Limit order
  side: 0,       // Buy
  price: "1000000000000",
  quantity: "100000000",
  timeInForce: 1,  // GoodTillTime
  reduceOnly: false,
  postOnly: false,
  opts: {
    nonce: 1,
    expiry: Math.floor(Date.now() / 1000) + 3600,
    accountIndex: 0,
    apiKeyIndex: 0
  }
};

// Sign the transaction
const signature = await signer.signCreateOrder(tx, privateKey, 1);
console.log('TX Hash:', signature.txHash);
console.log('Signature:', signature.signature);
```

### Field Element Conversion

The SDK provides utilities for converting different data types to field elements:

```typescript
import {
  uint32ToFieldElement,
  uint64ToFieldElement,
  int64ToFieldElement,
  hexToFieldElement,
  concatenateFieldElements
} from '@tealstreet/lighter-sdk/signer';

// Convert unsigned 32-bit integer
const fe32 = uint32ToFieldElement(12345);

// Convert unsigned 64-bit integer
const fe64 = uint64ToFieldElement(9876543210n);

// Convert signed 64-bit integer
const feInt = int64ToFieldElement(-1000n);

// Convert hex string to field element
const feHex = hexToFieldElement("0x123abc");

// Concatenate multiple field elements
const combined = concatenateFieldElements([fe32, fe64, feHex]);
```

## Development

### Building the Package

```bash
# Install dependencies
yarn install

# Build for production (using tsup)
yarn build-force

# Watch mode for development
yarn dev-force
```

### Running Tests

```bash
# Run tests once
yarn test

# Run tests in watch mode
yarn test-unit-watch

# Run tests with UI
yarn test-unit-watch
```

### Linting

```bash
# Lint all files
yarn lint

# Fix linting issues
yarn fix-all-files
```

### Type Checking

```bash
# Check types without emitting
yarn typecheck
```

## Constants and Enums

### Order Types

```typescript
enum OrderType {
  Limit = 0,        // Order with specified price
  Market = 1,       // Execute at market price
  StopLoss = 2,     // Triggered at stop price
  TakeProfit = 3,   // Triggered at target price
  TWAP = 4          // Time-weighted average price
}
```

### Order Side

```typescript
enum OrderSide {
  Buy = 0,
  Sell = 1
}
```

### Time in Force

```typescript
enum TimeInForce {
  ImmediateOrCancel = 0,  // Execute immediately or cancel
  GoodTillTime = 1,       // Valid until expiry time
  PostOnly = 2            // Only add liquidity, never take
}
```

### Order Status

```typescript
enum OrderStatus {
  Pending = 0,
  Open = 1,
  PartiallyFilled = 2,
  Filled = 3,
  Cancelled = 4,
  Rejected = 5,
  Expired = 6
}
```

### Transaction Types

```typescript
const TxType = {
  L2CreateOrder: 14,
  L2CancelOrder: 15,
  L2Transfer: 16,
  L2Withdraw: 17,
  L2UpdateApiKey: 18,
  L2Liquidation: 19,
  L2Deleverage: 20,
  L2FundingFee: 21
} as const;
```

## License

Apache-2.0

## Support

For issues and questions:
- GitHub Issues: [tealstreet/lighter-sdk](https://github.com/tealstreet/lighter-sdk)
- Documentation: [docs.lighter.xyz](https://docs.lighter.xyz)
- Discord: [discord.gg/lighter](https://discord.gg/lighter)

## Contributing

Contributions are welcome! Please read our contributing guidelines and submit pull requests to our repository.
