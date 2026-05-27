import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `simulation`               — show current state
 * `simulation on`            — enter dry-run (write commands log + skip)
 * `simulation off`           — leave dry-run (live trading resumes)
 * `simulation status`        — same as `simulation` (no-arg)
 *
 * Aliases: `sim`, `dry-run`.
 *
 * Persisted via PreprocessStore so a reload preserves the flag. The
 * REPL prompt should surface the state visually (`[SIM]` prefix) —
 * this command only flips the bit and prints confirmation.
 */
export class SimulationCommand extends BaseCommand {
  readonly name = 'simulation';
  readonly description = 'Toggle dry-run mode (write commands log + skip)';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = ['sim', 'dry-run'];
  readonly examples = ['simulation on', 'simulation off', 'simulation', 'sim status'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('simulation: preprocess store unavailable');
    }
    const target = (this.getArg(ctx, 1) ?? '').toLowerCase();

    if (!target || target === 'status') {
      const on = ctx.preprocessStore.getSimulation();
      this.printInfo(ctx, on ? 'simulation: ON (dry-run)' : 'simulation: OFF (live)');
      return this.success();
    }

    if (target === 'on' || target === 'true' || target === '1') {
      ctx.preprocessStore.setSimulation(true);
      this.printSuccess(ctx, 'simulation ON — write commands will log + skip');
      return this.success();
    }
    if (target === 'off' || target === 'false' || target === '0') {
      ctx.preprocessStore.setSimulation(false);
      this.printSuccess(ctx, 'simulation OFF — live trading resumed');
      return this.success();
    }

    this.printError(ctx, `Usage: simulation <on|off|status> (got '${target}')`);
    return this.error('Invalid argument');
  }
}
