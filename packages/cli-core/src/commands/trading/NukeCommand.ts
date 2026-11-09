import { describeStoppedAlgos, nukeWarning } from '@tealstreet/safe-cex/src/nukeResult';
import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * Nuke command - instantly closes all positions and/or cancels all orders
 */
export class NukeCommand extends TradingBaseCommand {
  readonly name = 'nuke';
  readonly description = 'Instantly nuke positions or orders';
  readonly category = CommandCategory.Trading;
  readonly aliases = [];
  readonly examples = ['nuke', 'nuke all', 'nuke positions', 'nuke pos', 'nuke orders', 'nuke ord'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (this.simulationGate(ctx)) return this.success();
    const exchange = ctx.exchange;
    if (!exchange) {
      return this.error('Not connected to exchange');
    }

    const args = this.getRawArgs(ctx);
    const assignment = this.refuseOrderAssignments(ctx);
    if (assignment) return assignment;
    const invalid = this.refuseUnknownArgs(ctx, args[1] ? [1] : []);
    if (invalid) return invalid;

    // The running chasers and TWAPs the nuke stopped first, so none re-places.
    const reportStopped = (result: unknown) => {
      const stopped = describeStoppedAlgos(result);
      if (stopped) this.printWarning(ctx, stopped);
      const warning = nukeWarning(result);
      if (warning) this.printWarning(ctx, warning);
    };

    // Awaited: a one-shot exec exits when this returns, and ok must mean it finished.
    const run = async (operation: string, start: () => Promise<unknown>): Promise<CommandResult> => {
      try {
        reportStopped(await start());
        return this.success();
      } catch (e) {
        const message = `${operation} failed: ${(e as Error).message}`;
        this.printError(ctx, message);
        return this.error(message);
      }
    };

    if (!args[1]) {
      if (!exchange.nuke) {
        return this.error('Exchange does not support nuke operation');
      }
      this.printInfo(ctx, 'Nuking in progress...');
      return run('Nuke', () => exchange.nuke!(3));
    }

    switch (args[1]) {
      case 'all':
        if (!exchange.nuke) {
          return this.error('Exchange does not support nuke operation');
        }
        this.printInfo(ctx, 'Nuking in progress...');
        return run('Nuke', () => exchange.nuke!(3));

      case 'positions':
      case 'pos':
        if (!exchange.nukePositions) {
          return this.error('Exchange does not support nuking positions');
        }
        this.printInfo(ctx, 'Nuking all positions with max retries of 3.');
        return run('Nuke positions', () => exchange.nukePositions!(3));

      case 'orders':
      case 'ord':
        if (!exchange.nukeOrders) {
          return this.error('Exchange does not support nuking orders');
        }
        this.printInfo(ctx, 'Nuking all orders.');
        return run('Nuke orders', () => exchange.nukeOrders!());

      default:
        this.printError(ctx, 'Malformed command, correct syntax is nuke <positions|pos|orders|ord|all>');
        return this.error(`Invalid syntax: ${this.unknownOrderArgument(args[1])}`);
    }
  }
}
