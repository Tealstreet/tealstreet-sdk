import { writeFileSync } from 'fs';
import { resolve } from 'path';
import { homedir } from 'os';
import { join } from 'path';
import { BaseCommand, CommandCategory } from '@tealstreet/cli-core';
// NewCommandResult is the BaseCommand-aligned shape; the public `CommandResult`
// export from `@tealstreet/cli-core` still points at the legacy result type and
// is incompatible with `BaseCommand.execute`. Tracked as cross-package drift.
import type { CommandContext, NewCommandResult as CommandResult } from '@tealstreet/cli-core';
import { BRAND } from '../../brand';

const DEFAULT_PATH = join(homedir(), BRAND.configDirName, 'cli-export.json');
const FORMAT_VERSION = 1;

/**
 * `export [path]` — write user vars + aliases to a JSON file.
 *
 * Default path is `~/.tealstreet/cli-export.json`. The shape is
 * `{ version, vars, aliases }`. Binds are reserved in the format for
 * future hotkey support but stored as `{}` for now.
 */
export class ExportCommand extends BaseCommand {
  readonly name = 'export';
  readonly description = 'Export user vars and aliases to a JSON file';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = ['export', 'export ./my-cli.json'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('export: preprocess store unavailable');
    }
    const target = this.getArg(ctx, 1);
    const path = target ? resolve(target) : DEFAULT_PATH;
    const payload = {
      version: FORMAT_VERSION,
      vars: ctx.preprocessStore.getVars(),
      aliases: ctx.preprocessStore.getAliases(),
      binds: {},
    };
    try {
      writeFileSync(path, JSON.stringify(payload, null, 2) + '\n', 'utf-8');
    } catch (e) {
      return this.error(`Failed to write ${path}: ${(e as Error).message}`);
    }
    const varCount = Object.keys(payload.vars).length;
    const aliasCount = Object.keys(payload.aliases).length;
    this.printSuccess(ctx, `Exported ${varCount} var(s) + ${aliasCount} alias(es) → ${path}`);
    return this.success();
  }
}
