import { describe, expect, it } from 'vitest';

import { parseScriptArgs } from './argParser';

describe('parseScriptArgs', () => {
  describe('positional tokenization', () => {
    it('splits on whitespace', () => {
      const args = parseScriptArgs('buy 1 BTCUSDT');
      expect(args.positional).toEqual(['buy', '1', 'BTCUSDT']);
      expect(args.flags).toEqual({});
    });

    it('preserves quoted strings as one token', () => {
      const args = parseScriptArgs('echo "hello world"');
      expect(args.positional).toEqual(['echo', 'hello world']);
    });

    it('honors single quotes', () => {
      const args = parseScriptArgs("echo 'hello world'");
      expect(args.positional).toEqual(['echo', 'hello world']);
    });

    it('honors backslash-escapes inside quotes', () => {
      const args = parseScriptArgs('echo "hello \\"quoted\\""');
      expect(args.positional).toEqual(['echo', 'hello "quoted"']);
    });

    it('collapses repeated whitespace', () => {
      const args = parseScriptArgs('a    b\t\tc');
      expect(args.positional).toEqual(['a', 'b', 'c']);
    });

    it('handles empty input', () => {
      const args = parseScriptArgs('');
      expect(args.positional).toEqual([]);
      expect(args.flags).toEqual({});
      expect(args.symbols).toEqual([]);
      expect(args.raw).toBe('');
    });

    it('preserves the raw input verbatim', () => {
      const raw = 'buy 1 BTC,ENA  --reduce --tif=GTC';
      const args = parseScriptArgs(raw);
      expect(args.raw).toBe(raw);
    });
  });

  describe('numeric-suffix tokens (kept raw)', () => {
    it.each(['$100', '100$', '50%', '50%bal', '1%', '0.5%', '-1.5', '+1%'])(
      'keeps %s as a literal positional',
      (token) => {
        const args = parseScriptArgs(`buy ${token}`);
        expect(args.positional).toEqual(['buy', token]);
        expect(args.flags).toEqual({});
      },
    );
  });

  describe('negative-number vs flag disambiguation', () => {
    it('treats `-100` as a literal positional', () => {
      const args = parseScriptArgs('buy -100');
      expect(args.positional).toEqual(['buy', '-100']);
      expect(args.flags).toEqual({});
    });

    it('treats `-1.5` as a literal positional', () => {
      const args = parseScriptArgs('move -1.5');
      expect(args.positional).toEqual(['move', '-1.5']);
    });

    it('treats `--reduce` as a boolean flag', () => {
      const args = parseScriptArgs('buy 1 --reduce');
      expect(args.positional).toEqual(['buy', '1']);
      expect(args.flags).toEqual({ reduce: true });
    });

    it('treats `-x` (single short letter) as a boolean flag', () => {
      const args = parseScriptArgs('cmd -x');
      expect(args.positional).toEqual(['cmd']);
      expect(args.flags).toEqual({ x: true });
    });

    it('treats bare `-` as a literal positional', () => {
      const args = parseScriptArgs('cat -');
      expect(args.positional).toEqual(['cat', '-']);
      expect(args.flags).toEqual({});
    });
  });

  describe('flag forms', () => {
    it('parses `--flag=value`', () => {
      const args = parseScriptArgs('buy 1 --tif=GTC');
      expect(args.positional).toEqual(['buy', '1']);
      expect(args.flags).toEqual({ tif: 'GTC' });
    });

    it('parses `--flag value` (space-separated)', () => {
      const args = parseScriptArgs('buy 1 --tif GTC');
      expect(args.positional).toEqual(['buy', '1']);
      expect(args.flags).toEqual({ tif: 'GTC' });
    });

    it('treats `--bool` as true when followed by another flag', () => {
      const args = parseScriptArgs('buy 1 --reduce --tif=GTC');
      expect(args.positional).toEqual(['buy', '1']);
      expect(args.flags).toEqual({ reduce: true, tif: 'GTC' });
    });

    it('treats `--bool` as true when at end of line', () => {
      const args = parseScriptArgs('buy 1 --reduce');
      expect(args.flags).toEqual({ reduce: true });
    });

    it('preserves quoted flag values', () => {
      const args = parseScriptArgs('echo --message "hello world"');
      expect(args.flags).toEqual({ message: 'hello world' });
    });

    it('parses `--flag=` with empty value', () => {
      const args = parseScriptArgs('cmd --name=');
      expect(args.flags).toEqual({ name: '' });
    });

    it('preserves negative-number values after flags', () => {
      const args = parseScriptArgs('cmd --offset -5');
      // -5 is a literal (matches `-<digit>`), so it's consumed as the value.
      expect(args.flags).toEqual({ offset: '-5' });
      expect(args.positional).toEqual(['cmd']);
    });

    it('later flag value wins on duplicate key', () => {
      const args = parseScriptArgs('cmd --tif=IOC --tif=GTC');
      expect(args.flags).toEqual({ tif: 'GTC' });
    });
  });

  describe('args.symbols helper', () => {
    it('splits positional[0] on comma', () => {
      const args = parseScriptArgs('BTC,ETH,SOL');
      expect(args.symbols).toEqual(['BTC', 'ETH', 'SOL']);
    });

    it('trims whitespace around comma-separated entries', () => {
      const args = parseScriptArgs('"BTC, ETH , SOL"');
      expect(args.symbols).toEqual(['BTC', 'ETH', 'SOL']);
    });

    it('drops empty entries from trailing/double commas', () => {
      const args = parseScriptArgs('"BTC,,ETH,"');
      expect(args.symbols).toEqual(['BTC', 'ETH']);
    });

    it('is empty when there are no positionals', () => {
      const args = parseScriptArgs('--only-flags');
      expect(args.symbols).toEqual([]);
    });

    it('only inspects positional[0]', () => {
      const args = parseScriptArgs('BTC ETH,SOL');
      // Only positional[0] (`BTC`) feeds symbols — `ETH,SOL` is a separate arg.
      expect(args.symbols).toEqual(['BTC']);
    });
  });

  describe('frozen output (POJO-only / SPEC §3)', () => {
    it('returns frozen positional / flags / symbols + the root object', () => {
      const args = parseScriptArgs('buy 1 --reduce');
      expect(Object.isFrozen(args)).toBe(true);
      expect(Object.isFrozen(args.positional)).toBe(true);
      expect(Object.isFrozen(args.flags)).toBe(true);
      expect(Object.isFrozen(args.symbols)).toBe(true);
    });

    it('survives structuredClone (POJO contract)', () => {
      const args = parseScriptArgs('buy 1 BTC --reduce --tif=GTC');
      const cloned = structuredClone(args);
      expect(cloned).toEqual(args);
    });
  });
});
