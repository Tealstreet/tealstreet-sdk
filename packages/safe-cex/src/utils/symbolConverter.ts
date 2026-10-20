import type { ExchangeName } from '../types';

// ---------------------------------------------------------------------------
// parseBinanceSymbol — the only parser needed
// ---------------------------------------------------------------------------

/** Known quote currencies, ordered longest-first for greedy matching */
const KNOWN_QUOTES = ['USDT', 'USDC', 'USDG', 'BUSD', 'FDUSD', 'USDY', 'USDD', 'USDP', 'USD', 'EUR'];

/**
 * Extract base/quote from a Binance-format symbol (concatenated, e.g., BTCUSDT).
 * This is the canonical intermediate format — all conversions go through this.
 */
export function parseBinanceSymbol(symbol: string): { base: string; quote: string } {
  for (const q of KNOWN_QUOTES) {
    if (symbol.endsWith(q) && symbol.length > q.length) {
      return { base: symbol.slice(0, -q.length), quote: q };
    }
  }
  // Fallback: whole thing is base, assume USDT
  return { base: symbol, quote: 'USDT' };
}

// ---------------------------------------------------------------------------
// toBinance — convert any exchange symbol to Binance canonical format
// ---------------------------------------------------------------------------

/**
 * Convert a symbol from any exchange to Binance canonical format (e.g., BTCUSDT).
 * Explicit case per exchange — TypeScript errors if a new exchange is added without a case.
 */
