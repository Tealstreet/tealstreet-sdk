// Literal production hosts: no tunnel, credentials, testnet or private sockets.
export const publicStreams = {
  lighter: { hosts: ['mainnet.zklighter.elliot.ai'], symbol: 'BTC', startupOnly: true },
  apexomni: { hosts: ['quote.omni.apex.exchange'], symbol: 'BTCUSDT' },
  binance: { hosts: ['fstream.binance.com'], symbol: 'BTCUSDT' },
  bybit: { hosts: ['stream.bybit.com'], symbol: 'BTCUSDT' },
  okx: { hosts: ['ws.okx.com'], symbol: 'BTCUSDT' },
  hyperliquid: { hosts: ['api-ui.hyperliquid.xyz'], symbol: 'BTC' },
  bitget: { hosts: ['ws.bitget.com'], symbol: 'BTCUSDT' },
  phemex: { hosts: ['ws.phemex.com'], symbol: 'BTCUSDT' },
  woo: { hosts: ['wss.woox.io'], symbol: 'BTCUSDT' },
  backpack: { hosts: ['ws.backpack.exchange'], symbol: 'BTC_USDC_PERP' },
  cryptocom: { hosts: ['stream.crypto.com'], symbol: 'BTCUSD-PERP' },
  bitunix: { hosts: ['fapi.bitunix.com', 'api.bitunix.com'], symbol: 'BTCUSDT' },
  kraken: { hosts: ['futures.kraken.com'], symbol: 'PI_XBTUSD' },
  mexc: { hosts: ['contract.mexc.com'], symbol: 'BTCUSDT' },
  bingx: { hosts: ['ws-market-swap.qq-os.com'], symbol: 'BTCUSDT' },
  coinbase: { hosts: ['advanced-trade-ws.coinbase.com'], symbol: 'BTCUSD' },
  grvt: { hosts: ['market-data.grvt.io'], symbol: 'BTCUSDT' },
};
export const tradeCapability =
  'The SDK has no standalone public trade listener. Bybit listenOHLCV subscribes to publicTrade and emits derived candles; other venues expose their production candle channel. No trade-event parity is claimed.';
