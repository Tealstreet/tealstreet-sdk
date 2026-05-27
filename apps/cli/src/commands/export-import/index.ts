import { commandRegistry } from '@tealstreet/cli-core';
import { ExportCommand } from './ExportCommand';
import { ImportCommand } from './ImportCommand';

export { ExportCommand } from './ExportCommand';
export { ImportCommand } from './ImportCommand';

let registered = false;

/**
 * Register the CLI-only `export` / `import` commands. They live in the
 * apps/cli tree (not cli-core) because they touch the filesystem; the
 * web CLI can replace them with platform-appropriate equivalents
 * later if needed.
 */
export function registerExportImportCommands(): void {
  if (registered) return;
  commandRegistry.registerAll([new ExportCommand(), new ImportCommand()]);
  registered = true;
}