export function toBinance(symbol: string, from: ExchangeName): string {
  switch (from) {
    // -- Passthrough: already Binance format --------------------------------
    case 'binance':
    case 'binanceinverse':
    case 'binancepm':
    case 'binancespot':
    case 'bitget':
    case 'bitunix':
    case 'asterdex':
    case 'apexomni':
    case 'woo':
    case 'blofin':
    case 'breakoutprop':
    case 'bitgetinverse':
    case 'bybitspot':
    case 'woofi':
    case 'kodiakfi':
    case 'mode':
    case 'salsadex':
    case 'tealstreet-dex':
    case 'what':
    case 'weex':
      return symbol;

    // -- BingX: symbol is concat (BTCUSDT) ----------------------------------
    case 'bingx':
      return symbol;

    // -- Phemex: U1000SHIBUSDT → 1000SHIBUSDT (strip U prefix on multiplier symbols)
    case 'phemex': {
      // Phemex prefixes some symbols with U (e.g., U1000SHIBUSDT)
      if (symbol.startsWith('U') && symbol.length > 1 && /\d/.test(symbol[1])) {
        return symbol.slice(1);
      }
      return symbol;
    }

    // -- Bybit Inverse: BTCUSD or ETHUSDU26 (strip expiry suffix) -----------
    case 'bybitinverse': {
      // Match expiry codes like U26, H26, Z25 (single letter + 2 digits)
      return symbol.replace(/[FGHJKMNQUVXZ]\d{2}$/, '');
    }

    // -- OKX: strip special suffixes like _UM --------------------------------
    case 'okx':
    case 'okxdemo':
    case 'okxeu':
    case 'okxeudemo': {
      const idx = symbol.indexOf('_');
      return idx > 0 ? symbol.slice(0, idx) : symbol;
    }

    // -- Bybit/BybitDemo/HyroTrader: BTCUSDT passthrough, BCHPERP → BCHUSDC -
    case 'bybit':
    case 'bybitdemo':
    case 'hyrotrader': {
      // Strip expiry date: MNTUSDT-03APR26 → MNTUSDT
      const dashIdx = symbol.indexOf('-');
      const clean = dashIdx > 0 ? symbol.slice(0, dashIdx) : symbol;
      // PERP suffix means USDC perp
      if (clean.endsWith('PERP')) {
        return clean.slice(0, -4) + 'USDC';
      }
      return clean;
    }

    // -- BitgetUSDC: BTCPERP → BTCUSDC --------------------------------------
    case 'bitgetusdc': {
      if (symbol.endsWith('PERP')) {
        return symbol.slice(0, -4) + 'USDC';
      }
      return symbol;
    }

    // -- Bitrue: E-BTC-USDT → BTCUSDT ---------------------------------------
    case 'bitrue': {
      const stripped = symbol.startsWith('E-') ? symbol.slice(2) : symbol;
      return stripped.replace(/-/g, '');
    }

    // -- Coinbase perp: ETH-PERP-INTX → ETHUSDC -----------------------------
    case 'coinbase': {
      const perpIdx = symbol.indexOf('-PERP-INTX');
      if (perpIdx > 0) {
        return symbol.slice(0, perpIdx) + 'USDC';
      }
      return symbol.replace(/-/g, '');
    }

    // -- Coinbase spot: BTC-USD → BTCUSD ------------------------------------
    case 'coinbasespot':
      return symbol.replace(/-/g, '');

    // -- Paradex: BTC-USD-PERP → BTCUSD -------------------------------------
    case 'paradex': {
      const clean = symbol.endsWith('-PERP') ? symbol.slice(0, -5) : symbol;
      return clean.replace(/-/g, '');
    }

    // -- Hyperliquid: BTC-PERP → BTCUSDC, XYZ|NFLX-PERP → NFLXUSDC --------
    case 'hyperliquid': {
      // Strip vendor prefix
      const pipeIdx = symbol.indexOf('|');
      const clean = pipeIdx >= 0 ? symbol.slice(pipeIdx + 1) : symbol;
      // Strip -PERP, add USDC
      const base = clean.endsWith('-PERP') ? clean.slice(0, -5) : clean;
      return base + 'USDC';
    }

    // -- Hyperliquid spot: PURR/USDC → PURRUSDC -----------------------------
    case 'hyperliquidspot':
      return symbol.replace('/', '');

    // -- TxFlow: BTC-USDC / BTC-USDT → BTCUSDC / BTCUSDT -------------------
    case 'txflow':
      return symbol.replace(/-/g, '');

    // -- Kraken: PF_XBTUSD → BTCUSD ----------------------------------------
    case 'kraken': {
      let clean = symbol.startsWith('PF_') ? symbol.slice(3) : symbol;
      // XBT → BTC
      if (clean.startsWith('XBT')) {
        clean = 'BTC' + clean.slice(3);
      }
      return clean;
    }

    // -- Kraken Spot: BTC/USD → BTCUSD --------------------------------------
    case 'krakenspot':
      return symbol.replace('/', '');

    // -- KuCoin Spot: BTC-USDT → BTCUSDT -----------------------------------
    case 'kucoinspot':
      return symbol.replace(/-/g, '');

    // -- KuCoin: XBTUSDTM → BTCUSDT ----------------------------------------
    case 'kucoin': {
      // Strip trailing M
      let clean = symbol.endsWith('M') ? symbol.slice(0, -1) : symbol;
      // XBT → BTC
      if (clean.startsWith('XBT')) {
        clean = 'BTC' + clean.slice(3);
      }
      return clean;
    }

    // -- GRVT/Backpack: BTC_USDT_PERP → BTCUSDT ----------------------------
    case 'grvt':
    case 'backpack': {
      const clean = symbol.endsWith('_PERP') ? symbol.slice(0, -5) : symbol;
      return clean.replace(/_/g, '');
    }

    // -- MEXC futures: BTC_USDT → BTCUSDT ----------------------------------
    case 'mexc':
      return symbol.replace(/_/g, '');

    // -- Crypto.com: BTCUSD-PERP → BTCUSD ----------------------------------
    case 'cryptocom': {
      return symbol.endsWith('-PERP') ? symbol.slice(0, -5) : symbol;
    }

    // -- Lighter: APT → APTUSDC, SKY/USDC → SKYUSDC ------------------------
    case 'lighter': {
      if (symbol.includes('/')) {
        return symbol.replace('/', '');
      }
      // Perp symbols are base-only and USDC-margined.
      return symbol + 'USDC';
    }
    case 'lighter-robinhood': {
      if (symbol.includes('/')) {
        return symbol.replace('/', '');
      }
      // Robinhood Chain Lighter perps are base-only and USDG-margined.
      return symbol + 'USDG';
    }

    // -- Polymarket: non-standard, best effort ------------------------------
    case 'binanceoptions':
    case 'polymarket':
      return symbol;
  }

  // Exhaustive check: if a new ExchangeName is added, this line errors:
  // "Type 'newexchange' is not assignable to type 'never'"
  const _exhaustive: never = from;
  return _exhaustive;
}

