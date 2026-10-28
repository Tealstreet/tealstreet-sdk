import { formatPrice } from '@tealstreet/utils/src/formatting/formatPrice';
import { Candle, Order, OrderType, Position, PositionSide, Timeframe, type Market } from '@tealstreet/safe-cex/src/types';
import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { GREEN, RED, RESET, DIM, CYAN } from './colors';
import { formatBaseSize } from '../../utils/formatBaseSize';

/**
 * Chart line for position or order overlay
 */
export interface ChartLine {
  price: number;
  type: 'position' | 'limit' | 'stop' | 'tp' | 'sl';
  side: 'long' | 'short' | 'buy' | 'sell';
  label: string; // e.g., "Long 0.242 BTC +$46.47" or "Limit 0.227 BTC"
}

/**
 * Format position for chart display
 */
export function formatPositionLine(position: Position, market?: Market): ChartLine | null {
  if (position.contracts === 0) return null;

  const side = position.side === PositionSide.Long ? 'long' : 'short';
  const sizeStr = formatBaseSize(position.contracts, market);
  const pnlStr =
    position.unrealizedPnl >= 0
      ? `+$${position.unrealizedPnl.toFixed(2)}`
      : `-$${Math.abs(position.unrealizedPnl).toFixed(2)}`;

  return {
    price: position.entryPrice,
    type: 'position',
    side,
    label: `${side === 'long' ? 'Long' : 'Short'} ${sizeStr} ${pnlStr}`,
  };
}

/**
 * Format order for chart display
 * Returns null if order has no valid price (e.g., market orders)
 */
export function formatOrderLine(order: Order, _pricePrecision?: number): ChartLine | null {
  const side = order.side === 'buy' ? 'buy' : 'sell';
  const sizeStr = Math.abs(order.amount)
    .toFixed(4)
    .replace(/\.?0+$/, '');

  let type: ChartLine['type'] = 'limit';
  let label = `Limit ${sizeStr}`;
  let price = order.price;

  // Handle stop/TP/SL orders - use trigger price if available
  if (order.type === OrderType.StopLoss) {
    type = 'sl';
    label = `SL ${sizeStr}`;
    // Use stopLoss trigger price if available, otherwise fall back to order.price
    if (order.stopLoss && order.stopLoss > 0) {
      price = order.stopLoss;
    }
  } else if (order.type === OrderType.TakeProfit) {
    type = 'tp';
    label = `TP ${sizeStr}`;
    // Use takeProfit trigger price if available, otherwise fall back to order.price
    if (order.takeProfit && order.takeProfit > 0) {
      price = order.takeProfit;
    }
  } else if (order.type === OrderType.TrailingStopLoss) {
    type = 'stop';
    label = `TSL ${sizeStr}`;
  } else if (order.type === OrderType.Market) {
    // Market orders have no price, skip them
    return null;
  }

  // Skip orders with no valid price
  if (!price || price <= 0) {
    return null;
  }

  return {
    price,
    type,
    side,
    label,
  };
}

// Valid timeframes for the chart command
export const VALID_TIMEFRAMES: Timeframe[] = [
  '1m',
  '3m',
  '5m',
  '15m',
  '30m',
  '1h',
  '2h',
  '4h',
  '6h',
  '12h',
  '1d',
  '1w',
];

// Timeframe to milliseconds mapping (exported for watch mode)
export const TIMEFRAME_MS: Record<Timeframe, number> = {
  '1m': 60 * 1000,
  '3m': 3 * 60 * 1000,
  '5m': 5 * 60 * 1000,
  '15m': 15 * 60 * 1000,
  '30m': 30 * 60 * 1000,
  '1h': 60 * 60 * 1000,
  '2h': 2 * 60 * 60 * 1000,
  '4h': 4 * 60 * 60 * 1000,
  '6h': 6 * 60 * 60 * 1000,
  '12h': 12 * 60 * 60 * 1000,
  '1d': 24 * 60 * 60 * 1000,
  '1w': 7 * 24 * 60 * 60 * 1000,
};

