import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { chaserOrderManagerInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager';
import { twapManager } from '@tealstreet/safe-cex/src/webworker/twap/TwapManager';
import { TwapStatus } from '@tealstreet/safe-cex/src/webworker/twap/TwapStatus';
import { listTasks } from './TasksCommand';

/**
 * `kill`        — kill the most-recent active task across chasers + TWAPs
 * `kill last`   — synonym for `kill` (kills the most-recent task)
 * `kill <id>`   — kill the task whose ID starts with the given prefix
 * `kill all`    — kill every active task
 *
 * Aliases `fuck` and `terminate` are surfaced for ichibot-muscle-memory
 * users — same semantics, same dispatch.
 *
 * "Most recent" = the last active row in `listTasks()`'s output, which
 * is chasers (in array order) followed by TWAPs (Map iteration order).
 * Refuses to fire if there are no active tasks.
 */
export class KillCommand extends BaseCommand {
  readonly name = 'kill';
  readonly description = 'Kill an active task';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = ['fuck', 'terminate'];
  readonly examples = [
    'kill',
    'kill last',
    'kill abc12345',
    'kill all',
    'fuck',
    'fuck last',
    'terminate all',
  ];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const rawTarget = this.getArg(ctx, 1);
    const target = rawTarget?.toLowerCase() === 'last' ? undefined : rawTarget;
    const rows = listTasks();
    const active = rows.filter((r) => r.active);

    if (target === 'all') {
      if (active.length === 0) {
        this.printInfo(ctx, '(no active tasks to kill)');
        return this.success();
      }
      let n = 0;
      for (const row of active) {
        n += killOne(row.id, row.type) ? 1 : 0;
      }
      this.printSuccess(ctx, `Killed ${n} task(s)`);
      return this.success();
    }

    if (!target) {
      if (active.length === 0) {
        return this.error('No active tasks to kill');
      }
      const last = active[active.length - 1];
      const ok = killOne(last.id, last.type);
      if (!ok) return this.error(`Failed to kill ${last.type} ${last.id.slice(0, 8)}`);
      this.printSuccess(ctx, `Killed ${last.type} [${last.id.slice(0, 8)}] (${last.symbol})`);
      return this.success();
    }

    // kill <id-prefix>
    const match = active.find((r) => r.id === target || r.id.startsWith(target));
    if (!match) {
      return this.error(`No active task matches "${target}"`);
    }
    const ok = killOne(match.id, match.type);
    if (!ok) return this.error(`Failed to kill ${match.type} ${match.id.slice(0, 8)}`);
    this.printSuccess(ctx, `Killed ${match.type} [${match.id.slice(0, 8)}] (${match.symbol})`);
    return this.success();
  }
}

function killOne(id: string, type: 'chase' | 'twap'): boolean {
  if (type === 'chase') {
    const inst = chaserOrderManagerInstance.instances.find((i) => i.id === id);
    if (!inst || inst.isStopped) return false;
    chaserOrderManagerInstance.remove(id);
    return true;
  }
  const twap = twapManager.runningTwaps.get(id);
  if (!twap || twap.state.status === TwapStatus.Completed) return false;
  // TwapInstance.terminate is async fire-and-forget; we call it but don't await
  void twap.terminate();
  return true;
}
