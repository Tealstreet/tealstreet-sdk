/**
 * Lighter SDK Client Module
 *
 * Exports HTTP API client, transaction client, and related types.
 */

// Export API client
export { LighterApiClient } from './api-client';

// Export account API client
export {
  AccountApi,
  type AccountApiKeys,
  type AccountInfo,
  type AccountLimits,
  type AccountMetadata,
  type AccountsByL1Address,
  type ApiKeyInfo,
  type GetLiquidationsOptions,
  type GetPnlOptions,
  type Liquidation,
  type Liquidations,
  type PnL,
  type PnLDataPoint,
  type SubAccount,
} from './account-api';

// Export order API client
export {
  OrderApi,
  type AccountOrdersResponse,
  type ApiOrder,
  type ApiTrade,
  type ExchangeStats,
  type ExchangeStatsResponse,
  type GetInactiveOrdersOptions,
  type GetTradesOptions,
  type OrderBookDetailsResponse,
  type OrderBookOrdersResponse,
  type OrderBooksResponse,
  type RecentTradesResponse,
  type TradesResponse,
} from './order-api';

// Export transaction client
export {
  TransactionClient,
  type ApproveIntegratorParams,
  type CancelOrderParams,
  type CreateGroupedOrdersParams,
  type CreateOrderParams,
  type LighterGroupedOrderType,
  type TransferParams,
  type UpdateLeverageParams,
} from './transaction-client';

// Export transaction API
export {
  TransactionApi,
  TransactionStatus,
  TransactionType,
  type BlockTransactionsResponse,
  type Deposit,
  type DepositHistoryResponse,
  type GetAccountTransactionsOptions,
  type GetDepositHistoryOptions,
  type GetTransferHistoryOptions,
  type Transaction,
  type TransactionDetailResponse,
  type TransactionsResponse,
  type Transfer,
  type TransferHistoryResponse,
} from './transaction-api';

// Export candlestick API client
export {
  CandlestickApi,
  type Candlestick,
  type CandlesticksResponse,
  type Funding,
  type FundingsResponse,
} from './candlestick-api';

// Export funding API client
export { FundingApi, type FundingRate, type FundingRatesResponse } from './funding-api';

// Export info API client
export { InfoApi, type WithdrawalDelay, type WithdrawalDelayResponse } from './info-api';

// Export referral API client
export { ReferralApi, type ReferralPoints, type ReferralPointsResponse } from './referral-api';

// Export HTTP types
export {
  LighterApiError,
  type ApiErrorResponse,
  type ApiKey,
  type ApiResponse,
  type ErrorInterceptor,
  type GetApiKeysResponse,
  type GetNonceResponse,
  type GetTransferFeeResponse,
  type Interceptors,
  type L2CancelOrderTxInfo,
  type L2CreateGroupedOrdersTxInfo,
  type L2CreateOrderTxInfo,
  type L2ApproveIntegratorTxInfo,
  type L2TransferTxInfo,
  type L2UpdateLeverageTxInfo,
  type LighterGroupedOrderDetails,
  type LighterApiClientConfig,
  type RequestInterceptor,
  type ResponseInterceptor,
  type SendTxBatchResponse,
  type SendTxResponse,
  type TransferFeeInfo,
  type TxInfo,
} from './http-types';