/**
 * Format a countdown duration in human readable format
 * Exported for use in watch mode
 */
export function formatCountdown(ms: number): string {
  if (ms <= 0) return '0s';

  const seconds = Math.floor(ms / 1000) % 60;
  const minutes = Math.floor(ms / (60 * 1000)) % 60;
  const hours = Math.floor(ms / (60 * 60 * 1000)) % 24;
  const days = Math.floor(ms / (24 * 60 * 60 * 1000));

  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m ${seconds}s`;
  return `${seconds}s`;
}

/**
 * Get color for a chart line based on type and side
 */
function getLineColor(line: ChartLine): string {
  if (line.type === 'position') {
    return line.side === 'long' ? GREEN : RED;
  }
  if (line.type === 'tp') return GREEN;
  if (line.type === 'sl' || line.type === 'stop') return RED;
  // Limit orders - cyan like TradingView
  return CYAN;
}

/**
 * Render an ASCII candlestick chart from OHLCV data
 * Exported for use in watch mode
 * @param candles - Array of candle data
 * @param chartHeight - Height of the chart in rows (default 15)
 * @param pricePrecision - Number of decimal places for prices (from market.precision.price)
 * @param lastPrice - Current last price to show with pointer (optional, for watch mode)
 * @param countdown - Countdown string to show below price (optional, e.g. "01:52")
 * @param priceUp - Whether the price is up (green) or down (red) for the pointer color
 * @param chartLines - Position and order lines to draw on the chart
 */
export function renderCandleChart(
  candles: Candle[],
  chartHeight: number = 15,
  pricePrecision?: number,
  lastPrice?: number,
  countdown?: string,
  priceUp?: boolean,
  chartLines?: ChartLine[],
): string {
  if (candles.length === 0) {
    return 'No candle data available';
  }

  // Find price range
  let minPrice = Infinity;
  let maxPrice = -Infinity;
  for (const c of candles) {
    if (c.low < minPrice) minPrice = c.low;
    if (c.high > maxPrice) maxPrice = c.high;
  }

  // Include lastPrice in the range if provided
  if (lastPrice !== undefined) {
    if (lastPrice < minPrice) minPrice = lastPrice;
    if (lastPrice > maxPrice) maxPrice = lastPrice;
  }

  // Include chart lines in the price range (so position/orders are visible)
  if (chartLines) {
    for (const line of chartLines) {
      if (line.price < minPrice) minPrice = line.price;
      if (line.price > maxPrice) maxPrice = line.price;
    }
  }

  const priceRange = maxPrice - minPrice;
  if (priceRange === 0) {
    return 'Price range is zero - cannot render chart';
  }

  // Create chart grid (rows x cols)
  // Each column represents one candle
  const rows: string[][] = [];
  for (let i = 0; i < chartHeight; i++) {
    rows.push(new Array(candles.length).fill(' '));
  }

  // Map price to row index (0 = top = max price, chartHeight-1 = bottom = min price)
  const priceToRow = (price: number): number => {
    const normalized = (maxPrice - price) / priceRange;
    return Math.min(chartHeight - 1, Math.max(0, Math.floor(normalized * (chartHeight - 1))));
  };

  // Calculate lastPrice row for pointer
  const lastPriceRow = lastPrice !== undefined ? priceToRow(lastPrice) : -1;

  // Pre-calculate chart line rows for quick lookup
  const linesByRow: Map<number, ChartLine[]> = new Map();
  if (chartLines) {
    for (const line of chartLines) {
      const row = priceToRow(line.price);
      if (!linesByRow.has(row)) {
        linesByRow.set(row, []);
      }
      linesByRow.get(row)!.push(line);
    }
  }

  // Draw each candle
  for (let col = 0; col < candles.length; col++) {
    const c = candles[col];
    const isGreen = c.close >= c.open;

    const highRow = priceToRow(c.high);
    const lowRow = priceToRow(c.low);
    const openRow = priceToRow(c.open);
    const closeRow = priceToRow(c.close);

    const bodyTop = Math.min(openRow, closeRow);
    const bodyBottom = Math.max(openRow, closeRow);

    // Draw the candle from top to bottom
    for (let row = highRow; row <= lowRow; row++) {
      if (row >= bodyTop && row <= bodyBottom) {
        // Body
        if (isGreen) {
          rows[row][col] = 'G█'; // Green body marker
        } else {
          rows[row][col] = 'R█'; // Red body marker
        }
      } else {
        // Wick
        if (isGreen) {
          rows[row][col] = 'G│'; // Green wick marker
        } else {
          rows[row][col] = 'R│'; // Red wick marker
        }
      }
    }
  }

  // Build output string with price axis
  const lines: string[] = [];

  // Calculate label width for alignment - use formatPrice for consistent formatting
  const maxLabel = formatPrice(maxPrice, pricePrecision);
  const labelWidth = maxLabel.length;

  for (let row = 0; row < chartHeight; row++) {
    // Calculate price at this row
    const price = maxPrice - (row / (chartHeight - 1)) * priceRange;
    const priceLabel = formatPrice(price, pricePrecision).padStart(labelWidth);

    // Only show price labels at top, middle, and bottom
    let showLabel = row === 0 || row === chartHeight - 1 || row === Math.floor(chartHeight / 2);

    // Check if there's a chart line at this row
    const linesAtRow = linesByRow.get(row);
    const hasLine = linesAtRow && linesAtRow.length > 0;

    let rowStr = '';
    if (showLabel) {
      rowStr = `${DIM}${priceLabel}${RESET} ┤`;
    } else {
      rowStr = ' '.repeat(labelWidth) + ' │';
    }

    // Add candle characters with colors (with spacing between candles)
    // If there's a position/order line at this row, use dotted line for empty spaces
    for (let col = 0; col < candles.length; col++) {
      const cell = rows[row][col];
      if (cell.startsWith('G')) {
        rowStr += GREEN + cell[1] + RESET + ' ';
      } else if (cell.startsWith('R')) {
        rowStr += RED + cell[1] + RESET + ' ';
      } else if (hasLine) {
        // Draw dotted line through empty space
        const lineColor = getLineColor(linesAtRow![0]);
        rowStr += `${lineColor}·${RESET} `;
      } else {
        rowStr += cell + ' ';
      }
    }

    // Add chart line labels on the right side
    if (hasLine) {
      const line = linesAtRow![0]; // Show first line (position takes priority)
      const lineColor = getLineColor(line);
      rowStr += `${lineColor}◄ ${line.label}${RESET}`;
    } else if (lastPrice !== undefined && row === lastPriceRow) {
      // Add last price pointer on the right side (price row)
      const formattedPrice = formatPrice(lastPrice, pricePrecision);
      const priceColor = priceUp ? GREEN : RED;
      rowStr += `${priceColor}◄ ${formattedPrice}${RESET}`;
    } else if (countdown && lastPrice !== undefined && row === lastPriceRow + 1 && lastPriceRow + 1 < chartHeight) {
      // Add countdown below the price pointer (white color)
      const padding = ' '.repeat(2); // "◄ " is 2 chars
      rowStr += `${padding}${countdown}`;
    }

    lines.push(rowStr);
  }

  // Add bottom axis (accounting for spacing between candles)
  const bottomLine = ' '.repeat(labelWidth) + ' └' + '─'.repeat(candles.length * 2);
  lines.push(bottomLine);

  // Add time labels (first, middle, last)
  // Note: candle timestamps are in seconds, so convert to ms
  if (candles.length >= 3) {
    const firstTime = new Date(candles[0].timestamp * 1000).toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
    });
    const lastTime = new Date(candles[candles.length - 1].timestamp * 1000).toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
    });

    // Account for spacing (2 chars per candle now)
    const totalWidth = candles.length * 2;
    const timeLabelsLine =
      ' '.repeat(labelWidth + 2) +
      DIM +
      firstTime +
      ' '.repeat(Math.max(0, totalWidth - firstTime.length - lastTime.length)) +
      lastTime +
      RESET;
    lines.push(timeLabelsLine);
  }

  return lines.join('\n');
}

/**
 * Format chart header with price info
 * @param pricePrecision - Number of decimal places for prices (from market.precision.price)
 * @param lastCandleTimestamp - Timestamp of the last candle (for countdown calculation)
 */
export function formatChartHeader(
  symbol: string,
  timeframe: string,
  candleCount: number,
  lastPrice: number,
  firstClose: number,
  pricePrecision?: number,
  lastCandleTimestamp?: number,
): string {
  const change = ((lastPrice - firstClose) / firstClose) * 100;
  const changeSign = change >= 0 ? '+' : '';
  const changeColor = change >= 0 ? GREEN : RED;

  let header = `${symbol} ${timeframe} (${candleCount} candles) | Last: ${formatPrice(lastPrice, pricePrecision)} ${changeColor}${changeSign}${change.toFixed(2)}%${RESET}`;

  // Add countdown if we have timestamp and valid timeframe
  // Note: candle timestamps are in seconds, so convert to ms
  if (lastCandleTimestamp && timeframe in TIMEFRAME_MS) {
    const tfMs = TIMEFRAME_MS[timeframe as Timeframe];
    const candleEndTime = lastCandleTimestamp * 1000 + tfMs;
    const now = Date.now();
    const remaining = candleEndTime - now;

    if (remaining > 0) {
      header += ` | ${DIM}closes in ${CYAN}${formatCountdown(remaining)}${RESET}`;
    }
  }

  return header;
}

/**
 * Format a position summary line for display above the chart
 * Shows position info even if entry price is outside the visible chart range
 */
export function formatPositionSummary(position: Position | null | undefined, market?: Market): string | null {
  if (!position || position.contracts === 0) return null;

  const side = position.side === PositionSide.Long ? 'Long' : 'Short';
  const sideColor = position.side === PositionSide.Long ? GREEN : RED;
  const sizeStr = formatBaseSize(position.contracts, market);
  const entryStr = formatPrice(position.entryPrice, market?.precision?.price ?? 2);
  const pnlColor = position.unrealizedPnl >= 0 ? GREEN : RED;
  const pnlStr =
    position.unrealizedPnl >= 0
      ? `+$${position.unrealizedPnl.toFixed(2)}`
      : `-$${Math.abs(position.unrealizedPnl).toFixed(2)}`;

  return `${sideColor}${side}${RESET} ${sizeStr} @ ${entryStr} | PnL: ${pnlColor}${pnlStr}${RESET}`;
}

/**
 * Chart watch mode parameters - returned when watch mode is requested
 */
export interface ChartWatchParams {
  symbol: string;
  timeframe: Timeframe;
  limit: number;
}

export class ChartCommand extends TradingBaseCommand {
  readonly name = 'chart';
  readonly description = 'Display ASCII candlestick chart';
  readonly category = CommandCategory.Trading;
  readonly aliases: string[] = [];
  readonly examples = [
    'chart - Show 30 candles with 1h timeframe',
    'chart 15m - Show 30 candles with 15m timeframe',
    'chart 4h 50 - Show 50 candles with 4h timeframe',
    'chart 1d ETHUSDT - Show daily chart for ETHUSDT',
    'watch chart 5m - Live updating chart (q to stop)',
  ];
  readonly requiresConnection = true;

  /**
   * Parse timeframe from argument
   */
  private parseTimeframe(arg: string): Timeframe | null {
    const tf = arg.toLowerCase() as Timeframe;
    if (VALID_TIMEFRAMES.includes(tf)) {
      return tf;
    }
    return null;
  }

  /**
   * Parse chart parameters from args - can be used by CLI for watch mode
   */
  parseChartParams(ctx: CommandContext, args: string[]): ChartWatchParams | { error: string } {
    const exchange = ctx.exchange;
    if (!exchange) {
      return { error: 'Not connected to exchange' };
    }

    let timeframe: Timeframe = '1h';
    let limit = 30;
    let targetSymbol = ctx.symbol;

    // Parse arguments
    for (let i = 1; i < args.length; i++) {
      const arg = args[i];

      // Check if it's a timeframe
      const tf = this.parseTimeframe(arg);
      if (tf) {
        timeframe = tf;
        continue;
      }

      // Check if it's a number (limit)
      if (/^\d+$/.test(arg)) {
        const num = parseInt(arg, 10);
        if (num > 0 && num <= 100) {
          limit = num;
        }
        continue;
      }
    }

    // Check for explicit symbol
    targetSymbol = this.getTargetSymbol(ctx, ['chart']);

    // Transform symbol to exchange format
    targetSymbol = this.transformSymbol(exchange, targetSymbol);

    // Validate symbol exists
    if (!this.validateSymbol(exchange, targetSymbol)) {
      return { error: `Unknown symbol: ${targetSymbol}` };
    }

    return { symbol: targetSymbol, timeframe, limit };
  }

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const exchange = ctx.exchange;
    if (!exchange) {
      return this.error('Not connected to exchange');
    }
    if (!exchange.fetchOHLCV) {
      return this.error('Exchange does not support OHLCV data');
    }

    const args = this.getRawArgs(ctx);

    // Parse chart parameters
    const params = this.parseChartParams(ctx, args);
    if ('error' in params) {
      return this.error(params.error);
    }

    const { symbol: targetSymbol, timeframe, limit } = params;

    try {
      // Fetch candles
      const candles = await exchange.fetchOHLCV({
        symbol: targetSymbol,
        interval: timeframe,
        limit,
      });

      if (!candles || candles.length === 0) {
        return this.error(`No candle data available for ${targetSymbol}`);
      }

      // Get market precision
      const market = exchange.store?.markets?.find((m) => m.symbol === targetSymbol);
      const pricePrecision = market?.precision?.price ?? 2;

      // Get current price info
      const ticker = exchange.store?.tickers?.find((t) => t.symbol === targetSymbol);
      const lastPrice = ticker?.last ?? candles[candles.length - 1].close;
      const firstClose = candles[0].close;

      // Get position and orders for chart lines
      const position = exchange.store?.positions?.find((p) => p.symbol === targetSymbol);
      const orders = exchange.store?.orders?.filter((o) => o.symbol === targetSymbol && o.status === 'open') ?? [];

      // Build chart lines from position and orders
      const chartLines: ChartLine[] = [];
      const positionLine = position ? formatPositionLine(position, market) : null;
      if (positionLine) {
        chartLines.push(positionLine);
      }
      for (const order of orders) {
        const orderLine = formatOrderLine(order, pricePrecision);
        if (orderLine) {
          chartLines.push(orderLine);
        }
      }

      // Build header and render chart
      const lastCandle = candles[candles.length - 1];
      const header = formatChartHeader(
        targetSymbol,
        timeframe,
        candles.length,
        lastPrice,
        firstClose,
        pricePrecision,
        lastCandle.timestamp,
      );

      // Position summary line (shows even if outside chart range)
      const positionSummary = formatPositionSummary(position, market);

      // Calculate countdown for chart
      let countdown: string | undefined;
      if (timeframe in TIMEFRAME_MS) {
        const tfMs = TIMEFRAME_MS[timeframe as Timeframe];
        const candleEndTime = lastCandle.timestamp * 1000 + tfMs;
        const remaining = candleEndTime - Date.now();
        if (remaining > 0) {
          countdown = formatCountdown(remaining);
        }
      }

      // Determine if price is up (based on last candle)
      const priceUp = lastCandle.close >= lastCandle.open;
      const chart = renderCandleChart(candles, 15, pricePrecision, lastPrice, countdown, priceUp, chartLines);

      // Build output with optional position summary
      const outputLines = [header];
      if (positionSummary) {
        outputLines.push(positionSummary);
      }
      outputLines.push('');
      outputLines.push(chart);

      this.print(ctx, outputLines);
      return this.success();
    } catch (error) {
      return this.error(`Error fetching candles: ${(error as Error).message}`);
    }
  }
}
