import { formatPrice } from '@tealstreet/utils/src/formatting/formatPrice';
import { OrderStatus, OrderType, PositionSide, type Position, type Order } from '@tealstreet/safe-cex/src/types';
import { calculatePositionAccountLeverage } from '@tealstreet/safe-cex/src/utils/leverage';
import type { ExchangeLike } from '../../types';
import { formatBaseSize } from '../../utils/formatBaseSize';
import { RESET, DIM, CLEAR_SCREEN } from './colors';
import {
  type TableData,
  type TextStyle,
  cell,
  separatorRow,
  renderTable,
} from '../../table';

/**
 * Type for multi-account data
 */
export type AccountExchange = {
  accountName: string;
  exchange: ExchangeLike;
};

/**
 * Format order type for display
 */
function formatOrderTypeLabel(type: OrderType): string {
  switch (type) {
    case OrderType.Limit:
      return 'Limit';
    case OrderType.Market:
      return 'Market';
    case OrderType.StopLoss:
      return 'SL';
    case OrderType.TakeProfit:
      return 'TP';
    case OrderType.TrailingStopLoss:
      return 'TSL';
    default:
      return String(type);
  }
}

/**
 * Format positions table - reusable formatter for CLI watch modes
 */
export function formatPositionsTable(exchange: ExchangeLike): string {
  const positions = exchange.store?.positions?.filter((pos) => pos.notional !== 0) ?? [];

  if (positions.length === 0) {
    return 'No open positions\n';
  }

  const tableData: TableData = {
    columns: [
      { header: 'Symbol', width: 15 },
      { header: 'Side', width: 5 },
      { header: 'Size', width: 10 },
      { header: 'Entry', width: 10 },
      { header: 'Mark', width: 10 },
      { header: 'uPnL', width: 14 },
      { header: 'Lev', width: 8 },
    ],
    rows: [],
  };

  let totalPnl = 0;
  for (const pos of positions) {
    const ticker = exchange.store?.tickers?.find((t) => t.symbol === pos.symbol);
    const market = exchange.store?.markets?.find((m) => m.symbol === pos.symbol);
    const pricePrecision = market?.precision?.price;
    const markPrice = ticker?.mark ?? ticker?.last ?? 0;
    const sideStyle: TextStyle = pos.side === PositionSide.Long ? 'green' : 'red';
    const side = pos.side === PositionSide.Long ? 'LONG' : 'SHORT';
    const pnlStyle: TextStyle = pos.unrealizedPnl >= 0 ? 'green' : 'red';
    const pnlSign = pos.unrealizedPnl >= 0 ? '+' : '';
    const pnlPercent = pos.entryPrice > 0 ? ((pos.unrealizedPnl / pos.notional) * 100).toFixed(1) : '0.0';

    const balance = exchange.store?.balance;
    const accountLev = calculatePositionAccountLeverage(pos.notional, balance?.total ?? 0);
    const levStyle: TextStyle = accountLev > 5 ? 'yellow' : 'cyan';
    const levStr = `${accountLev.toFixed(2)}/${pos.leverage}x`;

    totalPnl += pos.unrealizedPnl;

    const pnlStr = `${pnlSign}${formatPrice(pos.unrealizedPnl)} (${pnlSign}${pnlPercent}%)`;
    tableData.rows.push({
      cells: [
        cell(pos.symbol),
        cell(side, sideStyle),
        cell(formatBaseSize(pos.contracts, market)),
        cell(formatPrice(pos.entryPrice, pricePrecision)),
        cell(formatPrice(markPrice, pricePrecision), 'cyan'),
        cell(pnlStr, pnlStyle),
        cell(levStr, levStyle),
      ],
    });
  }

  const totalPnlStyle: TextStyle = totalPnl >= 0 ? 'green' : 'red';
  const totalSign = totalPnl >= 0 ? '+' : '';
  tableData.footer = `Total uPnL: ${totalSign}${formatPrice(totalPnl)}`;
  tableData.footerStyle = totalPnlStyle;

  return renderTable(tableData);
}

