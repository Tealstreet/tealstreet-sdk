import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  cell,
  separatorRow,
  setTableRenderMode,
  getTableRenderMode,
  renderTable,
  pad,
  type TableData,
  type TableCell,
  type TableRow,
} from './Table';

describe('cell', () => {
  it('should create a cell with just a value', () => {
    const result = cell('test');
    expect(result).toEqual({ value: 'test', style: undefined, align: undefined });
  });

  it('should create a cell with value and style', () => {
    const result = cell('test', 'green');
    expect(result).toEqual({ value: 'test', style: 'green', align: undefined });
  });

  it('should create a cell with value, style, and alignment', () => {
    const result = cell('test', 'red', 'right');
    expect(result).toEqual({ value: 'test', style: 'red', align: 'right' });
  });

  it('should handle empty string value', () => {
    const result = cell('');
    expect(result).toEqual({ value: '', style: undefined, align: undefined });
  });

  it('should handle all text styles', () => {
    const styles = ['green', 'red', 'yellow', 'cyan', 'dim', 'bold', 'none'] as const;
    for (const style of styles) {
      const result = cell('test', style);
      expect(result.style).toBe(style);
    }
  });

  it('should handle all alignment options', () => {
    const alignments = ['left', 'right', 'center'] as const;
    for (const align of alignments) {
      const result = cell('test', 'none', align);
      expect(result.align).toBe(align);
    }
  });
});

describe('separatorRow', () => {
  it('should create a separator row with specified column count', () => {
    const result = separatorRow(3);
    expect(result.isSeparator).toBe(true);
    expect(result.cells).toHaveLength(3);
  });

  it('should create cells with empty values and no style', () => {
    const result = separatorRow(2);
    expect(result.cells[0]).toEqual({ value: '', style: 'none' });
    expect(result.cells[1]).toEqual({ value: '', style: 'none' });
  });

  it('should handle zero columns', () => {
    const result = separatorRow(0);
    expect(result.isSeparator).toBe(true);
    expect(result.cells).toHaveLength(0);
  });

  it('should handle single column', () => {
    const result = separatorRow(1);
    expect(result.cells).toHaveLength(1);
  });

  it('should handle many columns', () => {
    const result = separatorRow(10);
    expect(result.cells).toHaveLength(10);
  });
});

describe('setTableRenderMode and getTableRenderMode', () => {
  // Store the original mode to restore after tests
  let originalMode: 'terminal' | 'web';

  beforeEach(() => {
    originalMode = getTableRenderMode();
  });

  afterEach(() => {
    setTableRenderMode(originalMode);
  });

  it('should set and get terminal mode', () => {
    setTableRenderMode('terminal');
    expect(getTableRenderMode()).toBe('terminal');
  });

  it('should set and get web mode', () => {
    setTableRenderMode('web');
    expect(getTableRenderMode()).toBe('web');
  });

  it('should switch between modes', () => {
    setTableRenderMode('terminal');
    expect(getTableRenderMode()).toBe('terminal');

    setTableRenderMode('web');
    expect(getTableRenderMode()).toBe('web');

    setTableRenderMode('terminal');
    expect(getTableRenderMode()).toBe('terminal');
  });
});

