import { describe, expect, it } from 'vitest';

import { commandRegistry } from './CommandRegistry';
import { executeCommand } from './CommandExecutor';
import { BaseCommand } from './BaseCommand';
import { CommandCategory, type CommandResult } from './Command';

/**
 * One failed order used to announce itself five times, in five wordings:
 *
 *   ✗ bybitdemo error You must sign the required agreement...
 *   ✗ You must sign the required agreement...
 *   ✗ Exchange error: Exchange Error: You must sign the required agreement...
 *   Command failed: You must sign the required agreement...
 *   Error in command: You must sign the required agreement...
 *
 * Two of those are structural and guaranteed for EVERY failing command: the
 * executor prints, and then the host has to convert `success === false` into a
 * throw (cli-core/CLAUDE.md, "Runner failure contract") and printed it again
 * from its own catch. `errorReported` is how the host knows to stay quiet.
 * Reported from Discord 2026-09-28.
 */
class ThrowingCommand extends BaseCommand {
  readonly name = '__throws__';
  readonly description = 'test';
  readonly category = CommandCategory.Session;
  readonly requiresConnection = false;
  async execute(): Promise<CommandResult> {
    throw new Error('You must sign the required agreement before trading this contract.');
  }
}

class FailingCommand extends BaseCommand {
  readonly name = '__fails__';
  readonly description = 'test';
  readonly category = CommandCategory.Session;
  readonly requiresConnection = false;
  async execute(): Promise<CommandResult> {
    return this.error('nope');
  }
}

const makeCtx = () => {
  const printed: string[] = [];
  return {
    printed,
    adapter: {
      printError: (m: string) => printed.push(m),
      printInfo: (m: string) => printed.push(m),
      printWarning: (m: string) => printed.push(m),
      print: (m: string) => printed.push(m),
    } as never,
  };
};

describe('executeCommand error reporting', () => {
  it('flags a thrown error as already shown to the user', async () => {
    commandRegistry.register(new ThrowingCommand());
    const { adapter, printed } = makeCtx();

    const result = await executeCommand('__throws__', 'BTCUSDT', 'a', null, adapter);

    expect(result.success).toBe(false);
    expect(result.errorReported).toBe(true);
    // Exactly one line from this layer — the host must not add a second.
    expect(printed.filter((p) => p.includes('required agreement'))).toHaveLength(1);
  });

  it('flags a returned failure too — most commands return rather than throw', async () => {
    commandRegistry.register(new FailingCommand());
    const { adapter } = makeCtx();

    const result = await executeCommand('__fails__', 'BTCUSDT', 'a', null, adapter);

    expect(result.success).toBe(false);
    expect(result.error).toBe('nope');
  });
});