// ---------------------------------------------------------------------------
// fromBinance — convert Binance canonical format to any exchange
// ---------------------------------------------------------------------------

/**
 * Convert a Binance-format symbol to the target exchange's format.
 * Explicit case per exchange — TypeScript errors if a new exchange is added without a case.
 */
export function fromBinance(symbol: string, to: ExchangeName): string {
  switch (to) {
    // -- Passthrough: Binance format is the native format -------------------
    case 'binance':
    case 'binanceinverse':
    case 'binancepm':
    case 'binancespot':
    case 'bitget':
    case 'bingx':
    case 'bitunix':
    case 'asterdex':
    case 'apexomni':
    case 'woo':
    case 'okx':
    case 'okxdemo':
    case 'okxeu':
    case 'okxeudemo':
    case 'blofin':
    case 'breakoutprop':
    case 'bitgetinverse':
    case 'bybitinverse':
    case 'bybitspot':
    case 'woofi':
    case 'kodiakfi':
    case 'mode':
    case 'salsadex':
    case 'tealstreet-dex':
    case 'what':
    case 'weex':
      return symbol;

    // -- Phemex: passthrough (U prefix only exists on API side, not in our format)
    case 'phemex':
      return symbol;

    // -- Bybit/BybitDemo/HyroTrader: BCHUSDC → BCHPERP, BTCUSDT stays -------
    case 'bybit':
    case 'bybitdemo':
    case 'hyrotrader': {
      if (symbol.endsWith('USDC')) {
        return symbol.slice(0, -4) + 'PERP';
      }
      return symbol;
    }

    // -- BitgetUSDC: BTCUSDC → BTCPERP --------------------------------------
    case 'bitgetusdc': {
      const { base } = parseBinanceSymbol(symbol);
      return base + 'PERP';
    }

    // -- Bitrue: BTCUSDT → E-BTC-USDT ---------------------------------------
    case 'bitrue': {
      const { base, quote } = parseBinanceSymbol(symbol);
      return `E-${base}-${quote}`;
    }

    // -- Coinbase perp: ETHUSDC → ETH-PERP-INTX ----------------------------
    case 'coinbase': {
      const { base } = parseBinanceSymbol(symbol);
      return `${base}-PERP-INTX`;
    }

    // -- Coinbase spot: BTCUSD → BTC-USD ------------------------------------
    case 'coinbasespot': {
      const { base, quote } = parseBinanceSymbol(symbol);
      return `${base}-${quote}`;
    }

    // -- Paradex: BTCUSD → BTC-USD-PERP ------------------------------------
    case 'paradex': {
      const { base, quote } = parseBinanceSymbol(symbol);
      return `${base}-${quote}-PERP`;
    }

    // -- Hyperliquid: BTCUSDC → BTC-PERP -----------------------------------
    case 'hyperliquid': {
      const { base } = parseBinanceSymbol(symbol);
      return `${base}-PERP`;
    }

    // -- Hyperliquid spot: PURRUSDC → PURR/USDC ----------------------------
    case 'hyperliquidspot': {
      const { base, quote } = parseBinanceSymbol(symbol);
      return `${base}/${quote}`;
    }

    // -- TxFlow: BTCUSDC / BTCUSDT → BTC-USDC / BTC-USDT -------------------
    case 'txflow': {
      const { base, quote } = parseBinanceSymbol(symbol);
      return `${base}-${quote}`;
    }

    // -- Kraken: BTCUSD → PF_XBTUSD ----------------------------------------
    case 'kraken': {
      const { base, quote } = parseBinanceSymbol(symbol);
      const krakenBase = base === 'BTC' ? 'XBT' : base;
      return `PF_${krakenBase}${quote}`;
    }

    // -- Kraken Spot: BTCUSD → BTC/USD -------------------------------------
    case 'krakenspot': {
      const { base, quote } = parseBinanceSymbol(symbol);
      return `${base}/${quote}`;
    }

    // -- KuCoin Spot: BTCUSDT → BTC-USDT -----------------------------------
    case 'kucoinspot': {
      const { base, quote } = parseBinanceSymbol(symbol);
      return `${base}-${quote}`;
    }

    // -- KuCoin: BTCUSDT → XBTUSDTM ----------------------------------------
    case 'kucoin': {
      const { base, quote } = parseBinanceSymbol(symbol);
      const kucoinBase = base === 'BTC' ? 'XBT' : base;
      return `${kucoinBase}${quote}M`;
    }

    // -- GRVT/Backpack: BTCUSDT → BTC_USDT_PERP ----------------------------
    case 'grvt':
    case 'backpack': {
      const { base, quote } = parseBinanceSymbol(symbol);
      return `${base}_${quote}_PERP`;
    }

    // -- MEXC futures: BTCUSDT → BTC_USDT ----------------------------------
    case 'mexc': {
      const { base, quote } = parseBinanceSymbol(symbol);
      return `${base}_${quote}`;
    }

    // -- Crypto.com: BTCUSD → BTCUSD-PERP ----------------------------------
    case 'cryptocom':
      return symbol + '-PERP';

    // -- Lighter: APTUSDC/APTUSDT → APT. Lighter spot uses slash symbols but the perps exchange filters spot out.
    case 'lighter': {
      return parseBinanceSymbol(symbol).base;
    }
    case 'lighter-robinhood': {
      return parseBinanceSymbol(symbol).base;
    }

    // -- Polymarket: non-standard, passthrough ------------------------------
    case 'binanceoptions':
    case 'polymarket':
      return symbol;
  }

  const _exhaustive: never = to;
  return _exhaustive;
}