describe('pad', () => {
  it('should pad string to left by default', () => {
    expect(pad('test', 10)).toBe('test      ');
  });

  it('should pad string to right', () => {
    expect(pad('test', 10, 'right')).toBe('      test');
  });

  it('should pad string to center', () => {
    expect(pad('test', 10, 'center')).toBe('   test   ');
  });

  it('should handle odd-length centering', () => {
    // 'hi' is 2 chars, target is 5, so 3 spaces: 1 left, 2 right
    expect(pad('hi', 5, 'center')).toBe(' hi  ');
  });

  it('should truncate string if longer than length', () => {
    expect(pad('hello world', 5)).toBe('hello');
  });

  it('should return string unchanged if exactly the right length', () => {
    expect(pad('test', 4)).toBe('test');
  });

  it('should handle empty string', () => {
    expect(pad('', 5)).toBe('     ');
    expect(pad('', 5, 'right')).toBe('     ');
    expect(pad('', 5, 'center')).toBe('     ');
  });

  it('should handle zero length', () => {
    expect(pad('test', 0)).toBe('');
    expect(pad('', 0)).toBe('');
  });

  it('should use non-breaking spaces when useNbsp is true', () => {
    const nbsp = '\u00A0';
    expect(pad('test', 6, 'left', true)).toBe('test' + nbsp + nbsp);
    expect(pad('test', 6, 'right', true)).toBe(nbsp + nbsp + 'test');
    expect(pad('test', 6, 'center', true)).toBe(nbsp + 'test' + nbsp);
  });

  it('should use regular spaces when useNbsp is false', () => {
    expect(pad('test', 6, 'left', false)).toBe('test  ');
    expect(pad('test', 6, 'right', false)).toBe('  test');
  });
});

