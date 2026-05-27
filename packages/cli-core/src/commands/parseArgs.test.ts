import { describe, it, expect } from 'vitest';
import { parseArgs, getPositional, getOption, hasFlag } from './parseArgs';
import type { ParsedArgs } from './Command';

describe('parseArgs', () => {
  describe('positional arguments', () => {
    it('should parse single positional argument', () => {
      const result = parseArgs('command');
      expect(result._).toEqual(['command']);
    });

    it('should parse multiple positional arguments', () => {
      const result = parseArgs('command arg1 arg2 arg3');
      expect(result._).toEqual(['command', 'arg1', 'arg2', 'arg3']);
    });

    it('should handle empty input', () => {
      const result = parseArgs('');
      expect(result._).toEqual([]);
    });

    it('should handle whitespace-only input', () => {
      const result = parseArgs('   ');
      expect(result._).toEqual([]);
    });

    it('should handle multiple spaces between arguments', () => {
      const result = parseArgs('command   arg1    arg2');
      expect(result._).toEqual(['command', 'arg1', 'arg2']);
    });

    it('should handle tabs as delimiters', () => {
      const result = parseArgs('command\targ1\targ2');
      expect(result._).toEqual(['command', 'arg1', 'arg2']);
    });
  });

  describe('quoted values', () => {
    it('should parse double-quoted values with spaces', () => {
      const result = parseArgs('command "hello world"');
      expect(result._).toEqual(['command', 'hello world']);
    });

    it('should parse single-quoted values with spaces', () => {
      const result = parseArgs("command 'hello world'");
      expect(result._).toEqual(['command', 'hello world']);
    });

    it('should handle empty quoted strings', () => {
      const result = parseArgs('command ""');
      expect(result._).toEqual(['command']);
    });

    it('should handle quotes within different quotes', () => {
      const result = parseArgs('command "it\'s working"');
      expect(result._).toEqual(['command', "it's working"]);
    });
  });

  describe('short flags', () => {
    it('should parse single short flag', () => {
      const result = parseArgs('command -f');
      expect(result.f).toBe(true);
      expect(result._).toEqual(['command']);
    });

    it('should parse multiple short flags separately', () => {
      const result = parseArgs('command -f -v');
      expect(result.f).toBe(true);
      expect(result.v).toBe(true);
    });

    it('should parse combined short flags', () => {
      const result = parseArgs('command -fvx');
      expect(result.f).toBe(true);
      expect(result.v).toBe(true);
      expect(result.x).toBe(true);
    });
  });

  describe('short options with values', () => {
    it('should parse short option with value', () => {
      const result = parseArgs('command -n value');
      expect(result.n).toBe('value');
    });

    it('should parse short option with quoted value', () => {
      const result = parseArgs('command -k "value with spaces"');
      expect(result.k).toBe('value with spaces');
    });

    it('should parse combined flags where last takes value', () => {
      const result = parseArgs('command -fvn value');
      expect(result.f).toBe(true);
      expect(result.v).toBe(true);
      expect(result.n).toBe('value');
    });
  });

  describe('long flags', () => {
    it('should parse long flag', () => {
      const result = parseArgs('command --force');
      expect(result.force).toBe(true);
    });

    it('should parse multiple long flags', () => {
      const result = parseArgs('command --force --verbose');
      expect(result.force).toBe(true);
      expect(result.verbose).toBe(true);
    });
  });

  describe('long options with values', () => {
    it('should parse long option with value (space separated)', () => {
      const result = parseArgs('command --name value');
      expect(result.name).toBe('value');
    });

    it('should parse long option with value (equals sign)', () => {
      const result = parseArgs('command --name=value');
      expect(result.name).toBe('value');
    });

    it('should parse long option with quoted value', () => {
      const result = parseArgs('command --key "value with spaces"');
      expect(result.key).toBe('value with spaces');
    });

    it('should parse long option with equals and quoted value', () => {
      const result = parseArgs('command --key="complex value"');
      // Tokenizer strips quotes and incorporates content into token
      expect(result.key).toBe('complex value');
    });

    it('should handle empty value with equals sign', () => {
      const result = parseArgs('command --name=');
      expect(result.name).toBe('');
    });
  });

  describe('mixed arguments', () => {
    it('should parse mix of positional, flags, and options', () => {
      const result = parseArgs('command arg1 -f --verbose -n value --key other arg2');
      expect(result._).toEqual(['command', 'arg1', 'arg2']);
      expect(result.f).toBe(true);
      expect(result.verbose).toBe(true);
      expect(result.n).toBe('value');
      expect(result.key).toBe('other');
    });

    it('should handle complex real-world command', () => {
      const result = parseArgs('order buy BTCUSDT --size 0.01 -l 50000 --reduce-only');
      expect(result._).toEqual(['order', 'buy', 'BTCUSDT']);
      expect(result.size).toBe('0.01');
      expect(result.l).toBe('50000');
      expect(result['reduce-only']).toBe(true);
    });
  });

  describe('edge cases', () => {
    it('should handle standalone dash as positional', () => {
      const result = parseArgs('command -');
      expect(result._).toEqual(['command', '-']);
    });

    it('should handle flag followed by another flag (not value)', () => {
      const result = parseArgs('command -n -f');
      expect(result.n).toBe(true);
      expect(result.f).toBe(true);
    });

    it('should handle long option followed by another option (not value)', () => {
      const result = parseArgs('command --name --force');
      expect(result.name).toBe(true);
      expect(result.force).toBe(true);
    });

    it('should handle numbers as values', () => {
      const result = parseArgs('command -n 123 --count 456');
      expect(result.n).toBe('123');
      expect(result.count).toBe('456');
    });
  });
});

