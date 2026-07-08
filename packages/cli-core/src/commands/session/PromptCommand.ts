import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import {
  PROMPT_PRESETS,
  listPromptPresets,
  getPromptPreset,
  formatPrompt,
  type PromptConfig,
} from '../../utils/PromptFormatter';

/**
 * Prompt command - configure the CLI prompt format
 *
 * Usage:
 *   prompt                     - Show current prompt config and available tokens
 *   prompt list                - List available presets
 *   prompt preset <name>       - Use a preset (minimal, default, detailed, trader, compact)
 *   prompt set <format>        - Set custom format string
 *   prompt prefix <prefix>     - Set prompt prefix
 *   prompt preview             - Preview current prompt with sample data
 *
 * Available tokens:
 *   {symbol}   - Current symbol (BTCUSDT)
 *   {account}  - Account name
 *   {price}    - Current price ($97,234)
 *   {side}     - Position side (↑/↓)
 *   {size}     - Position size (0.5 BTC)
 *   {entry}    - Entry price ($96,500)
 *   {pnl}      - Unrealized PnL (+$234)
 *   {pnl%}     - PnL percentage (+2.5%)
 *   {liq}      - Liquidation price ($85,000)
 *   {leverage} - Leverage (10x)
 *   {orders}   - Open order count (3)
 *   {conn}     - Connection status ([!], [SLOW])
 */
export class PromptCommand extends BaseCommand {
  readonly name = 'prompt';
  readonly description = 'Configure the CLI prompt format';
  readonly category = CommandCategory.Session;
  // NOTE: `ps` intentionally removed — it collides with PositionsCommand's
  // `ps` alias, which is the winner in the default registration order and the
  // intuitive one. `prompt` has no short alias.
  readonly aliases: string[] = [];
  readonly examples = [
    'prompt',
    'prompt list',
    'prompt preset detailed',
    'prompt set "[{symbol} {price} {pnl%}]> "',
    'prompt prefix ts',
    'prompt preview',
  ];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const subcommand = this.getArg(ctx, 1)?.toLowerCase();

    switch (subcommand) {
      case 'list':
        return this.listPresets(ctx);
      case 'preset':
        return this.setPreset(ctx);
      case 'set':
        return this.setFormat(ctx);
      case 'prefix':
        return this.setPrefix(ctx);
      case 'preview':
        return this.previewPrompt(ctx);
      default:
        return this.showHelp(ctx);
    }
  }

  private showHelp(ctx: CommandContext): CommandResult {
    const currentConfig = ctx.promptConfig || PROMPT_PRESETS.default;

    this.print(ctx, [
      '',
      'Prompt Configuration',
      '═'.repeat(60),
      '',
      'Current format:',
      `  Format: ${currentConfig.format}`,
      `  Prefix: ${currentConfig.prefix || '(none)'}`,
      '',
      'Available tokens:',
      '  {symbol}   - Current symbol (e.g., BTCUSDT)',
      '  {account}  - Account name',
      '  {price}    - Current price (e.g., $97,234)',
      '  {side}     - Position side (↑ for long, ↓ for short)',
      '  {size}     - Position size with currency (e.g., 0.5 BTC)',
      '  {entry}    - Entry price (e.g., $96,500)',
      '  {pnl}      - Unrealized PnL (e.g., +$234.50)',
      '  {pnl%}     - PnL as percentage (e.g., +2.5%)',
      '  {liq}      - Liquidation price (e.g., $85,000)',
      '  {leverage} - Position leverage (e.g., 10x)',
      '  {orders}   - Open order count (e.g., 3)',
      '  {conn}     - Connection status indicator',
      '',
      'Commands:',
      '  prompt list              - List available presets',
      '  prompt preset <name>     - Use a preset',
      '  prompt set "<format>"    - Set custom format',
      '  prompt prefix <prefix>   - Set prefix (e.g., "ts")',
      '  prompt preview           - Preview with sample data',
      '',
      'Example formats:',
      '  "[{symbol}]> "                           - Minimal',
      '  "[{symbol} {price} {pnl%}]> "            - With PnL',
      '  "{side} {size} @ {entry} → {price}> "   - Trade view',
      '',
    ]);

    return this.success();
  }

  private listPresets(ctx: CommandContext): CommandResult {
    const presets = listPromptPresets();
    const lines: string[] = ['', 'Available prompt presets:', '─'.repeat(60)];

    for (const name of presets) {
      const preset = PROMPT_PRESETS[name];
      const prefix = preset.prefix ? `${preset.prefix} ` : '';
      lines.push(`  ${name.padEnd(12)} ${prefix}${preset.format}`);
    }

    lines.push('', 'Use: prompt preset <name>');
    lines.push('');

    this.print(ctx, lines);
    return this.success();
  }

  private setPreset(ctx: CommandContext): CommandResult {
    const presetName = this.getArg(ctx, 2)?.toLowerCase();

    if (!presetName) {
      this.printError(ctx, 'Usage: prompt preset <name>');
      this.printInfo(ctx, 'Available: ' + listPromptPresets().join(', '));
      return this.error('Missing preset name');
    }

    const preset = getPromptPreset(presetName);
    if (!preset) {
      this.printError(ctx, `Unknown preset: ${presetName}`);
      this.printInfo(ctx, 'Available: ' + listPromptPresets().join(', '));
      return this.error('Unknown preset');
    }

    this.printSuccess(ctx, `Prompt preset set to: ${presetName}`);
    this.printInfo(ctx, `Format: ${preset.format}`);

    return {
      success: true,
      updateSession: {
        promptConfig: preset,
      },
    };
  }

  private setFormat(ctx: CommandContext): CommandResult {
    // Get everything after "prompt set " as the format string
    const args = ctx.args._.slice(2).join(' ');

    if (!args) {
      this.printError(ctx, 'Usage: prompt set "<format>"');
      this.printInfo(ctx, 'Example: prompt set "[{symbol} {price}]> "');
      return this.error('Missing format');
    }

    // Remove quotes if present
    const format = args.replace(/^["']|["']$/g, '');

    const currentConfig = ctx.promptConfig || PROMPT_PRESETS.default;
    const newConfig: PromptConfig = {
      ...currentConfig,
      format,
    };

    this.printSuccess(ctx, 'Prompt format updated');
    this.printInfo(ctx, `New format: ${format}`);

    return {
      success: true,
      updateSession: {
        promptConfig: newConfig,
      },
    };
  }

  private setPrefix(ctx: CommandContext): CommandResult {
    const prefix = this.getArg(ctx, 2) || '';

    const currentConfig = ctx.promptConfig || PROMPT_PRESETS.default;
    const newConfig: PromptConfig = {
      ...currentConfig,
      prefix,
    };

    this.printSuccess(ctx, `Prompt prefix set to: "${prefix}"`);

    return {
      success: true,
      updateSession: {
        promptConfig: newConfig,
      },
    };
  }

  private previewPrompt(ctx: CommandContext): CommandResult {
    const config = ctx.promptConfig || PROMPT_PRESETS.default;

    // Generate prompt with current exchange data
    const { prompt } = formatPrompt({
      accountName: ctx.accountName,
      symbol: ctx.symbol,
      exchange: ctx.exchange,
      config,
    });

    this.print(ctx, [
      '',
      'Prompt preview:',
      '─'.repeat(60),
      prompt,
      '─'.repeat(60),
      '',
      `Format: ${config.format}`,
      `Prefix: ${config.prefix || '(none)'}`,
      '',
    ]);

    return this.success();
  }
}
