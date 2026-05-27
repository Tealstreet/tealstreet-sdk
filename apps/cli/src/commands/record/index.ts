/**
 * Record Commands Index
 *
 * Exports and registers the unified record command.
 */

import { commandRegistry } from '@tealstreet/cli-core';
import { RecordCommand } from './RecordCommand';

// Export command class
export { RecordCommand } from './RecordCommand';

// Flag to track registration
let recordCommandsRegistered = false;

/**
 * Register record command with the command registry.
 * Should be called during CLI initialization.
 */
export function registerRecordCommands(): void {
  if (recordCommandsRegistered) return;

  commandRegistry.registerAll([new RecordCommand()]);

  recordCommandsRegistered = true;
}
