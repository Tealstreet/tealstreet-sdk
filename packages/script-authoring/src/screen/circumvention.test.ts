import { describe, expect, it } from 'vitest';

import { hasBlockingFinding, requiredTier, scanCircumvention } from './circumvention';

const rules = (code: string): string[] => scanCircumvention(code).map((f) => f.rule);
const severityOf = (code: string, rule: string): string | undefined =>
  scanCircumvention(code).find((f) => f.rule === rule)?.severity;

/**
 * These are not hypothetical. Each is a pattern measured in the production
 * corpus on 2026-08-07 that a naive word-match screen would have blocked,
 * which under fail-closed screening means the author cannot save at all.
 */
describe('measured false positives — must never fire', () => {
  // 28 of 70 production modules contain `createElement`. Every one is React's.
  it('does not fire on React.createElement', () => {
    expect(rules(`const el = React.createElement('div', null, 'hi');`)).toEqual([]);
  });

  // The single production `parent` is a local in a tree walk.
  it('does not fire on a local variable named parent', () => {
    expect(rules(`function walk(parent) { return parent.children; }`)).toEqual([]);
  });

  it('does not fire on a declared const named parent', () => {
    expect(rules(`const parent = node.up(); parent.focus();`)).toEqual([]);
  });

  it('does not fire on an object key named top', () => {
    expect(rules(`const style = { top: 0, left: 0 };`)).toEqual([]);
  });

  it('does not fire on a member named top', () => {
    expect(rules(`const y = boundingRect.top + 4;`)).toEqual([]);
  });

  it('does not fire on an imported binding that shadows a global', () => {
    expect(rules(`import { fetch } from './helpers'; fetch();`)).toEqual([]);
  });

  it('does not fire on a destructured binding that shadows a global', () => {
    expect(rules(`const { localStorage } = makeFakes(); localStorage.get('k');`)).toEqual([]);
  });

  it('does not fire on a parameter that shadows a global', () => {
    expect(rules(`const run = (fetch) => fetch('/x');`)).toEqual([]);
  });
});

describe('block — refused in EVERY tier, including standard', () => {
  it('blocks document.cookie', () => {
    expect(severityOf(`const c = document.cookie;`, 'document-cookie')).toBe('block');
  });

  // window.document.cookie and document.cookie must hit ONE rule, not two.
  it('normalizes away a window/globalThis/self root', () => {
    expect(rules(`const c = window.document.cookie;`)).toEqual(['document-cookie']);
    expect(rules(`const c = globalThis.document.cookie;`)).toEqual(['document-cookie']);
  });

  it('blocks a string element access, which is the same thing spelled differently', () => {
    expect(rules(`const c = document['cookie'];`)).toContain('document-cookie');
  });

  it('blocks reaching for the top window or opener', () => {
    expect(severityOf(`const t = top.location;`, 'frame-escape')).toBe('block');
    expect(rules(`const o = opener;`)).toContain('frame-escape');
    expect(rules(`const f = frameElement;`)).toContain('frame-escape');
  });

  it('blocks prototype pollution', () => {
    expect(rules(`obj.__proto__ = evil;`)).toContain('prototype-pollution');
    expect(rules(`Object.setPrototypeOf(obj, evil);`)).toContain('prototype-pollution');
  });

  it('blocks navigating the page away', () => {
    expect(rules(`location.href = 'https://evil.example';`)).toContain('navigation');
    expect(rules(`window.location = 'https://evil.example';`)).toContain('navigation');
    expect(rules(`location.replace('https://evil.example');`)).toContain('navigation');
  });

  it('blocks writing raw markup into the document', () => {
    expect(severityOf(`document.write('<b>x</b>');`, 'document-write')).toBe('block');
  });
});

/**
 * The tier that exists so superusers are not neutered. Reaching the network or
 * loading foreign code is legitimate — it just means the module runs only where
 * it was written and is never stored on our servers, which is precisely what
 * puts it out of reach of a stolen session.
 */
describe('standard — allowed, mounts in the frame with network', () => {
  const standard = (code: string, rule: string) => {
    expect(severityOf(code, rule)).toBe('standard');
    // Emphatically NOT a block: these authors keep working.
    expect(hasBlockingFinding(scanCircumvention(code))).toBe(false);
    expect(requiredTier(scanCircumvention(code))).toBe('standard');
  };

  it('routes fetch to standard', () => standard(`fetch('/x');`, 'network-fetch'));
  it('routes WebSocket to standard', () => standard(`new WebSocket('wss://x');`, 'network-socket'));
  it('routes EventSource to standard', () => standard(`new EventSource('/s');`, 'network-sse'));
  it('routes XHR to standard', () => standard(`new XMLHttpRequest();`, 'xhr'));
  it('routes sendBeacon to standard', () => standard(`navigator.sendBeacon('/x', d);`, 'send-beacon'));
  it('routes importScripts to standard', () => standard(`importScripts('/x.js');`, 'import-scripts'));
  it('routes dynamic import to standard', () => standard(`const m = await import('/x.js');`, 'dynamic-import'));

  /**
   * Measured: 3 of 70 production modules embed a widget or pull a charting
   * library this way. Loading foreign code means the code that runs is not the
   * code that was screened — an accepted risk of the standard tier.
   */
  it('routes createElement of an executable element to standard', () =>
    standard(`const s = document.createElement('script');`, 'create-executable-element'));

  it('still ignores React.createElement of the same tag', () => {
    expect(rules(`const f = React.createElement('iframe', { src });`)).toEqual([]);
  });

  it('treats an ordinary element as neither executable nor standard', () => {
    expect(severityOf(`const d = document.createElement('div');`, 'create-element')).toBe('flag');
    expect(requiredTier(scanCircumvention(`const d = document.createElement('div');`))).toBe('strict');
  });
});

