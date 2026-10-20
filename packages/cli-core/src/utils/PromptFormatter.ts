import type { ExchangeLike } from '../types';
import type { Market, Position, Order } from '@tealstreet/safe-cex/src/types';
import { OrderStatus, PositionSide } from '@tealstreet/safe-cex/src/types';
import { ConnectionMonitor } from './ConnectionMonitor';
import { formatBaseSize } from './formatBaseSize';
import { formatPriceCompact } from '@tealstreet/utils/src/formatting';
import chalk from 'chalk';

/**
 * Available prompt tokens that can be used in the format string
 *
 * Tokens are wrapped in {} and will be replaced with live data:
 *   {symbol}     - Current symbol (e.g., "BTCUSDT")
 *   {account}    - Account name
 *   {price}      - Current price (e.g., "$97,234")
 *   {side}       - Position side (colored "Long" or "Short", empty if no position)
 *   {size}       - Position size with currency (e.g., "0.5 BTC")
 *   {entry}      - Entry price (e.g., "$96,500")
 *   {pnl}        - Unrealized PnL (e.g., "+$234" or "-$50")
 *   {pnl%}       - Unrealized PnL as percentage (e.g., "+2.5%")
 *   {liq}        - Liquidation price (e.g., "$85,000")
 *   {leverage}   - Position leverage (e.g., "10x")
 *   {orders}     - Open order count (e.g., "3")
 *   {conn}       - Connection status indicator (e.g., "[!]", "[SLOW]", empty if ok)
 *   {health}     - Health status (empty if healthy, "[DEGRADED]" yellow, "[ERROR]" red)
 *   {version}    - CLI version (e.g., "v0.2.0")
 *
 * Default format: "{version} {conn}[{account} {symbol} {price} {side}{size}]> "
 *
 * Example formats:
 *   "{symbol}> "                                    - Minimal
 *   "[{account} {symbol} {price}]> "                - With account
 *   "[{account} {symbol} {price} {side}:{size} {pnl}]> " - With PnL
 *   "{version} [{account} {symbol}] {side} {entry}→{price} ({pnl%})> " - Detailed
 *   "{version} [{symbol}]> "                        - With version
 */
export type PromptToken =
  | 'symbol'
  | 'account'
  | 'price'
  | 'side'
  | 'size'
  | 'entry'
  | 'pnl'
  | 'pnl%'
  | 'liq'
  | 'leverage'
  | 'orders'
  | 'conn'
  | 'health'
  | 'version';

/**
 * Prompt configuration
 */
export interface PromptConfig {
  /** Format string with tokens (e.g., "[{symbol} {price}]> ") */
  format: string;
  /** Prefix before the format (e.g., "tealstreet") */
  prefix?: string;
  /** Whether to show account name after prefix */
  showAccountInPrefix?: boolean;
  /** Hide tokens that have no value (e.g., hide {pnl} when no position) */
  hideEmpty?: boolean;
}

/**
 * Default prompt configurations
 */
export const PROMPT_PRESETS: Record<string, PromptConfig> = {
  minimal: {
    format: '[{symbol}]> ',
    prefix: 'ts',
    hideEmpty: true,
  },
  default: {
    format: '{version} {conn}[{account} {symbol} {price} {side}{size}]> ',
    prefix: '',
    showAccountInPrefix: false,
    hideEmpty: true,
  },
  detailed: {
    format: '{version} {conn}[{account} {symbol} {price}] {side} {size} @ {entry} ({pnl}) [{orders}o]> ',
    prefix: '',
    showAccountInPrefix: false,
    hideEmpty: true,
  },
  trader: {
    format: '{version} {conn}[{account} {symbol}] {side} {size} entry:{entry} liq:{liq} pnl:{pnl%} [{orders}]> ',
    prefix: '',
    showAccountInPrefix: false,
    hideEmpty: true,
  },
  compact: {
    format: '[{account} {symbol} {price} {pnl%}]> ',
    prefix: '',
    hideEmpty: true,
  },
};