/**
 * Format orders table - reusable formatter for CLI watch modes
 */
export function formatOrdersTable(exchange: ExchangeLike): string {
  const orders = exchange.store?.orders?.filter(
    (o) => o.status === OrderStatus.Open || o.status === OrderStatus.Creating,
  ) ?? [];

  if (orders.length === 0) {
    return 'No open orders\n';
  }

  const tableData: TableData = {
    columns: [
      { header: 'Symbol', width: 15 },
      { header: 'Side', width: 5 },
      { header: 'Type', width: 8 },
      { header: 'Size', width: 10 },
      { header: 'Price', width: 10 },
      { header: 'Trigger', width: 10 },
    ],
    rows: [],
  };

  for (const order of orders) {
    const market = exchange.store?.markets?.find((m) => m.symbol === order.symbol);
    const pricePrecision = market?.precision?.price;
    const sideStyle: TextStyle = order.side === 'buy' ? 'green' : 'red';
    const side = order.side === 'buy' ? 'BUY' : 'SELL';
    const type = formatOrderTypeLabel(order.type);
    const trigger = order.takeProfit
      ? formatPrice(order.takeProfit, pricePrecision)
      : order.stopLoss
        ? formatPrice(order.stopLoss, pricePrecision)
        : '-';

    tableData.rows.push({
      cells: [
        cell(order.symbol),
        cell(side, sideStyle),
        cell(type),
        cell(formatBaseSize(order.remaining, market)),
        cell(order.price ? formatPrice(order.price, pricePrecision) : 'Market', 'cyan'),
        cell(trigger),
      ],
    });
  }

  return renderTable(tableData);
}

/**
 * Format margin/balance table - reusable formatter for CLI watch modes
 */
export function formatMarginTable(exchange: ExchangeLike): string {
  const balance = exchange.store?.balance;
  const positions = exchange.store?.positions?.filter((pos) => pos.notional !== 0) ?? [];

  if (!balance) {
    return 'Balance data not available\n';
  }

  // Calculate total margin used
  let totalNotional = 0;
  let totalUnrealizedPnl = 0;
  for (const pos of positions) {
    totalNotional += Math.abs(pos.notional);
    totalUnrealizedPnl += pos.unrealizedPnl;
  }

  // Calculate margin ratio if we have positions
  const marginRatio = balance.total > 0 ? (totalNotional / balance.total) * 100 : 0;
  const usedMargin = balance.used ?? 0;
  const freeMargin = balance.free ?? balance.total - usedMargin;

  const tableData: TableData = {
    columns: [
      { header: 'Metric', width: 15 },
      { header: 'Value', width: 14 },
    ],
    rows: [],
  };

  // Total balance
  tableData.rows.push({
    cells: [cell('Total Balance'), cell(formatPrice(balance.total), 'cyan')],
  });

  // Free margin
  const freeStyle: TextStyle = freeMargin > balance.total * 0.3 ? 'green' : 'yellow';
  tableData.rows.push({
    cells: [cell('Free Margin'), cell(formatPrice(freeMargin), freeStyle)],
  });

  // Used margin
  tableData.rows.push({
    cells: [cell('Used Margin'), cell(formatPrice(usedMargin))],
  });

  // Total notional
  tableData.rows.push({
    cells: [cell('Position Size'), cell(formatPrice(totalNotional))],
  });

  // Account leverage (total notional / balance)
  const accountLev = balance.total > 0 ? totalNotional / balance.total : 0;
  const levStyle: TextStyle = accountLev > 5 ? 'red' : accountLev > 2 ? 'yellow' : 'green';
  tableData.rows.push({
    cells: [cell('Account Lev'), cell(accountLev.toFixed(2) + 'x', levStyle)],
  });

  // Margin ratio
  const ratioStyle: TextStyle = marginRatio > 80 ? 'red' : marginRatio > 50 ? 'yellow' : 'green';
  tableData.rows.push({
    cells: [cell('Margin Ratio'), cell(marginRatio.toFixed(1) + '%', ratioStyle)],
  });

  // Unrealized PnL
  const pnlStyle: TextStyle = totalUnrealizedPnl >= 0 ? 'green' : 'red';
  const pnlSign = totalUnrealizedPnl >= 0 ? '+' : '';
  tableData.rows.push({
    cells: [cell('Unrealized PnL'), cell(pnlSign + formatPrice(totalUnrealizedPnl), pnlStyle)],
  });

  return renderTable(tableData);
}

