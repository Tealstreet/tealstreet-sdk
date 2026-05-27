import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { getConnectionStatus, type ConnectionHealth } from '../../utils/ConnectionMonitor';

// ANSI color codes
const GREEN = '\x1b[32m';
const YELLOW = '\x1b[33m';
const RED = '\x1b[31m';
const GRAY = '\x1b[90m';
const CYAN = '\x1b[36m';
const DIM = '\x1b[2m';
const RESET = '\x1b[0m';

/**
 * Connection command - shows connection status and allows reconnection
 *
 * Usage:
 *   connection          - Show connection status
 *   connection --all    - Show connection status for all connected accounts
 *   connection reconnect - Force reconnect all WebSockets
 */
export class ConnectionCommand extends BaseCommand {
  readonly name = 'connection';
  readonly description = 'Show connection status or reconnect';
  readonly category = CommandCategory.Session;
  readonly aliases = ['conn', 'ws'];
  readonly examples = ['connection', 'conn', 'conn --all', 'conn reconnect', 'ws reconnect'];
  readonly requiresConnection = false; // Allow --all without requiring current connection

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const hasAllFlag = this.hasFlag(ctx, 'all', 'a');
    const subcommand = this.getArg(ctx, 1)?.toLowerCase();

    // Handle --all flag
    if (hasAllFlag) {
      return this.showAllStatus(ctx);
    }

    if (subcommand === 'reconnect' || subcommand === 'rc') {
      return this.handleReconnect(ctx);
    }

