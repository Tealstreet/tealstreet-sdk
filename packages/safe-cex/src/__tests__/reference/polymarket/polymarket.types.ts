/**
 * Polymarket-specific type definitions
 *
 * These types represent the API responses and internal data structures
 * specific to Polymarket's prediction market platform.
 */

import type { SignatureType } from './polymarket.constants';

/**
 * API credentials derived from wallet signature
 */
export interface PolymarketCredentials {
  /** API key for authenticated requests */
  apiKey: string;
  /** Secret for HMAC signing */
  secret: string;
  /** Passphrase for additional security */
  passphrase: string;
}

/**
 * Token within a prediction market (YES or NO outcome)
 */
export interface PolymarketToken {
  /** ERC1155 token ID */
  token_id: string;
  /** Outcome this token represents */
  outcome: 'Yes' | 'No';
  /** Whether this token won (after resolution) */
  winner?: boolean;
}

/**
 * Market from the Gamma API (market metadata)
 */
export interface PolymarketGammaMarket {
  /** Unique condition ID for the market */
  condition_id: string;
  /** Question ID from UMA oracle */
  question_id: string;
  /** Tokens in this market (YES/NO) */
  tokens: PolymarketToken[];
  /** The prediction question */
  question: string;
  /** Detailed description */
  description: string;
  /** URL-friendly slug */
  market_slug: string;
  /** Market end date */
  end_date_iso: string;
  /** Whether market is active for trading */
  active: boolean;
  /** Whether market is closed */
  closed: boolean;
  /** Whether market is archived */
  archived: boolean;
  /** Minimum order size in shares */
  minimum_order_size: string;
  /** Minimum price tick size */
  minimum_tick_size: string;
  /** Category/tags */
  tags?: string[];
  /** Trading volume */
  volume?: string;
  /** 24h volume */
  volume_24h?: string;
  /** Liquidity */
  liquidity?: string;
  /** Number of unique traders */
  unique_traders?: number;
}

/**
 * Market from the CLOB API (trading info)
 */
export interface PolymarketClobMarket {
  /** Condition ID */
  condition_id: string;
  /** Token ID */
  token_id: string;
  /** YES or NO */
  outcome: string;
  /** Minimum order size */
  minimum_order_size: string;
  /** Minimum tick size */
  minimum_tick_size: string;
  /** Whether trading is enabled */
  active: boolean;
  /** Whether market is closed */
  closed: boolean;
}

/**
 * Order as returned by Polymarket API
 */
export interface PolymarketAPIOrder {
  /** Unique order ID */
  id: string;
  /** Owner's API key */
  owner: string;
  /** Market condition ID */
  market: string;
  /** Token ID being traded */
  asset_id: string;
  /** BUY or SELL */
  side: 'BUY' | 'SELL';
  /** Original order size in shares */
  original_size: string;
  /** Amount matched/filled */
  size_matched: string;
  /** Order price (0.00-1.00) */
  price: string;
  /** Order status */
  status: 'live' | 'matched' | 'cancelled' | 'delayed';
  /** Creation timestamp */
  created_at: number;
  /** Expiration timestamp */
  expiration: number;
  /** Order type */
  order_type: 'GTC' | 'FOK' | 'GTD' | 'FAK';
  /** Associated trades */
  associate_trades?: PolymarketTrade[];
}

/**
 * Trade/execution from Polymarket API
 */
export interface PolymarketTrade {
  /** Trade ID */
  id: string;
  /** Taker order ID */
  taker_order_id: string;
  /** Maker order ID */
  maker_order_id: string;
  /** Market condition ID */
  market: string;
  /** Token ID traded */
  asset_id: string;
  /** Trade side for the user */
  side: 'BUY' | 'SELL';
  /** Trade size in shares */
  size: string;
  /** Execution price */
  price: string;
  /** Trade status */
  status: string;
  /** Match timestamp */
  match_time: number;
  /** Transaction hash (if settled on-chain) */
  transaction_hash?: string;
  /** Fee paid */
  fee_rate_bps?: string;
  /** Bucket index for reward calculation */
  bucket_index?: number;
  /** Whether user was maker */
  maker_address?: string;
}