/**
 * Format positions table for multiple accounts - groups by account with account column
 */
export function formatMultiAccountPositionsTable(accounts: AccountExchange[]): string {
  // Collect all positions with account info
  const allPositions: Array<{
    accountName: string;
    position: Position;
    exchange: ExchangeLike;
  }> = [];

  for (const { accountName, exchange } of accounts) {
    const positions = exchange.store?.positions?.filter((pos) => pos.notional !== 0) ?? [];
    for (const pos of positions) {
      allPositions.push({ accountName, position: pos, exchange });
    }
  }

  if (allPositions.length === 0) {
    return 'No open positions across all accounts\n';
  }

  // Sort by account name, then symbol
  allPositions.sort((a, b) => {
    const accountCmp = a.accountName.localeCompare(b.accountName);
    if (accountCmp !== 0) return accountCmp;
    return a.position.symbol.localeCompare(b.position.symbol);
  });

  const tableData: TableData = {
    columns: [
      { header: 'Account', width: 12 },
      { header: 'Symbol', width: 15 },
      { header: 'Side', width: 5 },
      { header: 'Size', width: 10 },
      { header: 'Entry', width: 10 },
      { header: 'Mark', width: 10 },
      { header: 'uPnL', width: 14 },
      { header: 'Lev', width: 8 },
    ],
    rows: [],
  };

  let totalPnl = 0;
  let currentAccount = '';

  for (const { accountName, position: pos, exchange } of allPositions) {
    // Add separator between accounts
    if (currentAccount !== '' && currentAccount !== accountName) {
      tableData.rows.push(separatorRow(8));
    }
    currentAccount = accountName;

    const ticker = exchange.store?.tickers?.find((t) => t.symbol === pos.symbol);
    const market = exchange.store?.markets?.find((m) => m.symbol === pos.symbol);
    const pricePrecision = market?.precision?.price;
    const markPrice = ticker?.mark ?? ticker?.last ?? 0;
    const sideStyle: TextStyle = pos.side === PositionSide.Long ? 'green' : 'red';
    const side = pos.side === PositionSide.Long ? 'LONG' : 'SHORT';
    const pnlStyle: TextStyle = pos.unrealizedPnl >= 0 ? 'green' : 'red';
    const pnlSign = pos.unrealizedPnl >= 0 ? '+' : '';
    const pnlPercent = pos.entryPrice > 0 ? ((pos.unrealizedPnl / pos.notional) * 100).toFixed(1) : '0.0';

    const balance = exchange.store?.balance;
    const accountLev = calculatePositionAccountLeverage(pos.notional, balance?.total ?? 0);
    const levStyle: TextStyle = accountLev > 5 ? 'yellow' : 'cyan';
    const levStr = `${accountLev.toFixed(2)}/${pos.leverage}x`;

    totalPnl += pos.unrealizedPnl;

    const pnlStr = `${pnlSign}${formatPrice(pos.unrealizedPnl)} (${pnlSign}${pnlPercent}%)`;
    tableData.rows.push({
      cells: [
        cell(accountName),
        cell(pos.symbol),
        cell(side, sideStyle),
        cell(formatBaseSize(pos.contracts, market)),
        cell(formatPrice(pos.entryPrice, pricePrecision)),
        cell(formatPrice(markPrice, pricePrecision), 'cyan'),
        cell(pnlStr, pnlStyle),
        cell(levStr, levStyle),
      ],
    });
  }

  const totalPnlStyle: TextStyle = totalPnl >= 0 ? 'green' : 'red';
  const totalSign = totalPnl >= 0 ? '+' : '';
  tableData.footer = `Total uPnL (all accounts): ${totalSign}${formatPrice(totalPnl)}`;
  tableData.footerStyle = totalPnlStyle;

  return renderTable(tableData);
}

