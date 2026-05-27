import { formatPrice } from '@tealstreet/utils/src/formatting/formatPrice';
import { OrderSide, OrderStatus, PositionSide } from '@tealstreet/safe-cex/src/types';
import { ChaserOrderInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance';
import { chaserOrderManagerInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager';
import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import type { ExchangeLike } from '../../types';
import { GREEN, RED, YELLOW, CYAN, RESET, DIM, BOLD } from './colors';

/**
 * Chase watch mode parameters - returned when watch mode is requested
 */
export interface ChaseWatchParams {
  chaserIds: string[];
}

/**
 * Get chaser instance by ID from the manager
 */
export function getChaserInstance(id: string): ChaserOrderInstance | undefined {
  return chaserOrderManagerInstance.instances.find((instance) => instance.id === id);
}

/**
 * Get all active chaser instances
 */
export function getActiveChasers(): ChaserOrderInstance[] {
  return chaserOrderManagerInstance.instances.filter((instance) => !instance.isStopped);
}

/**
 * Render a mini tick chart showing recent prices and order position
 * @param priceHistory - Array of recent prices (newest last)
 * @param orderPrice - Current order price (or undefined if no order)
 * @param side - Order side (buy/sell)
 * @param chartWidth - Width of the chart in characters
 * @param chartHeight - Height of the chart in rows
 * @param pricePrecision - Price precision for labels
 */
export function renderMiniTickChart(
  priceHistory: number[],
  orderPrice: number | undefined,
  side: OrderSide,
  chartWidth: number = 40,
  chartHeight: number = 8,
  pricePrecision?: number,
): string {
  if (priceHistory.length < 2) {
    return DIM + '  Waiting for price data...' + RESET;
  }

  // Find price range including order price
  let minPrice = Math.min(...priceHistory);
  let maxPrice = Math.max(...priceHistory);

  if (orderPrice !== undefined) {
    minPrice = Math.min(minPrice, orderPrice);
    maxPrice = Math.max(maxPrice, orderPrice);
  }

  // Add small padding to range
  const padding = (maxPrice - minPrice) * 0.1 || maxPrice * 0.001;
  minPrice -= padding;
  maxPrice += padding;

  const priceRange = maxPrice - minPrice;
  if (priceRange === 0) {
    return DIM + '  No price movement' + RESET;
  }

  // Map price to row (0 = top = max, chartHeight-1 = bottom = min)
  const priceToRow = (price: number): number => {
    const normalized = (maxPrice - price) / priceRange;
    return Math.min(chartHeight - 1, Math.max(0, Math.floor(normalized * (chartHeight - 1))));
  };

  // Create chart grid
  const grid: string[][] = [];
  for (let i = 0; i < chartHeight; i++) {
    grid.push(new Array(chartWidth).fill(' '));
  }

  // Calculate order row if we have an order
  const orderRow = orderPrice !== undefined ? priceToRow(orderPrice) : -1;

  // Draw order line (dashed) across full width
  if (orderRow >= 0 && orderRow < chartHeight) {
    const orderChar = side === OrderSide.Buy ? '▸' : '◂';
    for (let col = 0; col < chartWidth; col++) {
      grid[orderRow][col] = col % 2 === 0 ? '─' : ' ';
    }
    // Mark order position
    grid[orderRow][0] = orderChar;
  }

  // Sample prices to fit chart width
  const step = Math.max(1, Math.floor(priceHistory.length / chartWidth));
  const sampledPrices: number[] = [];
  for (let i = 0; i < priceHistory.length; i += step) {
    sampledPrices.push(priceHistory[i]);
  }
  // Always include the latest price
  if (sampledPrices[sampledPrices.length - 1] !== priceHistory[priceHistory.length - 1]) {
    sampledPrices.push(priceHistory[priceHistory.length - 1]);
  }

  // Draw price line
  for (let col = 0; col < Math.min(sampledPrices.length, chartWidth); col++) {
    const price = sampledPrices[col];
    const row = priceToRow(price);

    // Use different character if on order line
    if (row === orderRow) {
      grid[row][col] = side === OrderSide.Buy ? GREEN + '●' + RESET : RED + '●' + RESET;
    } else {
      grid[row][col] = '●';
    }

    // Connect to previous point if there's a gap
    if (col > 0) {
      const prevRow = priceToRow(sampledPrices[col - 1]);
      const minRow = Math.min(row, prevRow);
      const maxRow = Math.max(row, prevRow);
      for (let r = minRow + 1; r < maxRow; r++) {
        if (grid[r][col] === ' ' || grid[r][col] === '─') {
          grid[r][col] = '│';
        }
      }
    }
  }

  // Mark latest price with arrow
  const latestPrice = priceHistory[priceHistory.length - 1];
  const latestRow = priceToRow(latestPrice);
  const latestCol = Math.min(sampledPrices.length - 1, chartWidth - 1);
  grid[latestRow][latestCol] = CYAN + '◀' + RESET;

  // Build output with price axis
  const lines: string[] = [];
  const maxLabel = formatPrice(maxPrice, pricePrecision);
  const labelWidth = Math.max(maxLabel.length, 8);

  for (let row = 0; row < chartHeight; row++) {
    let rowStr = '';

    // Price labels at top, middle, bottom
    if (row === 0) {
      rowStr = DIM + formatPrice(maxPrice, pricePrecision).padStart(labelWidth) + RESET + ' ┤';
    } else if (row === chartHeight - 1) {
      rowStr = DIM + formatPrice(minPrice, pricePrecision).padStart(labelWidth) + RESET + ' ┤';
    } else if (row === Math.floor(chartHeight / 2)) {
      const midPrice = (maxPrice + minPrice) / 2;
      rowStr = DIM + formatPrice(midPrice, pricePrecision).padStart(labelWidth) + RESET + ' ┤';
    } else {
      rowStr = ' '.repeat(labelWidth) + ' │';
    }

    // Add chart content
    for (let col = 0; col < chartWidth; col++) {
      rowStr += grid[row][col];
    }

    lines.push('  ' + rowStr);
  }

  // Add legend
  const orderLabel =
    orderPrice !== undefined
      ? `${side === OrderSide.Buy ? GREEN : RED}▸${RESET} Order: ${formatPrice(orderPrice, pricePrecision)}`
      : '';
  const lastLabel = `${CYAN}◀${RESET} Last: ${formatPrice(latestPrice, pricePrecision)}`;
  lines.push('  ' + ' '.repeat(labelWidth + 2) + DIM + '└' + '─'.repeat(chartWidth) + RESET);
  lines.push('  ' + ' '.repeat(labelWidth + 2) + orderLabel + (orderLabel ? '  ' : '') + lastLabel);

  return lines.join('\n');
}

/**
 * Format a single chaser's status for display
 * @param priceHistory - Optional price history for mini chart (from CLI tracking)
 */
export function formatChaserStatus(
  instance: ChaserOrderInstance,
  exchange: ExchangeLike,
  priceHistory?: number[],
): string {
  const { options, state, remainingSize } = instance;
  const filledSize = options.size - remainingSize;
  const fillPercent = options.size > 0 ? (filledSize / options.size) * 100 : 0;

  const market = exchange.store?.markets?.find((m) => m.symbol === options.symbol);
  const ticker = exchange.store?.tickers?.find((t) => t.symbol === options.symbol);
  const pricePrecision = market?.precision?.price;
  const amountPrecision = market?.precision?.amount;

  // Get current order info
  const currentOrders = exchange.store?.orders?.filter((o) => instance.currentOrderIds.includes(o.id)) ?? [];
  const openOrder = currentOrders.find((o) => o.status === OrderStatus.Open);

  const sideColor = options.side === OrderSide.Buy ? GREEN : RED;
  const sideStr = options.side === OrderSide.Buy ? 'BUY' : 'SELL';

  // Build progress bar
  const barWidth = 20;
  const filledBars = Math.round((fillPercent / 100) * barWidth);
  const progressBar = GREEN + '█'.repeat(filledBars) + RESET + DIM + '░'.repeat(barWidth - filledBars) + RESET;

  let lines: string[] = [];

  // Header
  lines.push(
    `${BOLD}${options.symbol}${RESET} ${sideColor}${sideStr}${RESET}${options.reduceOnly ? ` ${DIM}(reduce)${RESET}` : ''}`,
  );

  // Mini chart if we have price history
  if (priceHistory && priceHistory.length > 0) {
    lines.push(renderMiniTickChart(priceHistory, openOrder?.price, options.side, 40, 6, pricePrecision));
    lines.push('');
  }

  // Progress
  lines.push(`  Progress: [${progressBar}] ${fillPercent.toFixed(1)}%`);

  // Size info
  const filledStr = formatPrice(filledSize, amountPrecision);
  const totalStr = formatPrice(options.size, amountPrecision);
  const remainingStr = formatPrice(remainingSize, amountPrecision);
  lines.push(`  Filled:   ${GREEN}${filledStr}${RESET} / ${totalStr}  ${DIM}(${remainingStr} remaining)${RESET}`);

  // Current order
  if (openOrder) {
    const orderPrice = formatPrice(openOrder.price || 0, pricePrecision);
    lines.push(`  Order:    ${CYAN}${orderPrice}${RESET} @ ${formatPrice(openOrder.amount, amountPrecision)}`);
  } else if (!instance.isStopped) {
    lines.push(`  Order:    ${YELLOW}placing...${RESET}`);
  }

  // Market price
  if (ticker?.last) {
    lines.push(`  Market:   ${formatPrice(ticker.last, pricePrecision)}`);
  }

  // Attempts
  lines.push(`  Attempts: ${state.attempts}`);

  // Status
  if (instance.isStopped) {
    const statusColor =
      state.stopReason.includes('Completed') || state.stopReason.includes('Fully filled') ? GREEN : RED;
    lines.push(`  Status:   ${statusColor}${state.stopReason || 'Stopped'}${RESET}`);
  } else {
    lines.push(`  Status:   ${GREEN}Active${RESET}`);
  }

  return lines.join('\n');
}

/**
 * Format all active chasers for watch mode display
 * @param priceHistoryMap - Map of chaser ID to price history array
 */
export function formatAllChasersStatus(priceHistoryMap?: Map<string, number[]>): string {
  const activeChasers = chaserOrderManagerInstance.instances;

  if (activeChasers.length === 0) {
    return 'No active chasers';
  }

  const timestamp = new Date().toLocaleTimeString();
  let output = '\x1b[2J\x1b[H'; // Clear screen and move cursor to top
  output += `═══════════════════════════════════════════════════════════════════════════════\n`;
  output += `  CHASE MONITOR                                              ${timestamp}\n`;
  output += `═══════════════════════════════════════════════════════════════════════════════\n\n`;

  for (const instance of activeChasers) {
    const priceHistory = priceHistoryMap?.get(instance.id);
    // Cast to ExchangeLike - Exchange has all the required properties
    output += formatChaserStatus(instance, instance.exchange as unknown as ExchangeLike, priceHistory) + '\n';
    output += `───────────────────────────────────────────────────────────────────────────────\n`;
  }

  const activeCount = activeChasers.filter((c) => !c.isStopped).length;
  const completedCount = activeChasers.filter((c) => c.isStopped).length;
  output += `\n${DIM}Active: ${activeCount}  Completed: ${completedCount}${RESET}\n`;
  output += '\nPress Escape or Ctrl+X to exit (chasers will continue in background)\n';

  return output;
}

export class ChaseCommand extends TradingBaseCommand {
  readonly name = 'chase';
  readonly description = 'Chases the price to execute order at best bid/ask';
  readonly category = CommandCategory.Trading;
  readonly aliases: string[] = [];
  readonly examples = [
    'chase buy $100',
    'chase buy $100 ETHUSDT',
    'chase buy $100 reduce',
    'chase buy WIFUSDT $100',
    'chase buy WIFUSDT,ENAUSDT $100',
    'chase sell $120',
    'chase sell $120 ETHUSDT',
    'chase sell %all% 20% reduce',
    // Phase 3: terminate-at length
    'chase buy $100 to 49500',
    'chase buy $100 to 1%',
    'chase sell $100 to $50',
    'chase cancel',
    'chase cancel keep',
    'watch chase - Monitor all active chasers (q to stop watching)',
  ];
  readonly requiresConnection = true;

  /**
   * Create a chaser and return its ID. Throws on fatfinger overage so
   * the caller's `try/catch` can surface a clean command error.
   */
  protected async chaseBuySellSymbol(
    ctx: CommandContext,
    exchange: ExchangeLike,
    symbol: string,
    side: OrderSide,
    sizeArg: string,
    reduceOnly: boolean,
    terminate?: import('@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderOptions').ChaserTerminate,
  ): Promise<string> {
    const size = this.parseSizeAndConvertIfUsd(
      exchange,
      symbol,
      sizeArg,
      true,
      // For reduce-only chases, % defaults to position. For entry
      // chases, % defaults to balance per Phase 0 decision 1.
      reduceOnly ? (side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long) : undefined,
      reduceOnly ? 'position' : 'balance',
    );
    if (!reduceOnly) this.enforceFatfinger(ctx, symbol, size);
    // ChaserOrderInstance requires the full Exchange type - cast through unknown
    const instance = new ChaserOrderInstance(exchange as unknown as Exchange, {
      symbol,
      size,
      side,
      maxAttempts: 1000,
      reduceOnly,
      delay: 1_000,
      origin: 'cli',
      ...(terminate ? { terminate } : {}),
    });
    chaserOrderManagerInstance.add(instance);
    return instance.id;
  }

  /**
   * Parse a `to <length>` token. Returns a partial ChaserTerminate
   * (without the anchor — caller adds that) plus the original arg
   * for error messages, or null if the form isn't recognized.
   *
   * Accepted forms:
   *   `49500`           → absolute price
   *   `1%` / `0.5%`     → percent move from anchor
   *   `$50` / `50$`     → USD price delta from anchor
   */
  protected parseTerminateLength(arg: string): { type: 'price' | 'percent' | 'usd'; value: number } | null {
    const t = arg.trim();
    if (!t) return null;
    if (/^\d+(?:\.\d+)?%$/.test(t)) {
      const v = parseFloat(t.slice(0, -1));
      if (!Number.isFinite(v) || v <= 0) return null;
      return { type: 'percent', value: v };
    }
    if (/^\$\d+(?:\.\d+)?$/.test(t) || /^\d+(?:\.\d+)?\$$/.test(t)) {
      const raw = t.startsWith('$') ? t.slice(1) : t.slice(0, -1);
      const v = parseFloat(raw);
      if (!Number.isFinite(v) || v <= 0) return null;
      return { type: 'usd', value: v };
    }
    if (/^\d+(?:\.\d+)?$/.test(t)) {
      const v = parseFloat(t);
      if (!Number.isFinite(v) || v <= 0) return null;
      return { type: 'price', value: v };
    }
    return null;
  }

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const exchange = ctx.exchange;
    if (!exchange) {
      return this.error('Not connected to exchange');
    }

    const args = this.getRawArgs(ctx);

    // Validate side argument exists
    if (!args[1]) {
      return this.error('Please specify a side: chase buy or chase sell');
    }

    // Phase 3: `chase cancel` and `chase cancel keep` — stop the most
    // recently-started active chaser. `keep` leaves the open order in
    // the book; the default cancels it.
    if (args[1].toLowerCase() === 'cancel') {
      const keepOrder = args[2]?.toLowerCase() === 'keep';
      const active = chaserOrderManagerInstance.instances.filter((i) => !i.isStopped);
      if (active.length === 0) {
        this.printInfo(ctx, 'No active chasers to cancel');
        return this.success();
      }
      const target = active[active.length - 1];
      if (keepOrder) {
        chaserOrderManagerInstance.removeKeepOrder(target.id);
        this.printSuccess(ctx, `Stopped chaser [${target.id.slice(0, 8)}] — order left in book`);
      } else {
        chaserOrderManagerInstance.remove(target.id);
        this.printSuccess(ctx, `Cancelled chaser [${target.id.slice(0, 8)}]`);
      }
      return this.success();
    }

    const sideStr = args[1].toLowerCase();
    if (sideStr !== 'buy' && sideStr !== 'sell') {
      return this.error(`Invalid side "${args[1]}". Must be 'buy' or 'sell'.`);
    }
    // `chase cancel` is benign in sim — it only touches the local
    // chaser manager and (optionally) the exchange to cancel a
    // resting order. We let it through. Spawning a NEW chaser is
    // gated below.
    if (this.simulationGate(ctx)) return this.success();
    const side = sideStr as OrderSide;
    const positionSide = side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
    const reduceOnly = this.hasArgValue(args, 'reduce');

    // Health gate — block spawning an exposure-increasing chaser when the
    // private stream is degraded; reduce-only chases stay allowed (TSBUG-106).
    if (!reduceOnly) {
      const blocked = this.healthGate(ctx);
      if (blocked) return this.error(blocked);
    }

    // Parse `to <length>` once up front. The anchor price is captured
    // per-symbol when each chaser spawns (so multi-symbol chases work).
    let terminatePartial: { type: 'price' | 'percent' | 'usd'; value: number } | null = null;
    const toIdx = args.indexOf('to');
    if (toIdx !== -1) {
      const lenArg = args[toIdx + 1];
      if (!lenArg) {
        return this.error("chase: 'to' must be followed by a length (price / % / $)");
      }
      terminatePartial = this.parseTerminateLength(lenArg);
      if (!terminatePartial) {
        return this.error(`chase: invalid 'to' length '${lenArg}' — expected price, NN%, $NN, or NN$`);
      }
    }

    // Build a `to`-aware skip set so the trailing-symbol scan doesn't
    // try to validate `to` or the length token as market symbols.
    const skipIndices = new Set<number>();
    if (toIdx !== -1) {
      skipIndices.add(toIdx);
      skipIndices.add(toIdx + 1);
    }

    // Helper: spawn a chaser for one symbol, capturing the anchor price
    // from the live ticker if termination is configured.
    const spawn = async (sym: string, sizeArg: string): Promise<string | null> => {
      let term: import('@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderOptions').ChaserTerminate | undefined;
      if (terminatePartial) {
        const ticker = exchange.store?.tickers?.find((t) => t.symbol === sym);
        const anchor = ticker?.last;
        if (!anchor || anchor <= 0) {
          this.printError(ctx, `No anchor price available for ${sym} — can't compute 'to' threshold`);
          return null;
        }
        term = { ...terminatePartial, anchorPrice: anchor };
      }
      try {
        return await this.chaseBuySellSymbol(ctx, exchange, sym, side, sizeArg, reduceOnly, term);
      } catch (e) {
        this.printError(ctx, (e as Error).message);
        return null;
      }
    };

    const chaserIds: string[] = [];

    // Check for symbol at start: "chase buy SYMBOL $100"
    const symbolsAtStart = this.validateSymbols(exchange, args[2]?.split(',') || [], reduceOnly, positionSide);
    if (symbolsAtStart.length > 0) {
      if (!args[3]) {
        return this.error('Size argument missing. Usage: chase buy SYMBOL $amount');
      }
      const startedSymbols: string[] = [];
      for (const sym of symbolsAtStart) {
        const id = await spawn(sym, args[3]);
        if (id) {
          chaserIds.push(id);
          startedSymbols.push(sym);
        }
      }
      if (chaserIds.length === 0) {
        return this.error('No chasers started');
      }
      return this.formatStartMessage(ctx, chaserIds, startedSymbols);
    }

    if (!args[2]) {
      return this.error('Size argument missing. Usage: chase buy $amount');
    }

    // Check for symbol at end: "chase buy $100 SYMBOL" or "chase buy $100 reduce SYMBOL"
    let trailingSymbols: string[] = [];
    for (let i = args.length - 1; i >= 3; i--) {
      if (args[i] === 'reduce') continue;
      if (skipIndices.has(i)) continue;
      trailingSymbols = this.validateSymbols(exchange, args[i].split(','), reduceOnly, positionSide);
      if (trailingSymbols.length > 0) break;
    }

    if (trailingSymbols.length > 0) {
      const startedSymbols: string[] = [];
      for (const sym of trailingSymbols) {
        const id = await spawn(sym, args[2]);
        if (id) {
          chaserIds.push(id);
          startedSymbols.push(sym);
        }
      }
      if (chaserIds.length === 0) {
        return this.error('No chasers started');
      }
      return this.formatStartMessage(ctx, chaserIds, startedSymbols);
    }

    // Default: use session symbol
    const id = await spawn(ctx.symbol, args[2]);
    if (id) chaserIds.push(id);
    if (chaserIds.length === 0) {
      return this.error('No chasers started');
    }
    return this.formatStartMessage(ctx, chaserIds, [ctx.symbol]);
  }

  /**
   * Format the start message with chaser IDs for reference
   */
  private formatStartMessage(ctx: CommandContext, chaserIds: string[], symbols: string[]): CommandResult {
    const isSingle = chaserIds.length === 1;
    const symbolList = symbols.join(', ');
    const idList = chaserIds.map((id) => id.slice(0, 8)).join(', ');

    let message = isSingle
      ? `Chaser started for ${symbolList} [${idList}]`
      : `${chaserIds.length} chasers started for ${symbolList} [${idList}]`;

    message += '\nUse "watch chase" to monitor progress, "chasers" to list all';

    this.print(ctx, [message]);
    return this.success();
  }
}
