import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock the cli-core runtime deps the router pulls in
vi.mock('../commands/CommandExecutor', () => ({
  executeCommand: vi.fn(),
}));
vi.mock('../table/Table', () => ({
  setTableRenderMode: vi.fn(),
}));

import { commandRouter, routeCommands } from './router';
import { executeCommand } from '../commands/CommandExecutor';
import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';

const mockExecuteCommand = vi.mocked(executeCommand);

describe('router', () => {
  let mockExchange: Exchange;
  let mockEmit: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();

    mockEmit = vi.fn();
    mockExchange = {
      name: 'bybit',
      emitter: {
        emit: mockEmit,
      },
    } as unknown as Exchange;
  });

  describe('commandRouter', () => {
    it('should call executeCommand with correct parameters', async () => {
      mockExecuteCommand.mockResolvedValueOnce({ handled: true });

      await commandRouter(mockExchange, 'BTCUSDT', 'buy 1');

      expect(mockExecuteCommand).toHaveBeenCalledWith(
        'buy 1',
        'BTCUSDT',
        undefined, // accountName not used in webworker context
        expect.anything(), // exchange passed through
        expect.anything(), // adapter
      );
    });

    it('should emit exec-log when command is not handled', async () => {
      mockExecuteCommand.mockResolvedValueOnce({ handled: false });

      await commandRouter(mockExchange, 'BTCUSDT', 'unknowncmd arg');

      expect(mockEmit).toHaveBeenCalledWith('exec-log', {
        message: 'Unknown command: unknowncmd',
      });
    });

    it('should not emit when command is handled', async () => {
      mockExecuteCommand.mockResolvedValueOnce({ handled: true });

      await commandRouter(mockExchange, 'BTCUSDT', 'buy 1');

      expect(mockEmit).not.toHaveBeenCalled();
    });

    it('should extract first word for unknown command message', async () => {
      mockExecuteCommand.mockResolvedValueOnce({ handled: false });

      await commandRouter(mockExchange, 'ETHUSDT', 'someweirdcommand with many args');

      expect(mockEmit).toHaveBeenCalledWith('exec-log', {
        message: 'Unknown command: someweirdcommand',
      });
    });
  });

  describe('routeCommands', () => {
    it('should split commands by semicolon', async () => {
      mockExecuteCommand.mockResolvedValue({ handled: true });

      await routeCommands(mockExchange, 'BTCUSDT', 'buy 1; sell 1');

      expect(mockExecuteCommand).toHaveBeenCalledTimes(2);
    });

    it('should trim whitespace from commands', async () => {
      mockExecuteCommand.mockResolvedValue({ handled: true });

      await routeCommands(mockExchange, 'BTCUSDT', '  buy 1  ;  sell 1  ');

      // First call
      expect(mockExecuteCommand).toHaveBeenNthCalledWith(
        1,
        'buy 1',
        'BTCUSDT',
        undefined,
        expect.anything(),
        expect.anything(),
      );
      // Second call
      expect(mockExecuteCommand).toHaveBeenNthCalledWith(
        2,
        'sell 1',
        'BTCUSDT',
        undefined,
        expect.anything(),
        expect.anything(),
      );
    });

    it('should filter out empty commands', async () => {
      mockExecuteCommand.mockResolvedValue({ handled: true });

      await routeCommands(mockExchange, 'BTCUSDT', 'buy 1;;; sell 1');

      expect(mockExecuteCommand).toHaveBeenCalledTimes(2);
    });

    it('should handle single command', async () => {
      mockExecuteCommand.mockResolvedValue({ handled: true });

      await routeCommands(mockExchange, 'BTCUSDT', 'buy 1');

      expect(mockExecuteCommand).toHaveBeenCalledTimes(1);
    });

    it('should handle empty string', async () => {
      await routeCommands(mockExchange, 'BTCUSDT', '');

      expect(mockExecuteCommand).not.toHaveBeenCalled();
    });

    it('should handle only whitespace', async () => {
      await routeCommands(mockExchange, 'BTCUSDT', '   ');

      expect(mockExecuteCommand).not.toHaveBeenCalled();
    });

    it('should handle only semicolons', async () => {
      await routeCommands(mockExchange, 'BTCUSDT', ';;;');

      expect(mockExecuteCommand).not.toHaveBeenCalled();
    });

    describe('parallel execution with @', () => {
      it('should execute @ prefixed commands in parallel (fire and forget)', async () => {
        mockExecuteCommand.mockResolvedValue({ handled: true });

        // Note: @ prefixed commands are fire-and-forget, so we can't easily
        // verify the order of execution. We verify the command is stripped.
        await routeCommands(mockExchange, 'BTCUSDT', '@buy 1; sell 1');

        // Both commands should be processed
        expect(mockExecuteCommand).toHaveBeenCalledTimes(2);

        // First call should have @ stripped
        expect(mockExecuteCommand).toHaveBeenNthCalledWith(
          1,
          'buy 1', // @ is stripped
          'BTCUSDT',
          undefined,
          expect.anything(),
          expect.anything(),
        );
      });

      it('should emit error when parallel command fails', async () => {
        // First command (parallel) fails, second succeeds
        mockExecuteCommand.mockRejectedValueOnce(new Error('Command failed')).mockResolvedValueOnce({ handled: true });

        await routeCommands(mockExchange, 'BTCUSDT', '@failing; working');

        // Wait for parallel command to complete
        await vi.waitFor(() => {
          expect(mockEmit).toHaveBeenCalledWith('exec-log', {
            message: 'Error in parallel command: Command failed',
          });
        });
      });
    });

    describe('sequential execution', () => {
      it('should execute commands sequentially without @', async () => {
        const callOrder: string[] = [];
        mockExecuteCommand.mockImplementation(async (cmd) => {
          callOrder.push(cmd);
          return { handled: true };
        });

        await routeCommands(mockExchange, 'BTCUSDT', 'first; second; third');

        expect(callOrder).toEqual(['first', 'second', 'third']);
      });
    });

    describe('exec-result reply (capture-syntax writeback)', () => {
      it('does NOT emit exec-result when no reqId is provided', async () => {
        mockExecuteCommand.mockResolvedValueOnce({ handled: true });
        await routeCommands(mockExchange, 'BTCUSDT', 'buy 1');
        expect(mockEmit).not.toHaveBeenCalledWith('exec-result', expect.anything());
      });

      it('emits exec-log for handled command failures when no reqId is provided', async () => {
        mockExecuteCommand.mockResolvedValueOnce({
          handled: true,
          success: false,
          error: 'Invalid size',
        });

        await routeCommands(mockExchange, 'BTCUSDT', 'buy nope');

        expect(mockEmit).toHaveBeenCalledWith('exec-log', {
          message: 'Invalid size',
        });
      });

      it('emits exec-result with reqId + data when reqId is set', async () => {
        mockExecuteCommand.mockResolvedValueOnce({
          handled: true,
          success: true,
          data: { orderIds: ['order-abc'] },
        });

        await routeCommands(mockExchange, 'BTCUSDT', 'buy 1', 'req-1');

        expect(mockEmit).toHaveBeenCalledWith('exec-result', {
          reqId: 'req-1',
          handled: true,
          success: true,
          data: { orderIds: ['order-abc'] },
        });
      });

      it('emits exec-result with success=false + error when captured command fails', async () => {
        mockExecuteCommand.mockResolvedValueOnce({
          handled: true,
          success: false,
          error: 'wait timed out',
        });

        await routeCommands(mockExchange, 'BTCUSDT', 'wait price > 1', 'req-fail');

        expect(mockEmit).toHaveBeenCalledWith('exec-result', {
          reqId: 'req-fail',
          handled: true,
          success: false,
          error: 'wait timed out',
        });
      });

      it('emits exec-result with handled=false when command is unknown', async () => {
        mockExecuteCommand.mockResolvedValueOnce({ handled: false });

        await routeCommands(mockExchange, 'BTCUSDT', 'banana', 'req-2');

        // Unknown-command warning still fires.
        expect(mockEmit).toHaveBeenCalledWith('exec-log', {
          message: 'Unknown command: banana',
        });
        // AND the reply carries handled=false so the CLI knows the
        // capture didn't bind.
        expect(mockEmit).toHaveBeenCalledWith('exec-result', {
          reqId: 'req-2',
          handled: false,
          data: undefined,
        });
      });

      it('multi-command chain (`a; b; c`) — exec-result carries only the LAST result', async () => {
        mockExecuteCommand
          .mockResolvedValueOnce({ handled: true, data: { orderIds: ['first'] } })
          .mockResolvedValueOnce({ handled: true, data: { orderIds: ['second'] } })
          .mockResolvedValueOnce({ handled: true, data: { orderIds: ['third'] } });

        await routeCommands(mockExchange, 'BTCUSDT', 'a; b; c', 'req-3');

        // Exactly one exec-result, with the LAST result's data.
        const execResultCalls = mockEmit.mock.calls.filter(([event]) => event === 'exec-result');
        expect(execResultCalls).toHaveLength(1);
        expect(execResultCalls[0][1]).toEqual({
          reqId: 'req-3',
          handled: true,
          data: { orderIds: ['third'] },
        });
      });

      it('parallel `@cmd` slices do NOT contribute to the exec-result', async () => {
        // `@a; b` — `@a` is fire-and-forget (parallel), `b` is awaited.
        // The exec-result carries `b`'s result; `@a`'s result is
        // intentionally discarded.
        mockExecuteCommand
          .mockResolvedValueOnce({ handled: true, data: { orderIds: ['parallel'] } })
          .mockResolvedValueOnce({ handled: true, data: { orderIds: ['sequential'] } });

        await routeCommands(mockExchange, 'BTCUSDT', '@a; b', 'req-4');

        const execResultCalls = mockEmit.mock.calls.filter(([event]) => event === 'exec-result');
        expect(execResultCalls).toHaveLength(1);
        expect(execResultCalls[0][1]).toEqual({
          reqId: 'req-4',
          handled: true,
          data: { orderIds: ['sequential'] },
        });
      });
    });
  });
});