/**
 * Format orders table for multiple accounts - groups by account with account column
 */
export function formatMultiAccountOrdersTable(accounts: AccountExchange[]): string {
  // Collect all orders with account info
  const allOrders: Array<{
    accountName: string;
    order: Order;
    exchange: ExchangeLike;
  }> = [];

  for (const { accountName, exchange } of accounts) {
    const orders = exchange.store?.orders?.filter(
      (o) => o.status === OrderStatus.Open || o.status === OrderStatus.Creating,
    ) ?? [];
    for (const order of orders) {
      allOrders.push({ accountName, order, exchange });
    }
  }

  if (allOrders.length === 0) {
    return 'No open orders across all accounts\n';
  }

  // Sort by account name, then symbol
  allOrders.sort((a, b) => {
    const accountCmp = a.accountName.localeCompare(b.accountName);
    if (accountCmp !== 0) return accountCmp;
    return a.order.symbol.localeCompare(b.order.symbol);
  });

  const tableData: TableData = {
    columns: [
      { header: 'Account', width: 12 },
      { header: 'Symbol', width: 15 },
      { header: 'Side', width: 5 },
      { header: 'Type', width: 8 },
      { header: 'Size', width: 10 },
      { header: 'Price', width: 10 },
      { header: 'Trigger', width: 10 },
    ],
    rows: [],
  };

  let currentAccount = '';

  for (const { accountName, order, exchange } of allOrders) {
    // Add separator between accounts
    if (currentAccount !== '' && currentAccount !== accountName) {
      tableData.rows.push(separatorRow(7));
    }
    currentAccount = accountName;

    const market = exchange.store?.markets?.find((m) => m.symbol === order.symbol);
    const pricePrecision = market?.precision?.price;
    const sideStyle: TextStyle = order.side === 'buy' ? 'green' : 'red';
    const side = order.side === 'buy' ? 'BUY' : 'SELL';
    const type = formatOrderTypeLabel(order.type);
    const trigger = order.takeProfit
      ? formatPrice(order.takeProfit, pricePrecision)
      : order.stopLoss
        ? formatPrice(order.stopLoss, pricePrecision)
        : '-';

    tableData.rows.push({
      cells: [
        cell(accountName),
        cell(order.symbol),
        cell(side, sideStyle),
        cell(type),
        cell(formatBaseSize(order.remaining, market)),
        cell(order.price ? formatPrice(order.price, pricePrecision) : 'Market', 'cyan'),
        cell(trigger),
      ],
    });
  }

  return renderTable(tableData);
}

/**
 * Format margin/balance table for multiple accounts - shows each account's balance
 */
