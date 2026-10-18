import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { commandRegistry } from '../CommandRegistry';
import { getCliBrand } from '../../brand';

/**
 * Help command - displays available commands and usage
 */
export class HelpCommand extends BaseCommand {
  readonly name = 'help';
  readonly description = 'Show available commands and usage';
  readonly category = CommandCategory.Session;
  readonly aliases = ['?', 'h'];
  readonly examples = ['help', 'help buy', 'help accounts'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const subCommand = this.getArg(ctx, 1);

    if (subCommand) {
      return this.showCommandHelp(ctx, subCommand);
    }

    return this.showGeneralHelp(ctx);
  }

  private showGeneralHelp(ctx: CommandContext): CommandResult {
    const isEmbedded = ctx.adapter.platformType === 'embedded';

    const brand = getCliBrand();
    const lines: string[] = ['', `${brand.displayName} CLI Commands:`, ''];

    // Authentication - standalone only
    if (!isEmbedded) {
      lines.push(
        'Authentication:',
        `  login              Authenticate with ${brand.displayName}`,
        '  logout             Log out',
        '  auth, whoami       Show authentication status',
        '',
      );
    }

    // Accounts section - different for embedded vs standalone
    if (isEmbedded) {
      lines.push(
        'Accounts:',
        '  accounts           List connected accounts',
        '  account <name>     Switch to account by name',
        '  account -# <n>     Switch to account by number',
        '  exchanges          List supported exchanges',
        '',
      );
    } else {
      lines.push(
        'Accounts:',
        '  accounts           List configured accounts',
        '  accounts add       Add a new account',
        '  accounts remove    Remove an account',
        '  accounts enable    Enable multi-account streaming',
        '  accounts disable   Disable multi-account streaming',
        '  account <name>     Switch to account by name',
        '  account -# <n>     Switch to account by number',
        '  exchanges          List supported exchanges',
        '',
      );
    }

    // Session section - different for embedded
    if (isEmbedded) {
      lines.push(
        'Session:',
        '  symbol <symbol>    Change trading symbol (alias: switch)',
        '  markets            List available markets',
        '  status             Show current connection status',
        '',
      );
    } else {
      lines.push(
        'Session:',
        '  symbol <symbol>    Change trading symbol (alias: switch)',
        '  markets            List available markets',
        '  status             Show current connection status',
        '  prompt             Configure prompt format',
        '  connection         Show connection health details',
        '  logs               Show log file path',
        '  exit               Exit the CLI',
        '',
      );
    }

    // Trading - same for both
    lines.push(
      'Trading:',
      '  buy (b)            Buy market/limit order',
      '  sell (s)           Sell market/limit order',
      '  close (c)          Close position',
      '  cancel (x)         Cancel orders',
      '  scale              Scale into position',
      '  chase              Chase order',
      '  chasers            List active chasers',
      '  stop               Place stop order',
      '  nuke               Cancel all & close all',
      '',
    );

    // Info - same for both
    lines.push(
      'Info:',
      '  balance (bal)      Show balance',
      '  position (p)       Show current position',
      '  positions (ps)     Show all positions',
      '  last-price (l)     Show last price',
      '  mark-price (m)     Show mark price',
      '  index-price (i)    Show index price',
      '',
    );

    // Watch modes - standalone only (requires terminal control)
    if (!isEmbedded) {
      lines.push(
        'Watch Modes:',
        '  watch account (wa) Live position/order/margin display',
        '  watch positions (wp) Live positions table',
        '  watch orders (wo)  Live orders table',
        '  watch margin (wm)  Live margin display',
        '  watch chart        Live updating chart',
        '  watch chase        Live chaser monitoring',
        '',
      );
    }

    // Charts - standalone only
    if (!isEmbedded) {
      lines.push('Charts:', '  chart              ASCII candlestick chart', '');
    }

    // System section - different for embedded
    if (isEmbedded) {
      lines.push('System:', '  clear              Clear terminal', '  help               Show this help', '');
    } else {
      lines.push(
        'System:',
        '  update             Update CLI to latest version',
        '  help               Show this help',
        '',
      );
    }

    lines.push('Type "help <command>" for examples', '');

    if (isEmbedded) {
      lines.push(
        `Full reference + recipes: ${brand.docsUrl}`,
        'Standalone-only surface (watch modes, hooks, recording, listener) lives there.',
        '',
      );
    } else {
      lines.push(`Full reference + recipes: ${brand.docsUrl}`, '');
    }

    this.print(ctx, lines);

    return this.success();
  }

  private showCommandHelp(ctx: CommandContext, commandName: string): CommandResult {
    const command = commandRegistry.find(commandName);

    if (!command) {
      this.printWarning(ctx, `Unknown command: ${commandName}`);
      return this.success();
    }

    this.print(ctx, ['', `Command: ${command.name}`, '']);
    this.print(ctx, [`  ${command.description}`]);

    if (command.aliases.length > 0) {
      this.print(ctx, ['', `  Aliases: ${command.aliases.join(', ')}`]);
    }

    if (command.examples.length > 0) {
      this.print(ctx, ['', '  Examples:']);
      for (const example of command.examples) {
        this.print(ctx, [`    ${example}`]);
      }
    }

    this.print(ctx, ['']);
    return this.success();
  }
}
