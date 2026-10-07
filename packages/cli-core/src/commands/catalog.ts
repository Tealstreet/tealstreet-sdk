import { CommandAvailability } from '../types';
import { ensureCommandsRegistered } from './CommandExecutor';
import { commandRegistry } from './CommandRegistry';

export type CommandCatalogEntry = {
  name: string;
  aliases: string[];
  category: string;
  description: string;
  examples: string[];
  /** Needs a live exchange connection (an account) to run. */
  requiresConnection: boolean;
  /** `repl_only` commands take over the terminal and cannot run one-shot. */
  availability: CommandAvailability;
};

/**
 * Every shared command, described from the registry itself so a reader such as
 * an agent or `tealstreet commands --json` can never drift from what runs.
 */
export const buildCommandCatalog = (): CommandCatalogEntry[] => {
  ensureCommandsRegistered();
  return commandRegistry
    .getAll()
    .map((command) => ({
      name: command.name,
      aliases: [...command.aliases],
      category: String(command.category),
      description: command.description,
      examples: [...command.examples],
      requiresConnection: command.requiresConnection,
      availability: command.availability ?? CommandAvailability.BOTH,
    }))
    .sort((a, b) => a.name.localeCompare(b.name));
};
