// Literal production hosts: no tunnel, credentials, testnet or private sockets.
export const publicStreams = {
  binance: { hosts: ['fstream.binance.com'], symbol: 'BTCUSDT' },
  bybit: { hosts: ['stream.bybit.com'], symbol: 'BTCUSDT' },
  okx: { hosts: ['ws.okx.com'], symbol: 'BTCUSDT' },
  hyperliquid: { hosts: ['api-ui.hyperliquid.xyz'], symbol: 'BTC' },
};
export const tradeCapability =
  'The SDK has no standalone public trade listener. Bybit listenOHLCV subscribes to publicTrade and emits derived candles; other venues expose their production candle channel. No trade-event parity is claimed.';