export function formatMultiAccountMarginTable(accounts: AccountExchange[]): string {
  if (accounts.length === 0) {
    return 'No accounts connected\n';
  }

  const tableData: TableData = {
    columns: [
      { header: 'Account', width: 12 },
      { header: 'Total Balance', width: 14 },
      { header: 'Free Margin', width: 14 },
      { header: 'Position Size', width: 14 },
      { header: 'Acct Lev', width: 8 },
      { header: 'uPnL', width: 12 },
    ],
    rows: [],
  };

  let grandTotalBalance = 0;
  let grandTotalPnl = 0;
  let grandTotalNotional = 0;

  for (const { accountName, exchange } of accounts) {
    const balance = exchange.store?.balance;
    const positions = exchange.store?.positions?.filter((pos) => pos.notional !== 0) ?? [];

    let totalNotional = 0;
    let totalUnrealizedPnl = 0;
    for (const pos of positions) {
      totalNotional += Math.abs(pos.notional);
      totalUnrealizedPnl += pos.unrealizedPnl;
    }

    grandTotalBalance += balance?.total ?? 0;
    grandTotalPnl += totalUnrealizedPnl;
    grandTotalNotional += totalNotional;

    const freeMargin = balance?.free ?? (balance?.total ?? 0) - (balance?.used ?? 0);
    const accountLev = (balance?.total ?? 0) > 0 ? totalNotional / (balance?.total ?? 1) : 0;
    const levStyle: TextStyle = accountLev > 5 ? 'red' : accountLev > 2 ? 'yellow' : 'green';
    const pnlStyle: TextStyle = totalUnrealizedPnl >= 0 ? 'green' : 'red';
    const pnlSign = totalUnrealizedPnl >= 0 ? '+' : '';

    tableData.rows.push({
      cells: [
        cell(accountName),
        cell(formatPrice(balance?.total ?? 0), 'cyan'),
        cell(formatPrice(freeMargin)),
        cell(formatPrice(totalNotional)),
        cell(accountLev.toFixed(2) + 'x', levStyle),
        cell(pnlSign + formatPrice(totalUnrealizedPnl), pnlStyle),
      ],
    });
  }

  // Add separator before totals
  tableData.rows.push(separatorRow(6));

  // Totals row
  const grandLev = grandTotalBalance > 0 ? grandTotalNotional / grandTotalBalance : 0;
  const grandLevStyle: TextStyle = grandLev > 5 ? 'red' : grandLev > 2 ? 'yellow' : 'green';
  const grandPnlStyle: TextStyle = grandTotalPnl >= 0 ? 'green' : 'red';
  const grandPnlSign = grandTotalPnl >= 0 ? '+' : '';

  tableData.rows.push({
    cells: [
      cell('TOTAL', 'cyan'),
      cell(formatPrice(grandTotalBalance), 'cyan'),
      cell('-'),
      cell(formatPrice(grandTotalNotional)),
      cell(grandLev.toFixed(2) + 'x', grandLevStyle),
      cell(grandPnlSign + formatPrice(grandTotalPnl), grandPnlStyle),
    ],
  });

  return renderTable(tableData);
}

/**
 * Format full account monitor output for multiple accounts
 * Combines margin, positions, and orders tables
 */
export function formatMultiAccountMonitorOutput(accounts: AccountExchange[]): string {
  const timestamp = new Date().toLocaleTimeString();
  let output = CLEAR_SCREEN;
  output += `═══════════════════════════════════════════════════════════════════════════════\n`;
  output += `  ALL ACCOUNTS                                                  ${timestamp}\n`;
  output += `═══════════════════════════════════════════════════════════════════════════════\n\n`;

  output += `${DIM}MARGIN${RESET}\n`;
  output += formatMultiAccountMarginTable(accounts);
  output += '\n';

  output += `${DIM}POSITIONS${RESET}\n`;
  output += formatMultiAccountPositionsTable(accounts);
  output += '\n';

  output += `${DIM}ORDERS${RESET}\n`;
  output += formatMultiAccountOrdersTable(accounts);
  output += '\n';

  output += `${DIM}Press Escape or Ctrl+X to exit${RESET}\n`;

  return output;
}

/**
 * Format single account monitor output
 * Combines margin, positions, and orders tables for watch account mode
 */
export function formatAccountMonitorOutput(exchange: ExchangeLike): string {
  const timestamp = new Date().toLocaleTimeString();
  let output = CLEAR_SCREEN;
  output += `═══════════════════════════════════════════════════════════════════════════════\n`;
  output += `  ACCOUNT                                                    ${timestamp}\n`;
  output += `═══════════════════════════════════════════════════════════════════════════════\n\n`;

  output += `${DIM}MARGIN${RESET}\n`;
  output += formatMarginTable(exchange);
  output += '\n';

  output += `${DIM}POSITIONS${RESET}\n`;
  output += formatPositionsTable(exchange);
  output += '\n';

  output += `${DIM}ORDERS${RESET}\n`;
  output += formatOrdersTable(exchange);
  output += '\n';

  output += `${DIM}Press Escape or Ctrl+X to exit${RESET}\n`;

  return output;
}
