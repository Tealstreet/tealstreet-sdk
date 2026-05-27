import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `alias list`                — list all aliases
 * `alias <name> "<command>"`  — create or update an alias (the body is
 *   stored verbatim and resolved at *use* time, so `alias x "buy at
 *   $entry"` re-resolves `$entry` on every invocation).
 *
 * Quotes around the body are optional but recommended when the body
 * contains `;`, `&`, or quotes — those would otherwise break the chain
 * tokenizer. The preprocess layer already preserves the segment
 * verbatim for `alias`, so what reaches us is exactly what the user
 * typed.
 */
export class AliasCommand extends BaseCommand {
  readonly name = 'alias';
  readonly description = 'List or define a command alias';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = [
    'alias list',
    'alias b1 "buy 100$"',
    'alias z "buy at $entry -50; sleep 5s; close"',
  ];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('alias: preprocess store unavailable');
    }
    const sub = this.getArg(ctx, 1);
    if (!sub || sub === 'list') {
      const aliases = ctx.preprocessStore.getAliases();
      const entries = Object.entries(aliases);
      if (entries.length === 0) {
        this.printInfo(ctx, '(no aliases defined)');
        return this.success();
      }
      this.print(
        ctx,
        entries.sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => `${k} = ${v}`),
      );
      return this.success();
    }

    const name = sub;
    if (!/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(name)) {
      this.printError(ctx, 'alias name must start with a letter and contain only letters, digits, or underscore');
      return this.error('Invalid alias name');
    }

    const body = stripOuterQuotes(ctx.args._.slice(2).join(' ').trim());
    if (!body) {
      this.printError(ctx, 'Usage: alias <name> "<command>"');
      return this.error('Missing alias body');
    }

    ctx.preprocessStore.setAlias(name, body);
    this.printSuccess(ctx, `${name} = ${body}`);
    return this.success();
  }
}

function stripOuterQuotes(s: string): string {
  if (s.length < 2) return s;
  const first = s[0];
  const last = s[s.length - 1];
  if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
    return s.slice(1, -1);
  }
  return s;
}
