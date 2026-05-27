/**
 * Abstract table system for CLI output
 * Allows commands to define tables in a platform-agnostic way,
 * then render them appropriately for terminal or web.
 */

export type TextStyle = 'green' | 'red' | 'yellow' | 'cyan' | 'dim' | 'bold' | 'none';

export interface TableCell {
  value: string;
  style?: TextStyle;
  align?: 'left' | 'right' | 'center';
}

export interface TableColumn {
  header: string;
  width: number;
  align?: 'left' | 'right' | 'center';
}

export interface TableRow {
  cells: TableCell[];
  isSeparator?: boolean;
}

export interface TableData {
  columns: TableColumn[];
  rows: TableRow[];
  footer?: string;
  footerStyle?: TextStyle;
}

/**
 * Create a table cell with optional styling
 */
export function cell(value: string, style?: TextStyle, align?: 'left' | 'right' | 'center'): TableCell {
  return { value, style, align };
}

/**
 * Create a separator row
 */
export function separatorRow(columnCount: number): TableRow {
  return {
    cells: Array(columnCount).fill({ value: '', style: 'none' }),
    isSeparator: true,
  };
}

/**
 * Table render mode
 */
export type TableRenderMode = 'terminal' | 'web';

// Global render mode - can be set by platform adapters
let globalRenderMode: TableRenderMode = 'terminal';

export function setTableRenderMode(mode: TableRenderMode): void {
  globalRenderMode = mode;
}

export function getTableRenderMode(): TableRenderMode {
  return globalRenderMode;
}

/**
 * Render a table to string based on current render mode
 */
export function renderTable(data: TableData): string {
  if (globalRenderMode === 'web') {
    return renderTableWeb(data);
  }
  return renderTableTerminal(data);
}

/**
 * Pad a string to a fixed length
 * @param useNbsp - Use non-breaking spaces (for web rendering)
 */
export function pad(str: string, len: number, align: 'left' | 'right' | 'center' = 'left', useNbsp = false): string {
  if (str.length >= len) return str.substring(0, len);
  const spaces = len - str.length;
  const space = useNbsp ? '\u00A0' : ' ';
  if (align === 'right') {
    return space.repeat(spaces) + str;
  } else if (align === 'center') {
    const left = Math.floor(spaces / 2);
    const right = spaces - left;
    return space.repeat(left) + str + space.repeat(right);
  }
  return str + space.repeat(spaces);
}

// ANSI color codes
const COLORS: Record<TextStyle, string> = {
  green: '\x1b[32m',
  red: '\x1b[31m',
  yellow: '\x1b[33m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
  bold: '\x1b[1m',
  none: '',
};
const RESET = '\x1b[0m';

/**
 * Apply style to text (terminal mode)
 */
function applyStyle(text: string, style?: TextStyle): string {
  if (!style || style === 'none') return text;
  return `${COLORS[style]}${text}${RESET}`;
}

/**
 * Render table for terminal with box drawing characters
 */
function renderTableTerminal(data: TableData): string {
  const { columns, rows, footer, footerStyle } = data;

  // Build horizontal lines
  const topLine = '┌' + columns.map((c) => '─'.repeat(c.width + 2)).join('┬') + '┐';
  const midLine = '├' + columns.map((c) => '─'.repeat(c.width + 2)).join('┼') + '┤';
  const botLine = '└' + columns.map((c) => '─'.repeat(c.width + 2)).join('┴') + '┘';

  let output = topLine + '\n';

  // Header row
  output += '│ ' + columns.map((c) => pad(c.header, c.width, c.align)).join(' │ ') + ' │\n';
  output += midLine + '\n';

  // Data rows
  for (const row of rows) {
    if (row.isSeparator) {
      output += midLine + '\n';
      continue;
    }

    const cellStrs = row.cells.map((cell, i) => {
      const col = columns[i];
      const align = cell.align ?? col?.align ?? 'left';
      const padded = pad(cell.value, col?.width ?? 10, align);
      return applyStyle(padded, cell.style);
    });

    output += '│ ' + cellStrs.join(' │ ') + ' │\n';
  }

  output += botLine + '\n';

  // Footer
  if (footer) {
    output += applyStyle(footer, footerStyle) + '\n';
  }

  return output;
}

/**
 * Render table for web - simple aligned columns without box drawing
 * Uses ASCII characters only for better monospace font compatibility
 * Auto-calculates column widths based on actual content
 * Uses non-breaking spaces to prevent HTML whitespace collapsing
 */
function renderTableWeb(data: TableData): string {
  const { columns, rows, footer } = data;
  const nbsp = '\u00A0'; // Non-breaking space
  const colSep = nbsp + nbsp; // Column separator (2 non-breaking spaces)

  // Calculate actual column widths based on content
  const colWidths = columns.map((col, i) => {
    let maxWidth = col.header.length;
    for (const row of rows) {
      if (row.isSeparator) continue;
      const cellValue = row.cells[i]?.value || '';
      if (cellValue.length > maxWidth) {
        maxWidth = cellValue.length;
      }
    }
    return maxWidth;
  });

  let output = '';

  // Header row with underline using ASCII dash
  const headerLine = columns.map((c, i) => pad(c.header, colWidths[i], c.align, true)).join(colSep);
  output += headerLine + '\n';
  output += colWidths.map((w) => '-'.repeat(w)).join(colSep) + '\n';

  // Data rows
  for (const row of rows) {
    if (row.isSeparator) {
      output += colWidths.map((w) => '-'.repeat(w)).join(colSep) + '\n';
      continue;
    }

    const cellStrs = row.cells.map((cell, i) => {
      const col = columns[i];
      const align = cell.align ?? col?.align ?? 'left';
      return pad(cell.value, colWidths[i], align, true);
    });

    output += cellStrs.join(colSep) + '\n';
  }

  // Footer
  if (footer) {
    output += '\n' + footer + '\n';
  }

  return output;
}
