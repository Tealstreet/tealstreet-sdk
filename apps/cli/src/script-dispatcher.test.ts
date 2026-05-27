import { describe, expect, it } from 'vitest';

import { parseScriptInvocation } from './script-dispatcher';

describe('parseScriptInvocation — SPEC §6.1 grammar', () => {
  it('parses `script:name` with no args', () => {
    expect(parseScriptInvocation('script:hello')).toEqual({ name: 'hello', rawArgs: '' });
  });

  it('parses `script:name arg1 arg2 --flag=val` with rawArgs preserved', () => {
    expect(parseScriptInvocation('script:myStrategy 1 BTC --reduce')).toEqual({
      name: 'myStrategy',
      rawArgs: '1 BTC --reduce',
    });
  });

  it('preserves multi-space + quoted args in rawArgs (script-side tokenizer owns this)', () => {
    expect(parseScriptInvocation('script:fan "foo bar" $100')).toEqual({
      name: 'fan',
      rawArgs: '"foo bar" $100',
    });
  });

  it('accepts hyphenated names', () => {
    expect(parseScriptInvocation('script:my-strategy')).toEqual({ name: 'my-strategy', rawArgs: '' });
  });

  it('rejects `script:` with no name (parse error)', () => {
    expect(parseScriptInvocation('script:')).toBeNull();
  });

  it('rejects `script: foo` with space after the colon (SPEC §6.1)', () => {
    expect(parseScriptInvocation('script: foo')).toBeNull();
    expect(parseScriptInvocation('script:  foo')).toBeNull();
    expect(parseScriptInvocation('script:\tfoo')).toBeNull();
  });

  it('treats `script:name\\tabbed` (tab whitespace) as a valid name+args split', () => {
    // The args separator is any whitespace, not just space.
    expect(parseScriptInvocation('script:hello\targ1')).toEqual({ name: 'hello', rawArgs: 'arg1' });
  });

  it('does NOT lowercase the name — registry is case-sensitive', () => {
    expect(parseScriptInvocation('script:MyStrategy')).toEqual({ name: 'MyStrategy', rawArgs: '' });
  });

  it('preserves filenames that would collide with REPL builtins (legal under prefix-only model)', () => {
    expect(parseScriptInvocation('script:buy 1 BTC')).toEqual({ name: 'buy', rawArgs: '1 BTC' });
    expect(parseScriptInvocation('script:tasks')).toEqual({ name: 'tasks', rawArgs: '' });
  });
});
