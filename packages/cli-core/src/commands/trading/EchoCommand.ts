import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * Echo command — prints arguments after preprocess substitution. Useful
 * for inspecting `@vars`, `$constants`, and tracker / live var values
 * (`echo @hi` resolves the tracker AND updates its extreme).
 *
 * `say` is registered as an alias for ergonomic-message ichibot
 * parity (`say "peak pnl is @hi"`); behaviour is identical.
 */
export class EchoCommand extends BaseCommand {
  readonly name = 'echo';
  readonly description = 'Echo arguments (after var/constant substitution)';
  readonly category = CommandCategory.Session;
  readonly aliases = ['say'];
  readonly examples = ['echo hello world', 'echo $symbol', 'say "peak pnl is @hi"'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const args = this.getArgs(ctx);
    this.print(ctx, [args.join(' ')]);
    return this.success();
  }
}