/**
 * Position as represented in our system.
 */
export interface PolymarketPosition {
  /** Token ID */
  token_id: string;
  /** Market condition ID */
  condition_id: string;
  /** Outcome (YES/NO) */
  outcome: string;
  /** Number of shares held */
  size: string;
  /** Average entry price (cost basis) */
  avg_price: string;
  /** Market slug for display */
  market_slug?: string;
  /** The question for display */
  question?: string;
  /** Current market price */
  current_price?: string;
  /** Unrealized PnL */
  unrealized_pnl?: string;
}

/**
 * Balance/allowance response from API
 */
export interface PolymarketBalanceAllowance {
  /** pUSD balance in six-decimal base units */
  balance: string;
  /** Allowance set for exchange contract */
  allowance: string;
}

/**
 * Order book level
 */
export interface PolymarketOrderBookLevel {
  /** Price level */
  price: string;
  /** Size at this level */
  size: string;
}

/**
 * Order book from API
 */
export interface PolymarketOrderBook {
  /** Token ID */
  asset_id: string;
  /** Bid levels */
  bids: PolymarketOrderBookLevel[];
  /** Ask levels */
  asks: PolymarketOrderBookLevel[];
  /** Timestamp */
  timestamp?: number;
  /** Market hash */
  hash?: string;
}

/**
 * Price from API
 */
export interface PolymarketPrice {
  /** Token ID */
  token_id: string;
  /** Current price */
  price: string;
  /** Timestamp */
  timestamp?: number;
}

/**
 * Signed order structure for submission
 */
export interface PolymarketSignedOrder {
  /** Random salt for uniqueness */
  salt: string;
  /** Maker address (funder) */
  maker: string;
  /** Signer address */
  signer: string;
  /** Token ID */
  tokenId: string;
  /** Maximum amount maker will spend */
  makerAmount: string;
  /** Minimum amount taker must provide */
  takerAmount: string;
  /** Wire-only expiration timestamp; not part of the V2 EIP-712 struct */
  expiration: string;
  /** Order side */
  side: 'BUY' | 'SELL';
  /** Signature type */
  signatureType: SignatureType;
  /** Millisecond creation timestamp used by the V2 signed struct */
  timestamp: string;
  /** V2 metadata field */
  metadata: string;
  /** V2 builder attribution field */
  builder: string;
  /** EIP-712 signature */
  signature: string;
}

/**
 * Order creation request
 */
export interface PolymarketCreateOrderRequest {
  /** Signed order structure */
  order: PolymarketSignedOrder;
  /** Owner's API key */
  owner: string;
  /** Order type */
  orderType: 'GTC' | 'FOK' | 'GTD' | 'FAK';
}

/**
 * Order response from API
 */
export interface PolymarketOrderResponse {
  /** Whether order was successful */
  success: boolean;
  /** Error message if failed */
  errorMsg?: string;
  /** Order ID if successful */
  orderID?: string;
  /** Transaction hashes if matched */
  transactionsHashes?: string[];
  /** Order status */
  status?: string;
  /** Amount taken (for market orders) */
  takingAmount?: string;
  /** Amount made (for market orders) */
  makingAmount?: string;
}

/**
 * Cancel request
 */
export interface PolymarketCancelRequest {
  /** Order ID(s) to cancel */
  orderIds?: string[];
  /** Market to cancel all orders for */
  market?: string;
  /** Asset ID to cancel all orders for */
  asset_id?: string;
}

/**
 * WebSocket subscription message
 */
export interface PolymarketWSSubscription {
  /** Authentication object */
  auth?: {
    apiKey: string;
    secret: string;
    passphrase: string;
  };
  /** Markets to subscribe to (condition IDs) */
  markets?: string[];
  /** Asset IDs to subscribe to (token IDs) */
  assets_ids?: string[];
  /** Channel type */
  type: 'user' | 'market';
  /** Custom feature flag */
  custom_feature_enabled?: boolean;
}

/**
 * WebSocket operation message (subscribe/unsubscribe)
 */
