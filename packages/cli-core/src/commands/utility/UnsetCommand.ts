import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `unset @<name>` — remove a single user variable. Clears whichever
 *                   kind currently holds the name (static / reactive /
 *                   tracked). Storage mutex guarantees at most one
 *                   binding so we try all three; cosmetic order.
 * `unset all`     — clear every user variable (all three kinds).
 *
 * Use `untrack @<name>` / `untrack all` for the kind-specific
 * remove if you don't want to touch sibling static / live bindings
 * — but those bindings would already be cleared by the mutex when
 * `track` was registered, so in practice `unset @<name>` is the
 * one-stop shop.
 */
export class UnsetCommand extends BaseCommand {
  readonly name = 'unset';
  readonly description = 'Remove a user variable, or `unset all`';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = ['unset @size', 'unset all'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('unset: preprocess store unavailable');
    }
    const target = this.getArg(ctx, 1);
    if (!target) {
      this.printError(ctx, 'Usage: unset @<name>  or  unset all');
      return this.error('Missing target');
    }

    if (target === 'all') {
      // Clear ALL three kinds — the user typed `unset all`, they
      // don't care which kind a given name was. Symmetric with the
      // three-way `set` / `sets` / `track` mutex at storage.
      ctx.preprocessStore.unsetAllVars();
      ctx.preprocessStore.unsetAllLiveVars();
      ctx.preprocessStore.unsetAllTrackedVars();
      this.printSuccess(ctx, 'All variables unset');
      return this.success();
    }

    if (!target.startsWith('@') || target.length < 2) {
      this.printError(ctx, 'Usage: unset @<name>  or  unset all');
      return this.error('Invalid variable name');
    }

    const name = target.slice(1);
    // Try all three kinds. Mutex at storage means at most one
    // returns true, so the order is purely cosmetic.
    const removed =
      ctx.preprocessStore.unsetVar(name) ||
      ctx.preprocessStore.unsetLiveVar(name) ||
      ctx.preprocessStore.unsetTrackedVar(name);
    if (!removed) {
      this.printWarning(ctx, `@${name} is not set`);
      return this.success();
    }
    this.printSuccess(ctx, `@${name} unset`);
    return this.success();
  }
}
