import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

const ANSI_CLEAR_SCREEN = '\x1b[2J\x1b[H';

/**
 * `clear` — clear the terminal screen.
 *
 * Wipes scrollback-visible content via the standard ANSI clear-screen
 * + move-cursor-home sequence. Web platform adapters that don't drive
 * a real terminal can override `printRaw` to no-op or to clear their
 * virtual buffer; the default falls back to printing the sequence.
 */
export class ClearCommand extends BaseCommand {
  readonly name = 'clear';
  readonly description = 'Clear the console';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = ['cls'];
  readonly examples = ['clear'];
  readonly requiresConnection = false;

  async execute(_ctx: CommandContext): Promise<CommandResult> {
    process.stdout.write(ANSI_CLEAR_SCREEN);
    return this.success();
  }
}