export interface PolymarketWSOperation {
  /** Asset IDs */
  assets_ids?: string[];
  /** Markets (condition IDs) */
  markets?: string[];
  /** Operation type */
  operation: 'subscribe' | 'unsubscribe';
  /** Custom feature flag */
  custom_feature_enabled?: boolean;
}

/**
 * WebSocket price change event
 */
export interface PolymarketWSPriceChange {
  /** Event type */
  event_type: 'price_change';
  /** Price-level changes, potentially spanning both outcome assets. */
  price_changes: Array<{
    asset_id: string;
    price: string;
    size: string;
    side: 'BUY' | 'SELL';
    best_bid: string;
    best_ask: string;
  }>;
  /** Timestamp */
  timestamp: number;
}

/**
 * WebSocket order book update event
 */
export interface PolymarketWSOrderBookUpdate {
  /** Event type */
  event_type: 'book';
  /** Token ID */
  asset_id: string;
  /** Market condition ID */
  market: string;
  /** Updated bids */
  bids?: PolymarketOrderBookLevel[];
  /** Updated asks */
  asks?: PolymarketOrderBookLevel[];
  /** Timestamp */
  timestamp: number;
  /** Update hash */
  hash?: string;
}

/**
 * WebSocket trade event
 */
export type PolymarketWSTrade = PolymarketTrade & { event_type: 'trade' };

/**
 * WebSocket order update event
 */
export type PolymarketWSOrderUpdate = PolymarketAPIOrder & { event_type: 'order' };

/**
 * API key derivation request
 */
export interface PolymarketDeriveKeyRequest {
  /** Wallet address */
  address: string;
  /** Nonce for signing */
  nonce: number;
  /** Signature of the nonce */
  signature: string;
  /** Timestamp */
  timestamp: number;
}

/**
 * API key derivation response
 */
export interface PolymarketDeriveKeyResponse {
  /** API key */
  apiKey: string;
  /** Secret for HMAC */
  secret: string;
  /** Passphrase */
  passphrase: string;
}

/**
 * L2 authentication headers
 */
export interface PolymarketL2Headers {
  /** Wallet signer address */
  'POLY_ADDRESS': string;
  /** HMAC signature */
  'POLY_SIGNATURE': string;
  /** Timestamp */
  'POLY_TIMESTAMP': string;
  /** API key */
  'POLY_API_KEY': string;
  /** Passphrase */
  'POLY_PASSPHRASE'?: string;
}

/**
 * Exchange-specific options for Polymarket
 */
export interface PolymarketExchangeOptions {
  /** Optional explicit discovery scope; each market still exposes every outcome. */
  marketSlugs?: string[];
  /** Derived API key */
  apiKey?: string;
  /** API secret */
  apiSecret?: string;
  /** API passphrase */
  apiPassphrase?: string;
  /** Signature type (0 = EOA, 1 = Magic, 2 = Gnosis Safe, 3 = POLY_1271) */
  signerType?: SignatureType;
  /** Funder address if different from signer */
  funderAddress?: string;
}

/**
 * Market info for Gamma API endpoint
 */
export interface GammaMarketInfo {
  id: string;
  question: string;
  conditionId: string;
  slug: string;
  resolutionSource: string;
  endDate: string;
  liquidity: string;
  startDate: string;
  fee: string;
  description: string;
  outcomes: string | null;
  outcomePrices: string | null;
  volume: string;
  active: boolean;
  closed: boolean;
  marketType: string;
  clobTokenIds: string | null;
  orderPriceMinTickSize: number | string | null;
  orderMinSize: number | string | null;
  negRisk?: boolean | null;
  enableOrderBook?: boolean | null;
  acceptingOrders?: boolean | null;
}

/** Current holding returned by the public Data API. */
export interface PolymarketDataPosition {
  proxyWallet: string;
  asset: string;
  conditionId: string;
  size: number;
  avgPrice: number;
  currentValue: number;
  cashPnl: number;
  curPrice: number;
  redeemable: boolean;
  mergeable: boolean;
  title: string;
  slug: string;
  outcome: string;
  negativeRisk: boolean;
}
