import { describeStoppedAlgos } from '@tealstreet/safe-cex/src/nukeResult';
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

    const logNukeError = (operation: string) => (e: unknown) => {
      this.printWarning(ctx, `Warning: ${operation} may have failed: ${(e as Error).message}`);
    };
    // The running chasers and TWAPs the nuke stopped first, so none re-places.
    const reportStopped = (result: unknown) => {
      const stopped = describeStoppedAlgos(result);
      if (stopped) this.printWarning(ctx, stopped);
    };

    if (!args[1]) {
      if (!exchange.nuke) {
        return this.error('Exchange does not support nuke operation');
      }
      exchange.nuke(3).then(reportStopped, logNukeError('Nuke'));
      this.printInfo(ctx, 'Nuking in progress...');
      return this.success();
    }

    switch (args[1]) {
      case 'all':
        if (!exchange.nuke) {
          return this.error('Exchange does not support nuke operation');
        }
        exchange.nuke(3).then(reportStopped, logNukeError('Nuke'));
        this.printInfo(ctx, 'Nuking in progress...');
        return this.success();

      case 'positions':
      case 'pos':
        if (!exchange.nukePositions) {
          return this.error('Exchange does not support nuking positions');
        }
        exchange.nukePositions(3).then(reportStopped, logNukeError('Nuke positions'));
        this.printInfo(ctx, 'Nuking all positions with max retries of 3.');
        return this.success();

      case 'orders':
      case 'ord':
        if (!exchange.nukeOrders) {
          return this.error('Exchange does not support nuking orders');
        }
        exchange.nukeOrders().then(reportStopped, logNukeError('Nuke orders'));
        this.printInfo(ctx, 'Nuking all orders.');
        return this.success();

      default:
        this.printError(ctx, 'Malformed command, correct syntax is nuke <positions|pos|orders|ord|all>');
        return this.error('Invalid syntax');
    }
  }
}
