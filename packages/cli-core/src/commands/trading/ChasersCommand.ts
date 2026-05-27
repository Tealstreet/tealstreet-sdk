import { formatPrice } from '@tealstreet/utils/src/formatting/formatPrice';
import { OrderSide } from '@tealstreet/safe-cex/src/types';
import { chaserOrderManagerInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager';
import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { GREEN, RED, YELLOW, RESET, DIM, BOLD } from './colors';

const SEPARATOR = '─'.repeat(60);

export class ChasersCommand extends TradingBaseCommand {
  readonly name = 'chasers';
  readonly description = 'List all active and recent chasers';
  readonly category = CommandCategory.Trading;
  readonly aliases: string[] = [];
  readonly examples = [
    'chasers - List all chasers',
    'chasers cancel <id> - Cancel a specific chaser',
    'chasers cancel all - Cancel all active chasers',
    'watch chase - Monitor all chasers in real-time',
  ];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const args = this.getRawArgs(ctx);

    // Handle cancel subcommand
    if (args[1]?.toLowerCase() === 'cancel') {
      return this.handleCancel(ctx, args[2]);
    }

    const instances = chaserOrderManagerInstance.instances;

    if (instances.length === 0) {
      this.print(ctx, ['No active or recent chasers']);
      return this.success();
    }

    const outputLines: string[] = [];
    outputLines.push(`${SEPARATOR}`);
    outputLines.push(`${BOLD}Chasers${RESET} (${instances.length})`);
    outputLines.push(`${SEPARATOR}`);
    outputLines.push('');

    for (const instance of instances) {
      const { id, options, state, remainingSize } = instance;
      const filledSize = options.size - remainingSize;
      const fillPercent = options.size > 0 ? (filledSize / options.size) * 100 : 0;

      const market = instance.exchange.store.markets.find((m) => m.symbol === options.symbol);
      const amountPrecision = market?.precision?.amount;

      const sideColor = options.side === OrderSide.Buy ? GREEN : RED;
      const sideStr = options.side === OrderSide.Buy ? 'BUY' : 'SELL';

      // Status indicator
      let statusStr: string;
      let statusColor: string;
      if (instance.isStopped) {
        if (state.stopReason.includes('Completed') || state.stopReason.includes('Fully filled')) {
          statusStr = 'DONE';
          statusColor = GREEN;
        } else if (state.stopReason.includes('cancelled') || state.stopReason.includes('User')) {
          statusStr = 'CANCELLED';
          statusColor = YELLOW;
        } else {
          statusStr = 'STOPPED';
          statusColor = RED;
        }
      } else {
        statusStr = 'ACTIVE';
        statusColor = GREEN;
      }

      // Short ID for display
      const shortId = id.slice(0, 8);

      // Format sizes
      const filledStr = formatPrice(filledSize, amountPrecision);
      const totalStr = formatPrice(options.size, amountPrecision);

      let line = `${DIM}[${shortId}]${RESET} ${BOLD}${options.symbol}${RESET} `;
      line += `${sideColor}${sideStr}${RESET}`;
      line += options.reduceOnly ? ` ${DIM}(reduce)${RESET}` : '';
      outputLines.push(line);

      line = `  ${statusColor}${statusStr}${RESET}`;
      line += ` | ${GREEN}${filledStr}${RESET}/${totalStr} (${fillPercent.toFixed(1)}%)`;
      line += ` | ${state.attempts} attempts`;
      outputLines.push(line);

      if (instance.isStopped && state.stopReason) {
        outputLines.push(`  ${DIM}${state.stopReason}${RESET}`);
      }
      outputLines.push('');
    }

    const activeCount = instances.filter((c) => !c.isStopped).length;
    const completedCount = instances.filter((c) => c.isStopped).length;

    outputLines.push(`${SEPARATOR}`);
    outputLines.push(`${DIM}Active: ${activeCount}  Completed: ${completedCount}${RESET}`);
    outputLines.push(`${DIM}Use "watch chase" to monitor in real-time${RESET}`);

    this.print(ctx, outputLines);
    return this.success();
  }

  private handleCancel(ctx: CommandContext, target: string | undefined): CommandResult {
    if (!target) {
      this.print(ctx, ['Usage: chasers cancel <id> or chasers cancel all']);
      return this.success();
    }

    if (target.toLowerCase() === 'all') {
      const activeInstances = chaserOrderManagerInstance.instances.filter((i) => !i.isStopped);
      if (activeInstances.length === 0) {
        this.print(ctx, ['No active chasers to cancel']);
        return this.success();
      }

      for (const instance of activeInstances) {
        chaserOrderManagerInstance.remove(instance.id);
      }

      this.print(ctx, [`Cancelled ${activeInstances.length} chaser(s)`]);
      return this.success();
    }

    // Find by partial ID match
    const instance = chaserOrderManagerInstance.instances.find(
      (i) => i.id === target || i.id.startsWith(target),
    );

    if (!instance) {
      this.print(ctx, [`Chaser not found: ${target}`]);
      return this.success();
    }

    if (instance.isStopped) {
      this.print(ctx, [`Chaser [${target}] is already stopped`]);
      return this.success();
    }

    chaserOrderManagerInstance.remove(instance.id);
    this.print(ctx, [`Cancelled chaser [${instance.id.slice(0, 8)}] for ${instance.options.symbol}`]);
    return this.success();
  }
}