export interface PromptOptions {
  /** Account name */
  accountName?: string;
  /** Current symbol */
  symbol: string;
  /** Exchange instance */
  exchange: ExchangeLike | null;
  /** Prompt configuration (uses default if not provided) */
  config?: PromptConfig;
  /** CLI version (e.g., "0.2.0") */
  version?: string;
  /** Health status from ExchangeHealthMonitor ('healthy' | 'degraded' | 'error') */
  healthStatus?: 'healthy' | 'degraded' | 'error';
  /**
   * Connection lifecycle of the selected account, surfaced when there is no
   * live `exchange` yet. `connected`/undefined falls through to the normal
   * `{conn}` indicator; the other states render an explicit status word in
   * `{conn}` and tint the `{account}` token so the prompt never implies a
   * not-connected account is live.
   */
  connectionState?: 'connecting' | 'connected' | 'failed' | 'auth-failed';
  // Legacy options (for backward compatibility)
  /** @deprecated Use config.format instead */
  showConnectionStatus?: boolean;
  /** @deprecated Use config.format instead */
  showPrice?: boolean;
  /** @deprecated Use config.format instead */
  showPosition?: boolean;
  /** @deprecated Use config.prefix instead */
  prefix?: string;
}

export interface PromptInfo {
  /** The formatted prompt string */
  prompt: string;
  /** Current price (if available) */
  price?: number;
  /** Position side ('Long' or 'Short', null for none) */
  positionSide?: 'Long' | 'Short' | null;
  /** Position size in base units */
  positionSize?: number;
  /** Connection status indicator */
  connectionIndicator?: string;
  /** Unrealized PnL */
  unrealizedPnl?: number;
  /** Entry price */
  entryPrice?: number;
  /** Liquidation price */
  liquidationPrice?: number;
  /** Open order count */
  openOrderCount?: number;
}

/**
 * Track last prices per symbol for tick direction coloring
 */
const lastPrices: Map<string, number> = new Map();

/**
 * Token values for prompt formatting
 */
interface TokenValues {
  symbol: string;
  account: string;
  price: string;
  side: string;
  size: string;
  entry: string;
  pnl: string;
  'pnl%': string;
  liq: string;
  leverage: string;
  orders: string;
  conn: string;
  health: string;
  version: string;
}

/**
 * Format a CLI prompt with customizable tokens
 */
