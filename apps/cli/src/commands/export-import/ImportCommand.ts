import { existsSync, readFileSync } from 'fs';
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

interface ImportPayload {
  version?: number;
  vars?: Record<string, string>;
  aliases?: Record<string, string>;
  binds?: Record<string, string>;
}

/**
 * `import [path]` — load user vars + aliases from a JSON file written
 * by `export`. Default path is `~/.tealstreet/cli-export.json`.
 *
 * Merges into the current store: existing names are overwritten, names
 * not in the import are kept. Pass `--clear` to wipe existing first.
 *
 * `binds` in the payload is read but ignored for now (hotkeys aren't
 * implemented yet — see ROADMAP.md).
 */
export class ImportCommand extends BaseCommand {
  readonly name = 'import';
  readonly description = 'Import user vars and aliases from a JSON file';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = ['import', 'import ./my-cli.json', 'import ./my-cli.json --clear'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('import: preprocess store unavailable');
    }
    const target = this.getArg(ctx, 1);
    const path = target ? resolve(target) : DEFAULT_PATH;
    if (!existsSync(path)) {
      return this.error(`File not found: ${path}`);
    }

    let payload: ImportPayload;
    try {
      const raw = readFileSync(path, 'utf-8');
      payload = JSON.parse(raw) as ImportPayload;
    } catch (e) {
      return this.error(`Failed to parse ${path}: ${(e as Error).message}`);
    }

    if (payload.version !== undefined && payload.version !== 1) {
      this.printWarning(
        ctx,
        `Import file version ${payload.version} is newer than supported (1) — fields may be ignored`,
      );
    }

    if (this.hasFlag(ctx, 'clear')) {
      ctx.preprocessStore.unsetAllVars();
      ctx.preprocessStore.unsetAllAliases();
    }

    let varCount = 0;
    if (payload.vars && typeof payload.vars === 'object') {
      for (const [k, v] of Object.entries(payload.vars)) {
        if (typeof v === 'string') {
          ctx.preprocessStore.setVar(k, v);
          varCount++;
        }
      }
    }

    let aliasCount = 0;
    if (payload.aliases && typeof payload.aliases === 'object') {
      for (const [k, v] of Object.entries(payload.aliases)) {
        if (typeof v === 'string') {
          ctx.preprocessStore.setAlias(k, v);
          aliasCount++;
        }
      }
    }

    this.printSuccess(ctx, `Imported ${varCount} var(s) + ${aliasCount} alias(es) from ${path}`);
    return this.success();
  }
}