describe('getPositional', () => {
  it('should return positional argument at index', () => {
    const args: ParsedArgs = { _: ['command', 'arg1', 'arg2'] };
    expect(getPositional(args, 0)).toBe('command');
    expect(getPositional(args, 1)).toBe('arg1');
    expect(getPositional(args, 2)).toBe('arg2');
  });

  it('should return undefined for out-of-bounds index', () => {
    const args: ParsedArgs = { _: ['command'] };
    expect(getPositional(args, 1)).toBeUndefined();
    expect(getPositional(args, 10)).toBeUndefined();
  });

  it('should return undefined for negative index', () => {
    const args: ParsedArgs = { _: ['command', 'arg1'] };
    expect(getPositional(args, -1)).toBeUndefined();
  });

  it('should handle empty positional array', () => {
    const args: ParsedArgs = { _: [] };
    expect(getPositional(args, 0)).toBeUndefined();
  });
});

describe('getOption', () => {
  it('should return option value by single name', () => {
    const args: ParsedArgs = { _: [], name: 'value' };
    expect(getOption(args, 'name')).toBe('value');
  });

  it('should return first matching option from multiple names', () => {
    const args: ParsedArgs = { _: [], n: 'short', name: 'long' };
    expect(getOption(args, 'n', 'name')).toBe('short');
    expect(getOption(args, 'name', 'n')).toBe('long');
  });

  it('should return undefined when option not found', () => {
    const args: ParsedArgs = { _: [] };
    expect(getOption(args, 'missing')).toBeUndefined();
  });

  it('should return undefined for boolean flags (not string values)', () => {
    const args: ParsedArgs = { _: [], force: true };
    expect(getOption(args, 'force')).toBeUndefined();
  });

  it('should handle multiple names where only one exists', () => {
    const args: ParsedArgs = { _: [], name: 'value' };
    expect(getOption(args, 'n', 'name', 'alias')).toBe('value');
  });

  it('should return undefined when all names are missing', () => {
    const args: ParsedArgs = { _: [] };
    expect(getOption(args, 'a', 'b', 'c')).toBeUndefined();
  });
});

describe('hasFlag', () => {
  it('should return true when flag is set', () => {
    const args: ParsedArgs = { _: [], force: true };
    expect(hasFlag(args, 'force')).toBe(true);
  });

  it('should return false when flag is not set', () => {
    const args: ParsedArgs = { _: [] };
    expect(hasFlag(args, 'force')).toBe(false);
  });

  it('should return true when any of multiple names is set', () => {
    const args: ParsedArgs = { _: [], f: true };
    expect(hasFlag(args, 'f', 'force')).toBe(true);
  });

  it('should return false when none of multiple names is set', () => {
    const args: ParsedArgs = { _: [] };
    expect(hasFlag(args, 'f', 'force')).toBe(false);
  });

  it('should return false for string values (not true boolean)', () => {
    const args: ParsedArgs = { _: [], name: 'value' };
    expect(hasFlag(args, 'name')).toBe(false);
  });

  it('should handle mixed flags and options', () => {
    const args: ParsedArgs = { _: [], f: true, verbose: true, name: 'value' };
    expect(hasFlag(args, 'f')).toBe(true);
    expect(hasFlag(args, 'verbose')).toBe(true);
    expect(hasFlag(args, 'name')).toBe(false);
  });
});