export function formatPrompt(options: PromptOptions): PromptInfo {
  const { accountName, symbol, exchange } = options;

  // Use config or build from legacy options
  const config: PromptConfig = options.config || buildLegacyConfig(options);

  const info: PromptInfo = { prompt: '' };

  // Gather all data
  const market = getMarket(exchange, symbol);
  const position = getFullPosition(exchange, symbol);
  const price = getCurrentPrice(exchange, symbol);
  const openOrders = getOpenOrders(exchange, symbol);
  const connStatus = exchange ? new ConnectionMonitor(exchange).getPromptStatus() : null;

  // Populate info object
  if (price !== null) info.price = price;
  if (position) {
    // Use the position's side property, not contracts sign
    const isLong = position.side === PositionSide.Long;
    info.positionSide = isLong ? 'Long' : 'Short';
    info.positionSize = Math.abs(position.contracts);
    info.unrealizedPnl = position.unrealizedPnl;
    info.entryPrice = position.entryPrice;
    info.liquidationPrice = position.liquidationPrice;
  }
  info.openOrderCount = openOrders.length;
  if (connStatus) info.connectionIndicator = connStatus.trim();

  // Build token values
  const pricePrecision = market?.precision?.price;
  const currency = market?.base;

  // When the selected account isn't connected yet, the `{conn}` slot carries
  // an explicit status word and the account name is tinted. Only applies when
  // there's no live `exchange` — once connected, the live ConnectionMonitor
  // status owns `{conn}` and the account renders normally.
  const effectiveConnState = exchange ? undefined : options.connectionState;
  const connStateToken = formatConnectionStateToken(effectiveConnState);

  const tokens: TokenValues = {
    symbol,
    account: tintAccountByState(accountName || '', effectiveConnState),
    price: price !== null ? formatPriceWithTick(price, symbol, pricePrecision) : '',
    side: position ? (position.side === PositionSide.Long ? chalk.green('Long:') : chalk.red('Short:')) : '',
    size: position
      ? (position.side === PositionSide.Long ? chalk.green : chalk.red)(
          `${formatBaseSize(position.contracts, market)}${currency ? ' ' + currency : ''}`,
        )
      : '',
    entry: position ? formatPriceCompact(position.entryPrice, pricePrecision) : '',
    pnl: position ? formatPnl(position.unrealizedPnl) : '',
    'pnl%':
      position && position.entryPrice
        ? formatPnlPercent(position.unrealizedPnl, position.entryPrice, Math.abs(position.contracts))
        : '',
    liq: position && position.liquidationPrice > 0 ? formatPriceCompact(position.liquidationPrice, pricePrecision) : '',
    leverage: position ? `${position.leverage}x` : '',
    orders: openOrders.length > 0 ? openOrders.length.toString() : '',
    conn: connStateToken || (connStatus ? connStatus.trim() + ' ' : ''),
    health: formatHealthToken(options.healthStatus),
    version: options.version ? `v${options.version}` : '',
  };

  // Build the prompt
  let prompt = config.prefix || '';

  // Add account name after prefix if configured — tint it by connection state
  // too, so the legacy prefix layout doesn't imply a not-connected account is
  // live (mirrors the `{account}` token tinting above).
  if (config.showAccountInPrefix && accountName) {
    prompt += `:${tintAccountByState(accountName, effectiveConnState)}`;
  }

  // Add space before format if we have a prefix
  if (prompt) {
    prompt += ' ';
  }

  // Replace tokens in format string
  let format = config.format;
  for (const [token, value] of Object.entries(tokens)) {
    const placeholder = `{${token}}`;
    if (config.hideEmpty && !value) {
      // Remove token and any surrounding whitespace/punctuation patterns
      format = format
        .replace(new RegExp(`\\s*${escapeRegex(placeholder)}\\s*`, 'g'), ' ')
        .replace(new RegExp(`${escapeRegex(placeholder)}:?`, 'g'), '');
    } else {
      format = format.replace(new RegExp(escapeRegex(placeholder), 'g'), value);
    }
  }

  // Clean up extra spaces and empty brackets
  format = format
    .replace(/\s+/g, ' ')
    .replace(/\[\s*\]/g, '')
    .replace(/\(\s*\)/g, '')
    .replace(/:\s*\]/g, ']')
    .replace(/\[\s+/g, '[')
    .replace(/\s+\]/g, ']')
    .trim();

  prompt += format;

  // Ensure prompt ends with space for input
  if (!prompt.endsWith(' ')) {
    prompt += ' ';
  }

  info.prompt = prompt;
  return info;
}

/**
 * Build a PromptConfig from legacy options (backward compatibility)
 */
function buildLegacyConfig(options: PromptOptions): PromptConfig {
  const { showConnectionStatus = true, showPrice = true, showPosition = true, prefix = 'tealstreet' } = options;

  let format = '';
  if (showConnectionStatus) format += '{conn}';
  format += '[{symbol}';
  if (showPrice) format += ' {price}';
  if (showPosition) format += ' {side}{size}';
  format += ']> ';

  return {
    format,
    prefix,
    showAccountInPrefix: true,
    hideEmpty: true,
  };
}

/**
 * Format price with tick direction coloring
 */
function formatPriceWithTick(price: number, symbol: string, precision?: number): string {
  const formatted = formatPriceCompact(price, precision);
  const lastPrice = lastPrices.get(symbol);
  lastPrices.set(symbol, price);

  if (lastPrice === undefined || price === lastPrice) {
    return formatted; // No color on first tick or no change
  }

  return price > lastPrice ? chalk.green(formatted) : chalk.red(formatted);
}

/**
 * Format PnL value with sign and color hint
 */