describe('renderTable', () => {
  let originalMode: 'terminal' | 'web';

  beforeEach(() => {
    originalMode = getTableRenderMode();
  });

  afterEach(() => {
    setTableRenderMode(originalMode);
  });

  describe('terminal mode', () => {
    beforeEach(() => {
      setTableRenderMode('terminal');
    });

    it('should render a simple table with box drawing characters', () => {
      const data: TableData = {
        columns: [
          { header: 'Name', width: 10 },
          { header: 'Value', width: 8 },
        ],
        rows: [{ cells: [cell('Alice'), cell('100')] }, { cells: [cell('Bob'), cell('200')] }],
      };

      const output = renderTable(data);

      expect(output).toContain('┌');
      expect(output).toContain('┐');
      expect(output).toContain('└');
      expect(output).toContain('┘');
      expect(output).toContain('│');
      expect(output).toContain('─');
      expect(output).toContain('Name');
      expect(output).toContain('Value');
      expect(output).toContain('Alice');
      expect(output).toContain('Bob');
      expect(output).toContain('100');
      expect(output).toContain('200');
    });

    it('should render separator rows', () => {
      const data: TableData = {
        columns: [{ header: 'Col', width: 5 }],
        rows: [{ cells: [cell('A')] }, separatorRow(1), { cells: [cell('B')] }],
      };

      const output = renderTable(data);

      // Should have multiple mid-line separators
      const midLineMatches = output.match(/├.*┤/g);
      expect(midLineMatches).not.toBeNull();
      expect(midLineMatches!.length).toBeGreaterThanOrEqual(2); // Header separator + our separator
    });

    it('should render footer', () => {
      const data: TableData = {
        columns: [{ header: 'Col', width: 5 }],
        rows: [{ cells: [cell('A')] }],
        footer: 'Total: 1 item',
      };

      const output = renderTable(data);
      expect(output).toContain('Total: 1 item');
    });

    it('should apply footer style', () => {
      const data: TableData = {
        columns: [{ header: 'Col', width: 5 }],
        rows: [{ cells: [cell('A')] }],
        footer: 'Styled footer',
        footerStyle: 'dim',
      };

      const output = renderTable(data);
      // Dim style is \x1b[2m
      expect(output).toContain('\x1b[2m');
      expect(output).toContain('Styled footer');
    });

    it('should apply cell styles with ANSI codes', () => {
      const data: TableData = {
        columns: [{ header: 'Status', width: 10 }],
        rows: [{ cells: [cell('Running', 'green')] }],
      };

      const output = renderTable(data);
      // Green is \x1b[32m
      expect(output).toContain('\x1b[32m');
      expect(output).toContain('Running');
      expect(output).toContain('\x1b[0m'); // Reset
    });

    it('should respect column alignment', () => {
      const data: TableData = {
        columns: [
          { header: 'Left', width: 10, align: 'left' },
          { header: 'Right', width: 10, align: 'right' },
          { header: 'Center', width: 10, align: 'center' },
        ],
        rows: [{ cells: [cell('L'), cell('R'), cell('C')] }],
      };

      const output = renderTable(data);
      expect(output).toContain('L');
      expect(output).toContain('R');
      expect(output).toContain('C');
    });

    it('should handle empty rows array', () => {
      const data: TableData = {
        columns: [{ header: 'Empty', width: 5 }],
        rows: [],
      };

      const output = renderTable(data);
      expect(output).toContain('Empty');
      expect(output).toContain('┌');
      expect(output).toContain('└');
    });
  });

  describe('web mode', () => {
    beforeEach(() => {
      setTableRenderMode('web');
    });

    it('should render a simple table without box drawing characters', () => {
      const data: TableData = {
        columns: [
          { header: 'Name', width: 10 },
          { header: 'Value', width: 8 },
        ],
        rows: [{ cells: [cell('Alice'), cell('100')] }],
      };

      const output = renderTable(data);

      expect(output).not.toContain('┌');
      expect(output).not.toContain('│');
      expect(output).toContain('Name');
      expect(output).toContain('Value');
      expect(output).toContain('Alice');
      expect(output).toContain('100');
      expect(output).toContain('-'); // ASCII dashes for separator
    });

    it('should use non-breaking spaces', () => {
      const data: TableData = {
        columns: [{ header: 'Col', width: 10 }],
        rows: [{ cells: [cell('A')] }],
      };

      const output = renderTable(data);
      expect(output).toContain('\u00A0'); // Non-breaking space
    });

    it('should render separator rows with dashes', () => {
      const data: TableData = {
        columns: [{ header: 'Col', width: 5 }],
        rows: [{ cells: [cell('A')] }, separatorRow(1), { cells: [cell('B')] }],
      };

      const output = renderTable(data);
      // Should have multiple dash lines (web mode auto-calculates width from content)
      // 'Col' is 3 chars so we expect '---'
      expect(output).toContain('---');
      // Count dash lines (header separator + separator row)
      const dashLineMatches = output.match(/^---$/gm);
      expect(dashLineMatches).not.toBeNull();
      expect(dashLineMatches!.length).toBeGreaterThanOrEqual(2);
    });

    it('should render footer', () => {
      const data: TableData = {
        columns: [{ header: 'Col', width: 5 }],
        rows: [{ cells: [cell('A')] }],
        footer: 'Total: 1 item',
      };

      const output = renderTable(data);
      expect(output).toContain('Total: 1 item');
    });

    it('should calculate column widths based on content', () => {
      const data: TableData = {
        columns: [{ header: 'Short', width: 5 }], // Width is a hint, will auto-adjust
        rows: [{ cells: [cell('VeryLongContent')] }],
      };

      const output = renderTable(data);
      expect(output).toContain('VeryLongContent');
    });
  });

  describe('edge cases', () => {
    beforeEach(() => {
      setTableRenderMode('terminal');
    });

    it('should handle cells with undefined style', () => {
      const data: TableData = {
        columns: [{ header: 'Col', width: 5 }],
        rows: [{ cells: [{ value: 'Test' }] }], // No style or align
      };

      const output = renderTable(data);
      expect(output).toContain('Test');
    });

    it('should handle multiple columns with different widths', () => {
      const data: TableData = {
        columns: [
          { header: 'A', width: 3 },
          { header: 'B', width: 5 },
          { header: 'C', width: 10 },
        ],
        rows: [{ cells: [cell('1'), cell('22'), cell('333')] }],
      };

      const output = renderTable(data);
      expect(output).toContain('A');
      expect(output).toContain('B');
      expect(output).toContain('C');
    });

    it('should handle special characters in values', () => {
      const data: TableData = {
        columns: [{ header: 'Special', width: 20 }],
        rows: [{ cells: [cell('$1,234.56 (USD)')] }],
      };

      const output = renderTable(data);
      expect(output).toContain('$1,234.56 (USD)');
    });

    it('should handle unicode characters', () => {
      const data: TableData = {
        columns: [{ header: 'Unicode', width: 10 }],
        rows: [{ cells: [cell('\u4e2d\u6587')] }],
      };

      const output = renderTable(data);
      expect(output).toContain('\u4e2d\u6587');
    });
  });
});
