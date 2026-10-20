import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  formatPrompt,
  simplePrompt,
  getPromptPreset,
  listPromptPresets,
  PROMPT_PRESETS,
  type PromptOptions,
  type PromptConfig,
} from './PromptFormatter';
import type { ExchangeLike } from '../types';
import { PositionSide, OrderStatus } from '@tealstreet/safe-cex/src/types';

// Mock chalk to return plain strings for testing
vi.mock('chalk', () => ({
  default: {
    green: (s: string) => `[green]${s}[/green]`,
    red: (s: string) => `[red]${s}[/red]`,
    yellow: (s: string) => `[yellow]${s}[/yellow]`,
    dim: (s: string) => `[dim]${s}[/dim]`,
  },
}));

// Mock ConnectionMonitor
vi.mock('./ConnectionMonitor', () => ({
  ConnectionMonitor: vi.fn(function () {
    return {
      getPromptStatus: vi.fn().mockReturnValue(''),
    };
  }),
}));

describe('PromptFormatter', () => {
  describe('simplePrompt', () => {
    it('should generate prompt with symbol only', () => {
      const result = simplePrompt('BTCUSDT');
      expect(result).toBe('tealstreet [BTCUSDT]> ');
    });

    it('should include account name when provided', () => {
      const result = simplePrompt('BTCUSDT', 'myaccount');
      expect(result).toBe('tealstreet:myaccount [BTCUSDT]> ');
    });

    it('should use custom prefix', () => {
      const result = simplePrompt('ETHUSDT', undefined, 'ts');
      expect(result).toBe('ts [ETHUSDT]> ');
    });

    it('should use custom prefix with account name', () => {
      const result = simplePrompt('ETHUSDT', 'test', 'ts');
      expect(result).toBe('ts:test [ETHUSDT]> ');
    });

    it('should handle empty prefix', () => {
      const result = simplePrompt('BTCUSDT', undefined, '');
      expect(result).toBe(' [BTCUSDT]> ');
    });

    it('should handle empty account name', () => {
      const result = simplePrompt('BTCUSDT', '');
      expect(result).toBe('tealstreet [BTCUSDT]> ');
    });
  });

  describe('getPromptPreset', () => {
    it('should return minimal preset', () => {
      const preset = getPromptPreset('minimal');
      expect(preset).toBeDefined();
      expect(preset?.format).toBe('[{symbol}]> ');
      expect(preset?.prefix).toBe('ts');
    });

    it('should return default preset', () => {
      const preset = getPromptPreset('default');
      expect(preset).toBeDefined();
      expect(preset?.format).toContain('{symbol}');
      expect(preset?.format).toContain('{price}');
    });

    it('should return detailed preset', () => {
      const preset = getPromptPreset('detailed');
      expect(preset).toBeDefined();
      expect(preset?.format).toContain('{pnl}');
      expect(preset?.format).toContain('{entry}');
    });

    it('should return trader preset', () => {
      const preset = getPromptPreset('trader');
      expect(preset).toBeDefined();
      expect(preset?.format).toContain('{liq}');
      expect(preset?.format).toContain('{pnl%}');
    });

    it('should return compact preset', () => {
      const preset = getPromptPreset('compact');
      expect(preset).toBeDefined();
      expect(preset?.format).toContain('{account}');
      expect(preset?.format).toContain('{pnl%}');
    });

    it('should return undefined for unknown preset', () => {
      const preset = getPromptPreset('nonexistent');
      expect(preset).toBeUndefined();
    });

    it('should return undefined for empty string', () => {
      const preset = getPromptPreset('');
      expect(preset).toBeUndefined();
    });
  });

  describe('listPromptPresets', () => {
    it('should return all preset names', () => {
      const presets = listPromptPresets();
      expect(presets).toContain('minimal');
      expect(presets).toContain('default');
      expect(presets).toContain('detailed');
      expect(presets).toContain('trader');
      expect(presets).toContain('compact');
    });

    it('should return array of strings', () => {
      const presets = listPromptPresets();
      expect(Array.isArray(presets)).toBe(true);
      presets.forEach((preset) => {
        expect(typeof preset).toBe('string');
      });
    });

    it('should match PROMPT_PRESETS keys', () => {
      const presets = listPromptPresets();
      const keys = Object.keys(PROMPT_PRESETS);
      expect(presets.sort()).toEqual(keys.sort());
    });
  });

  describe('PROMPT_PRESETS', () => {
    it('should have all expected presets', () => {
      expect(PROMPT_PRESETS.minimal).toBeDefined();
      expect(PROMPT_PRESETS.default).toBeDefined();
      expect(PROMPT_PRESETS.detailed).toBeDefined();
      expect(PROMPT_PRESETS.trader).toBeDefined();
      expect(PROMPT_PRESETS.compact).toBeDefined();
    });

    it('should have hideEmpty set for most presets', () => {
      expect(PROMPT_PRESETS.minimal.hideEmpty).toBe(true);
      expect(PROMPT_PRESETS.default.hideEmpty).toBe(true);
      expect(PROMPT_PRESETS.detailed.hideEmpty).toBe(true);
      expect(PROMPT_PRESETS.trader.hideEmpty).toBe(true);
      expect(PROMPT_PRESETS.compact.hideEmpty).toBe(true);
    });
  });

  describe('formatPrompt', () => {
    describe('basic formatting', () => {
      it('should format prompt with symbol only (no exchange)', () => {
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          exchange: null,
          config: {
            format: '[{symbol}]> ',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.prompt).toContain('BTCUSDT');
        expect(result.prompt.endsWith(' ')).toBe(true);
      });

      it('should include account name in prompt', () => {
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          accountName: 'testaccount',
          exchange: null,
          config: {
            format: '[{account} {symbol}]> ',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.prompt).toContain('testaccount');
        expect(result.prompt).toContain('BTCUSDT');
      });

      it('should add prefix when configured', () => {
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          exchange: null,
          config: {
            format: '[{symbol}]> ',
            prefix: 'ts',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.prompt.startsWith('ts ')).toBe(true);
      });

      it('should show account in prefix when configured', () => {
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          accountName: 'myaccount',
          exchange: null,
          config: {
            format: '[{symbol}]> ',
            prefix: 'ts',
            showAccountInPrefix: true,
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.prompt).toContain('ts:myaccount');
      });

      it('should include version when provided', () => {
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          exchange: null,
          version: '1.2.3',
          config: {
            format: '{version} [{symbol}]> ',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.prompt).toContain('v1.2.3');
      });
    });

    describe('connectionState (selected-but-not-connected)', () => {
      const optsWith = (connectionState: PromptOptions['connectionState']): PromptOptions => ({
        symbol: 'BTCUSDT',
        accountName: 'bu test',
        exchange: null,
        connectionState,
        config: { format: '{conn}[{account} {symbol}]> ', hideEmpty: true },
      });

      // chalk is mocked to wrap text as [dim]…[/dim] / [red]…[/red] (see top
      // of file), so the tint assertions check for those markers.
      it('renders a [connecting] status word and dims the account while connecting', () => {
        const result = formatPrompt(optsWith('connecting')).prompt;
        expect(result).toContain('connecting');
        expect(result).toContain('[dim]bu test[/dim]');
      });

      it('renders [disconnected] and reddens the account on a generic failure', () => {
        const result = formatPrompt(optsWith('failed')).prompt;
        expect(result).toContain('disconnected');
        expect(result).toContain('[red]bu test[/red]');
      });

      it('renders [auth failed] and reddens the account on an auth failure', () => {
        const result = formatPrompt(optsWith('auth-failed')).prompt;
        expect(result).toContain('auth failed');
        expect(result).toContain('[red]bu test[/red]');
      });

      it('emits no status word and does not tint once connected', () => {
        const result = formatPrompt(optsWith('connected')).prompt;
        expect(result).not.toContain('connecting');
        expect(result).not.toContain('disconnected');
        expect(result).not.toContain('[dim]');
        expect(result).not.toContain('[red]');
        expect(result).toContain('bu test');
      });

      it('emits no status word when connectionState is undefined', () => {
        const result = formatPrompt(optsWith(undefined)).prompt;
        expect(result).not.toContain('connecting');
        expect(result).not.toContain('disconnected');
      });

      it('ignores connectionState when a live exchange is present (status owned by ConnectionMonitor)', () => {
        // A live exchange means the account IS connected — connectionState
        // must not override the live indicator or tint the account.
        const liveExchange = {
          store: { markets: [], tickers: [], positions: [], orders: [], balance: undefined },
        } as unknown as ExchangeLike;
        const result = formatPrompt({ ...optsWith('connecting'), exchange: liveExchange }).prompt;
        expect(result).not.toContain('connecting');
        expect(result).not.toContain('[dim]');
      });

      it('tints the account in the legacy showAccountInPrefix path too', () => {
        const result = formatPrompt({
          symbol: 'BTCUSDT',
          accountName: 'bu test',
          exchange: null,
          connectionState: 'connecting',
          config: { format: '[{symbol}]> ', prefix: 'ts', showAccountInPrefix: true, hideEmpty: true },
        }).prompt;
        expect(result).toContain('[dim]bu test[/dim]');
      });
    });

    describe('with exchange data', () => {
      const createMockExchange = (options: {
        price?: number;
        position?: {
          side: PositionSide;
          contracts: number;
          entryPrice: number;
          unrealizedPnl: number;
          liquidationPrice?: number;
          leverage?: number;
        };
        orders?: number;
      }): ExchangeLike => {
        const tickers = options.price ? [{ symbol: 'BTCUSDT', last: options.price, mark: options.price }] : [];

        const positions = options.position
          ? [
              {
                symbol: 'BTCUSDT',
                side: options.position.side,
                contracts: options.position.contracts,
                entryPrice: options.position.entryPrice,
                unrealizedPnl: options.position.unrealizedPnl,
                liquidationPrice: options.position.liquidationPrice || 0,
                leverage: options.position.leverage || 1,
              },
            ]
          : [];

        const orders = options.orders
          ? Array(options.orders)
              .fill(null)
              .map((_, i) => ({
                id: `order-${i}`,
                symbol: 'BTCUSDT',
                status: OrderStatus.Open,
              }))
          : [];

        return {
          store: {
            tickers,
            positions,
            orders,
            markets: [
              {
                symbol: 'BTCUSDT',
                precision: { price: 2, amount: 4 },
              },
            ],
          },
        } as unknown as ExchangeLike;
      };

      it('should return price in PromptInfo', () => {
        const exchange = createMockExchange({ price: 50000 });
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          exchange,
          config: {
            format: '[{symbol} {price}]> ',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.price).toBe(50000);
      });

      it('should return position info in PromptInfo', () => {
        const exchange = createMockExchange({
          price: 50000,
          position: {
            side: PositionSide.Long,
            contracts: 1.5,
            entryPrice: 48000,
            unrealizedPnl: 3000,
            liquidationPrice: 40000,
            leverage: 10,
          },
        });
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          exchange,
          config: {
            format: '[{symbol} {side}{size}]> ',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.positionSide).toBe('Long');
        expect(result.positionSize).toBe(1.5);
        expect(result.unrealizedPnl).toBe(3000);
        expect(result.entryPrice).toBe(48000);
        expect(result.liquidationPrice).toBe(40000);
      });

      it('should return short position info', () => {
        const exchange = createMockExchange({
          price: 50000,
          position: {
            side: PositionSide.Short,
            contracts: -2,
            entryPrice: 52000,
            unrealizedPnl: 4000,
          },
        });
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          exchange,
          config: {
            format: '[{symbol} {side}{size}]> ',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.positionSide).toBe('Short');
        expect(result.positionSize).toBe(2);
      });

      it('shows an OKX DOGE position in base units with the asset', () => {
        const exchange = {
          store: {
            positions: [{ symbol: 'DOGEUSDT', side: PositionSide.Long, contracts: 240, entryPrice: 0.1 }],
            markets: [
              { symbol: 'DOGEUSDT', base: 'DOGE', precision: { price: 0.00001, amount: 0.01 }, contractSize: 1000 },
            ],
          },
        } as unknown as ExchangeLike;
        const result = formatPrompt({ symbol: 'DOGEUSDT', exchange, config: { format: '{side}{size}> ' } });
        expect(result.prompt).toContain('240 DOGE');
      });

      it('should return open order count', () => {
        const exchange = createMockExchange({
          price: 50000,
          orders: 3,
        });
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          exchange,
          config: {
            format: '[{symbol} {orders}o]> ',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.openOrderCount).toBe(3);
      });
    });

    describe('hideEmpty behavior', () => {
      it('should hide empty tokens when hideEmpty is true', () => {
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          exchange: null,
          config: {
            format: '[{symbol} {price} {side}]> ',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        // Price and side should be removed when empty
        expect(result.prompt).not.toContain('{price}');
        expect(result.prompt).not.toContain('{side}');
      });

      it('should clean up empty brackets', () => {
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          exchange: null,
          config: {
            format: '[{symbol}] ({pnl}) [{orders}]> ',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        // Empty parentheses and brackets should be cleaned up
        expect(result.prompt).not.toContain('()');
        expect(result.prompt).not.toContain('[]>');
      });
    });

    describe('legacy options compatibility', () => {
      it('should work with legacy showPrice option', () => {
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          exchange: null,
          showPrice: true,
        };
        const result = formatPrompt(options);
        // Should not throw and should return valid prompt
        expect(result.prompt).toContain('BTCUSDT');
        expect(result.prompt.endsWith(' ')).toBe(true);
      });

      it('should work with legacy prefix option', () => {
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          exchange: null,
          prefix: 'custom',
        };
        const result = formatPrompt(options);
        expect(result.prompt).toContain('custom');
      });
    });

    describe('edge cases', () => {
      it('should handle empty symbol', () => {
        const options: PromptOptions = {
          symbol: '',
          exchange: null,
          config: {
            format: '[{symbol}]> ',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.prompt.endsWith(' ')).toBe(true);
      });

      it('should handle undefined accountName', () => {
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          accountName: undefined,
          exchange: null,
          config: {
            format: '[{account} {symbol}]> ',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.prompt).toContain('BTCUSDT');
      });

      it('should ensure prompt ends with space', () => {
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          exchange: null,
          config: {
            format: '[{symbol}]',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.prompt.endsWith(' ')).toBe(true);
      });

      it('should handle complex format string', () => {
        const options: PromptOptions = {
          symbol: 'BTCUSDT',
          accountName: 'test',
          exchange: null,
          version: '1.0.0',
          config: {
            format: '{version} {conn}[{account} {symbol} {price} {side}{size}]> ',
            prefix: '',
            hideEmpty: true,
          },
        };
        const result = formatPrompt(options);
        expect(result.prompt).toContain('v1.0.0');
        expect(result.prompt).toContain('test');
        expect(result.prompt).toContain('BTCUSDT');
      });
    });
  });
});