    return this.showStatus(ctx);
  }

  /**
   * Show connection status for all connected accounts
   */
  private showAllStatus(ctx: CommandContext): CommandResult {
    const accounts = ctx.adapter.listAccounts();
    const connectedAccounts = accounts.filter((acc) => ctx.adapter.isAccountConnected?.(acc.name));

    if (connectedAccounts.length === 0) {
      this.printWarning(ctx, 'No accounts connected');
      return this.success();
    }

    const lines: string[] = [''];

    for (const acc of connectedAccounts) {
      const exchange = ctx.adapter.getExchange?.(acc.name);
      if (!exchange) continue;

      const status = getConnectionStatus(exchange);
      const dot = this.getConnectionDot(status.overall);
      const isCurrent = acc.name === ctx.accountName;
      const suffix = isCurrent ? ` ${CYAN}(primary)${RESET}` : '';

      lines.push(`${dot} ${acc.name}${suffix}`);
      lines.push('─'.repeat(50));

      // Public WS
      const publicDot = this.getConnectionDot(status.public.status);
      lines.push(`  ${publicDot} Public WS:  ${this.formatAge(status.public.lastMessageAgo)}`);

      // Private WS
      const privateDot = this.getConnectionDot(status.private.status);
      lines.push(`  ${privateDot} Private WS: ${this.formatAge(status.private.lastMessageAgo)}`);

      // Optional: Futures WS
      if (status.futures) {
        const futuresDot = this.getConnectionDot(status.futures.status);
        lines.push(`  ${futuresDot} Futures WS: ${this.formatAge(status.futures.lastMessageAgo)}`);
      }

      // Optional: Business WS
      if (status.business) {
        const businessDot = this.getConnectionDot(status.business.status);
        lines.push(`  ${businessDot} Business WS: ${this.formatAge(status.business.lastMessageAgo)}`);
      }

      // Latency
      if (status.latency !== undefined) {
        lines.push(`  ${DIM}Latency:${RESET} ${status.latency}ms`);
      }

      lines.push('');
    }

    this.print(ctx, lines);
    return this.success();
  }

  private showStatus(ctx: CommandContext): CommandResult {
    const exchange = ctx.exchange;
    if (!exchange) {
      this.printError(ctx, 'Not connected to any exchange');
      return this.error('Not connected');
    }

    const status = getConnectionStatus(exchange);
    const overallDot = this.getConnectionDot(status.overall);

    const lines: string[] = [];
    lines.push('');
    lines.push(`${overallDot} Connection Status: ${exchange.name}`);
    lines.push('─'.repeat(50));

    // Public WebSocket
    const publicDot = this.getConnectionDot(status.public.status);
    lines.push(`${publicDot} Public WS:  ${this.formatAge(status.public.lastMessageAgo)}`);

    // Private WebSocket
    const privateDot = this.getConnectionDot(status.private.status);
    lines.push(`${privateDot} Private WS: ${this.formatAge(status.private.lastMessageAgo)}`);

    // Optional streams (if present)
    if (status.futures) {
      const futuresDot = this.getConnectionDot(status.futures.status);
      lines.push(`${futuresDot} Futures WS: ${this.formatAge(status.futures.lastMessageAgo)}`);
    }

    if (status.business) {
      const businessDot = this.getConnectionDot(status.business.status);
      lines.push(`${businessDot} Business WS: ${this.formatAge(status.business.lastMessageAgo)}`);
    }

    lines.push('');

    // Latency
    if (status.latency !== undefined) {
      lines.push(`API Latency: ${status.latency}ms`);
    }

    // Data loaded status
    const loaded = exchange.store?.loaded;
    if (loaded) {
      lines.push('');
      lines.push('Data Loaded:');
      lines.push(`  Markets:   ${loaded.markets ? 'Yes' : 'No'}`);
      lines.push(`  Tickers:   ${loaded.tickers ? 'Yes' : 'No'}`);
      lines.push(`  Balance:   ${loaded.balance ? 'Yes' : 'No'}`);
      lines.push(`  Orders:    ${loaded.orders ? 'Yes' : 'No'}`);
      lines.push(`  Positions: ${loaded.positions ? 'Yes' : 'No'}`);
    }

    // Check for issues
    const hasIssues = status.overall === 'error' || status.overall === 'disconnected';
    const hasWarnings = status.overall === 'warning';

    if (hasIssues) {
      lines.push('');
      lines.push(`${RED}WARNING: Connection issues detected!${RESET}`);
      lines.push('Run "connection reconnect" to force reconnect.');
    } else if (hasWarnings) {
      lines.push('');
      lines.push(`${YELLOW}Note: Some connections may be delayed.${RESET}`);
    }

    lines.push('');
    this.print(ctx, lines);
    return this.success();
  }

  private async handleReconnect(ctx: CommandContext): Promise<CommandResult> {
    const exchange = ctx.exchange;
    if (!exchange) {
      this.printError(ctx, 'Not connected to any exchange');
      return this.error('Not connected');
    }

    if (!exchange.getAllSockets) {
      this.printWarning(ctx, 'This exchange does not support manual reconnection');
      return this.success();
    }

    this.printInfo(ctx, 'Reconnecting WebSockets...');

    try {
      const sockets = exchange.getAllSockets();
      let reconnectedCount = 0;

      for (const socket of sockets) {
        if (socket?.reconnect) {
          socket.reconnect();
          reconnectedCount++;
        }
      }

      if (reconnectedCount > 0) {
        this.printSuccess(ctx, `Triggered reconnection for ${reconnectedCount} WebSocket(s)`);
      } else {
        this.printWarning(ctx, 'No active WebSockets to reconnect');
      }

      return this.success();
    } catch (error) {
      this.printError(ctx, `Reconnection failed: ${(error as Error).message}`);
      return this.error((error as Error).message);
    }
  }

  /**
   * Get colored dot based on connection health (same as AccountsCommand)
   */
  private getConnectionDot(health: ConnectionHealth): string {
    switch (health) {
      case 'healthy':
        return `${GREEN}●${RESET}`;
      case 'warning':
      case 'initializing':
        return `${YELLOW}●${RESET}`;
      case 'error':
      case 'disconnected':
        return `${RED}●${RESET}`;
      default:
        return `${GRAY}●${RESET}`;
    }
  }

  private formatAge(ageMs: number | null): string {
    if (ageMs === null) return `${GRAY}No data received yet${RESET}`;

    if (ageMs < 1000) {
      return `${ageMs}ms ago`;
    } else if (ageMs < 60_000) {
      return `${(ageMs / 1000).toFixed(1)}s ago`;
    } else {
      const minutes = Math.floor(ageMs / 60_000);
      const seconds = Math.floor((ageMs % 60_000) / 1000);
      return `${minutes}m ${seconds}s ago`;
    }
  }
}