function formatPnl(pnl: number): string {
  const sign = pnl >= 0 ? '+' : '';
  return `${sign}$${Math.abs(pnl).toFixed(2)}`;
}

/**
 * Format PnL as percentage
 */
function formatPnlPercent(pnl: number, entryPrice: number, size: number): string {
  if (!entryPrice || !size) return '';
  const costBasis = entryPrice * size;
  if (costBasis === 0) return '';
  const percent = (pnl / costBasis) * 100;
  const sign = percent >= 0 ? '+' : '';
  return `${sign}${percent.toFixed(2)}%`;
}

/**
 * Format health status token for prompt display
 * healthy → empty, degraded → yellow [DEGRADED], error → red [ERROR]
 */
function formatHealthToken(status?: 'healthy' | 'degraded' | 'error'): string {
  if (!status || status === 'healthy') return '';
  if (status === 'degraded') return chalk.yellow('[DEGRADED]');
  return chalk.red('[ERROR]');
}

/**
 * Status word rendered into the `{conn}` slot when the selected account has
 * no live exchange. `connected`/undefined returns '' so the live connection
 * indicator (or nothing) is used instead.
 */
function formatConnectionStateToken(state?: 'connecting' | 'connected' | 'failed' | 'auth-failed'): string {
  switch (state) {
    case 'connecting':
      return chalk.yellow('[connecting]') + ' ';
    case 'failed':
      return chalk.red('[disconnected]') + ' ';
    case 'auth-failed':
      return chalk.red('[auth failed]') + ' ';
    default:
      return '';
  }
}

/**
 * Tint the account name by connection state: dim while connecting, red on
 * failure, untouched once connected (the caller's outer styling applies).
 */
function tintAccountByState(account: string, state?: 'connecting' | 'connected' | 'failed' | 'auth-failed'): string {
  if (!account) return '';
  switch (state) {
    case 'connecting':
      return chalk.dim(account);
    case 'failed':
    case 'auth-failed':
      return chalk.red(account);
    default:
      return account;
  }
}

/**
 * Escape special regex characters
 */
function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Get market info for a symbol
 */
function getMarket(exchange: ExchangeLike | null, symbol: string): Market | null {
  if (!exchange) return null;
  const markets = exchange.store?.markets;
  if (!markets) return null;

  return markets.find((m) => m.symbol === symbol) || null;
}

/**
 * Get current price for a symbol from exchange tickers
 */
function getCurrentPrice(exchange: ExchangeLike | null, symbol: string): number | null {
  if (!exchange) return null;
  const tickers = exchange.store?.tickers;
  if (!tickers) return null;

  const ticker = tickers.find((t) => t.symbol === symbol);
  if (!ticker) return null;

  // Prefer last price, fall back to mark
  return ticker.last || ticker.mark || null;
}

/**
 * Get full position info for a symbol
 */
function getFullPosition(exchange: ExchangeLike | null, symbol: string): Position | null {
  if (!exchange) return null;
  const positions = exchange.store?.positions;
  if (!positions) return null;

  return positions.find((p) => p.symbol === symbol && p.contracts !== 0) || null;
}

/**
 * Get open orders for a symbol
 */
function getOpenOrders(exchange: ExchangeLike | null, symbol: string): Order[] {
  if (!exchange) return [];
  const orders = exchange.store?.orders;
  if (!orders) return [];

  return orders.filter(
    (o) => o.symbol === symbol && (o.status === OrderStatus.Open || o.status === OrderStatus.Creating),
  );
}

/**
 * Simple prompt generator for basic usage
 */
export function simplePrompt(symbol: string, accountName?: string, prefix = 'tealstreet'): string {
  let prompt = prefix;
  if (accountName) {
    prompt += `:${accountName}`;
  }
  prompt += ` [${symbol}]> `;
  return prompt;
}

/**
 * Get a preset configuration by name
 */
export function getPromptPreset(name: string): PromptConfig | undefined {
  return PROMPT_PRESETS[name];
}

/**
 * List available preset names
 */
export function listPromptPresets(): string[] {
  return Object.keys(PROMPT_PRESETS);
}
