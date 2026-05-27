import { formatPrice } from '@tealstreet/utils/src/formatting/formatPrice';
import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import type { ExchangeLike } from '../../types';
import { GREEN, YELLOW, CYAN, RESET, DIM } from './colors';
import type { AccountExchange } from './MonitorCommand';

// Padding helper
function pad(str: string, width: number): string {
  if (str.length >= width) return str.substring(0, width);
  return str + ' '.repeat(width - str.length);
}

/**
 * Format spot balances table - reusable formatter for CLI watch modes
 */
export function formatSpotTable(exchange: ExchangeLike): string {
  const balance = exchange.store?.balance;
  if (!balance) {
    return 'No balance data available\n';
  }

  const spotCoins = Object.values(balance.coins || {})
    .filter((coin) => coin.notional > 0)
    .sort((a, b) => b.notional - a.notional);

  if (spotCoins.length === 0) {
    return 'No spot balances\n';
  }

  // Calculate total spot value
  const totalSpot = spotCoins.reduce((sum, coin) => sum + coin.notional, 0);

  // Header
  let table = '┌──────────┬──────────────┬────────────────┐\n';
  table += '│ Asset    │ Amount       │ Value (USD)    │\n';
  table += '├──────────┼──────────────┼────────────────┤\n';

  for (const coin of spotCoins) {
    const asset = pad(coin.base, 8);
    const amount = pad(formatPrice(coin.amount, 4), 12);
    const value = pad('$' + formatPrice(coin.notional), 14);

    table += `│ ${CYAN}${asset}${RESET} │ ${amount} │ ${GREEN}${value}${RESET} │\n`;
  }

  // Footer with total
  table += '├──────────┴──────────────┼────────────────┤\n';
  table += `│ ${DIM}Total${RESET}                   │ ${YELLOW}${pad('$' + formatPrice(totalSpot), 14)}${RESET} │\n`;
  table += '└────────────────────────┴────────────────┘\n';

  return table;
}

/**
 * Format spot balances table for multiple accounts - shows each account's spot holdings
 */
export function formatMultiAccountSpotTable(accounts: AccountExchange[]): string {
  // Collect all spot coins with account info
  const allCoins: Array<{
    accountName: string;
    coin: { base: string; amount: number; notional: number };
  }> = [];

  for (const { accountName, exchange } of accounts) {
    const balance = exchange.store?.balance;
    const spotCoins = Object.values(balance?.coins || {}).filter((coin) => coin.notional > 0);
    for (const coin of spotCoins) {
      allCoins.push({ accountName, coin });
    }
  }

  if (allCoins.length === 0) {
    return 'No spot balances across all accounts\n';
  }

  // Sort by account name, then by notional value (descending)
  allCoins.sort((a, b) => {
    const accountCmp = a.accountName.localeCompare(b.accountName);
    if (accountCmp !== 0) return accountCmp;
    return b.coin.notional - a.coin.notional;
  });

  // Header with account column
  let table = '┌──────────────┬──────────┬──────────────┬────────────────┐\n';
  table += '│ Account      │ Asset    │ Amount       │ Value (USD)    │\n';
  table += '├──────────────┼──────────┼──────────────┼────────────────┤\n';

  let grandTotal = 0;
  let currentAccount = '';
  let accountTotal = 0;

  for (let i = 0; i < allCoins.length; i++) {
    const { accountName, coin } = allCoins[i];

    // Add separator between accounts
    if (currentAccount !== '' && currentAccount !== accountName) {
      // Add account subtotal before separator
      table += `│ ${DIM}${pad('Subtotal', 12)}${RESET} │          │              │ ${YELLOW}${pad('$' + formatPrice(accountTotal), 14)}${RESET} │\n`;
      table += '├──────────────┼──────────┼──────────────┼────────────────┤\n';
      accountTotal = 0;
    }
    currentAccount = accountName;
    accountTotal += coin.notional;
    grandTotal += coin.notional;

    const asset = pad(coin.base, 8);
    const amount = pad(formatPrice(coin.amount, 4), 12);
    const value = pad('$' + formatPrice(coin.notional), 14);

    table += `│ ${pad(accountName, 12)} │ ${CYAN}${asset}${RESET} │ ${amount} │ ${GREEN}${value}${RESET} │\n`;
  }

  // Add final account subtotal
  table += `│ ${DIM}${pad('Subtotal', 12)}${RESET} │          │              │ ${YELLOW}${pad('$' + formatPrice(accountTotal), 14)}${RESET} │\n`;

  // Grand total footer
  table += '├──────────────┴──────────┴──────────────┼────────────────┤\n';
  table += `│ ${CYAN}${pad('TOTAL', 37)}${RESET} │ ${YELLOW}${pad('$' + formatPrice(grandTotal), 14)}${RESET} │\n`;
  table += '└──────────────────────────────────────────┴────────────────┘\n';

  return table;
}

/**
 * Spot command - shows spot balances
 */
export class SpotCommand extends BaseCommand {
  readonly name = 'spot';
  readonly description = 'Show spot balances';
  readonly category = CommandCategory.Trading;
  readonly aliases = [];
  readonly examples = ['spot'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const exchange = ctx.exchange;
    if (!exchange) {
      this.printError(ctx, 'Not connected to exchange');
      return this.success();
    }

    const output = formatSpotTable(exchange);
    this.print(ctx, [output]);
    return this.success();
  }
}
