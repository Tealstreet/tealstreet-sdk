import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { OrderSide } from '@tealstreet/safe-cex/src/types';
import { chaserOrderManagerInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager';
import { twapManager } from '@tealstreet/safe-cex/src/webworker/twap/TwapManager';
import { TwapStatus } from '@tealstreet/safe-cex/src/webworker/twap/TwapStatus';

export interface TaskRow {
  id: string;
  type: 'chase' | 'twap';
  status: string;
  symbol: string;
  side: 'BUY' | 'SELL';
  progress: string;
  active: boolean;
}

/**
 * Build a unified row list across all task types. Insertion order is
 * preserved: chasers first (in `instances` array order), then TWAPs (in
 * `runningTwaps` Map iteration order). The last entry is the most
 * recently created task — `kill` (no args) targets it.
 */
export function listTasks(): TaskRow[] {
  const rows: TaskRow[] = [];

  for (const c of chaserOrderManagerInstance.instances) {
    const filled = c.options.size - c.remainingSize;
    const pct = c.options.size > 0 ? (filled / c.options.size) * 100 : 0;
    rows.push({
      id: c.id,
      type: 'chase',
      status: c.isStopped ? c.state.stopReason || 'stopped' : 'active',
      symbol: c.options.symbol,
      side: c.options.side === OrderSide.Buy ? 'BUY' : 'SELL',
      progress: `${pct.toFixed(1)}% filled`,
      active: !c.isStopped,
    });
  }

  for (const t of twapManager.runningTwaps.values()) {
    const pct =
      t.options.ordersCount > 0 ? (t.state.ordersExecuted / t.options.ordersCount) * 100 : 0;
    rows.push({
      id: t.id,
      type: 'twap',
      status: t.state.status === TwapStatus.Running
        ? 'running'
        : t.state.status === TwapStatus.Paused
          ? 'paused'
          : t.state.status === TwapStatus.Completed
            ? 'completed'
            : String(t.state.status),
      symbol: t.options.activeMarket.symbol,
      side: t.options.side === OrderSide.Buy ? 'BUY' : 'SELL',
      progress: `${t.state.ordersExecuted}/${t.options.ordersCount} slices (${pct.toFixed(1)}%)`,
      active: t.state.status !== TwapStatus.Completed,
    });
  }

  return rows;
}

/**
 * `tasks` — list all active CLI tasks (chasers + TWAPs in one view).
 */
export class TasksCommand extends BaseCommand {
  readonly name = 'tasks';
  readonly description = 'List all active CLI tasks';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = ['tasks'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const rows = listTasks();
    if (rows.length === 0) {
      this.printInfo(ctx, '(no active tasks)');
      return this.success();
    }
    const lines = rows.map((r) => {
      const shortId = r.id.slice(0, 8);
      const status = r.active ? r.status : `[${r.status}]`;
      return `[${shortId}] ${r.type.padEnd(5)} ${r.symbol} ${r.side}  ${r.progress}  ${status}`;
    });
    const active = rows.filter((r) => r.active).length;
    const completed = rows.length - active;
    lines.push(`(active: ${active}  completed: ${completed})`);
    this.print(ctx, lines);
    return this.success();
  }
}