// ---------------------------------------------------------------------------
// convertSymbol — cross-exchange via Binance intermediary
// ---------------------------------------------------------------------------

// Cache is intentionally unbounded but effectively capped by (symbols x exchange pairs)
// and used on hot paths to avoid repeated string parsing. The nested shape avoids
// allocating a composite `${symbol}:${from}:${to}` key on every cache hit.
const conversionCache = new Map<string, Map<ExchangeName, Map<ExchangeName, string>>>();

/**
 * Convert a symbol between any two exchanges.
 * Goes through Binance canonical format as the common denominator.
 *
 * convertSymbol(sym, from, to) = fromBinance(toBinance(sym, from), to)
 */
export function convertSymbol(symbol: string, from: ExchangeName, to: ExchangeName): string {
  if (from === to) return symbol;

  let fromCache = conversionCache.get(symbol);
  let toCache = fromCache?.get(from);
  const cached = toCache?.get(to);
  if (cached !== undefined) return cached;

  const binanceSymbol = toBinance(symbol, from);
  const result = fromBinance(binanceSymbol, to);

  if (!fromCache) {
    fromCache = new Map();
    conversionCache.set(symbol, fromCache);
  }
  if (!toCache) {
    toCache = new Map();
    fromCache.set(from, toCache);
  }
  toCache.set(to, result);
  return result;
}