describe('flag — evidence only, changes no tier', () => {
  const flagged = (code: string, rule: string) => {
    expect(severityOf(code, rule)).toBe('flag');
    expect(requiredTier(scanCircumvention(code))).toBe('strict');
  };

  /**
   * The standard tier relaxes the network, NOT the origin, so storage stays
   * unreachable in both tiers. These modules need api.createStorageAtom
   * whatever tier they land in — which is why this is a flag and not a route
   * into standard.
   */
  it('flags localStorage without routing it to standard', () =>
    flagged(`localStorage.setItem('k','v');`, 'local-storage'));
  it('flags sessionStorage', () => flagged(`sessionStorage.getItem('k');`, 'session-storage'));
  it('flags indexedDB', () => flagged(`indexedDB.open('db');`, 'indexed-db'));

  // 'unsafe-eval' is granted deliberately — transpiled module code needs it.
  it('flags eval, which works in-frame', () => flagged(`eval('1+1');`, 'eval'));
  it('flags the Function constructor', () => flagged(`const f = new Function('return 1');`, 'function-constructor'));
  it('flags raw HTML assignment', () => flagged(`el.innerHTML = s;`, 'html-injection'));
  it('flags the JSX escape hatch', () => {
    expect(rules(`const x = <div dangerouslySetInnerHTML={{ __html: s }} />;`)).toContain('html-injection');
  });
  it('flags base64, which is how a string gets hidden', () => flagged(`const s = atob('ZXZpbA==');`, 'base64'));
});

describe('requiredTier', () => {
  it('is strict for an ordinary panel', () => {
    expect(requiredTier(scanCircumvention(`const Component = () => <div>hi</div>;`))).toBe('strict');
  });

  it('block wins over standard', () => {
    const found = scanCircumvention(`fetch('/x');\nconst c = document.cookie;`);
    expect(requiredTier(found)).toBe('blocked');
  });

  it('standard wins over flag', () => {
    expect(requiredTier(scanCircumvention(`atob('x');\nfetch('/y');`))).toBe('standard');
  });
});

describe('scan behaviour', () => {
  /**
   * A computed key we cannot read is exactly the obfuscation case. We do not
   * guess — the raw source still reaches the model, which is the layer meant
   * to catch it.
   */
  it('gives up on a computed member rather than guessing', () => {
    expect(rules(`const c = document[k];`)).toEqual([]);
  });

  /**
   * Same rule as validate(): findings stacked on a parse failure bury the one
   * thing that actually needs fixing, and a file that will not parse cannot
   * run either.
   */
  it('reports nothing when the source does not parse', () => {
    expect(rules(`const broken = (((;`)).toEqual([]);
  });

  it('reports a chain once, at the outermost node', () => {
    expect(rules(`const c = document.cookie;`)).toEqual(['document-cookie']);
  });

  it('carries a line number and a bounded snippet', () => {
    const [finding] = scanCircumvention(`const a = 1;\nconst c = document.cookie;`);
    expect(finding.line).toBe(2);
    expect(finding.snippet).toBe('document.cookie');
  });

  it('returns findings in source order', () => {
    const found = scanCircumvention(`fetch('/a');\nconst c = document.cookie;`);
    expect(found.map((f) => f.line)).toEqual([1, 2]);
  });

  /**
   * The rule tables are object literals, so a bare `TABLE[path]` inherits from
   * `Object.prototype` and every one of these names resolves to a truthy
   * function. Spreading a function into a finding produced one with no `rule`,
   * `severity` or `detail` — measured on 10 of the 70 production modules.
   *
   * Tiering survived it, since `requiredTier` tests for a specific severity.
   * The findings were still fed to the model, and the screen is fail-closed,
   * so noise that reads as tampering can only push a verdict toward refusing a
   * module its author is simply trying to save.
   */
  it('does not match names inherited from Object.prototype', () => {
    for (const name of ['toString', 'constructor', 'valueOf', 'hasOwnProperty', 'isPrototypeOf']) {
      expect(scanCircumvention(`const x = ${name};`)).toEqual([]);
    }
  });

  it('reports nothing for an ordinary module', () => {
    expect(
      rules(`
        const Component = () => {
          const positions = api.data.usePositions();
          const total = positions.reduce((sum, p) => sum + p.notional, 0);
          return <div style={{ top: 0 }}>{api.format.usd(total)}</div>;
        };
      `),
    ).toEqual([]);
  });
});
