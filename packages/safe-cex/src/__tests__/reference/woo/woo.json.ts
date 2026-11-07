/** Keep native integer IDs exact without changing quoted text or financial decimals. */
export const parseWooJson = <T = unknown>(input: unknown): T | null => {
  if (typeof input !== 'string') return input as T;
  const parts: string[] = [];
  let last = 0;
  let index = 0;
  while (index < input.length) {
    if (input[index] === '"') {
      index += 1;
      while (index < input.length) {
        if (input[index] === '\\') index += 2;
        else if (input[index++] === '"') break;
      }
      continue;
    }
    if (input[index] !== '-' && (input[index] < '0' || input[index] > '9')) {
      index += 1;
      continue;
    }
    const start = index;
    if (input[index] === '-') index += 1;
    const digitsStart = index;
    while (index < input.length && input[index] >= '0' && input[index] <= '9') index += 1;
    let integer = index > digitsStart;
    if (input[index] === '.') {
      integer = false;
      index += 1;
      while (index < input.length && input[index] >= '0' && input[index] <= '9') index += 1;
    }
    if (input[index] === 'e' || input[index] === 'E') {
      integer = false;
      index += 1;
      if (input[index] === '+' || input[index] === '-') index += 1;
      while (index < input.length && input[index] >= '0' && input[index] <= '9') index += 1;
    }
    const digits = input.slice(digitsStart, index);
    if (
      integer &&
      digits[0] !== '0' &&
      (digits.length > 16 || (digits.length === 16 && digits > '9007199254740991'))
    ) {
      parts.push(input.slice(last, start), '"', input.slice(start, index), '"');
      last = index;
    }
  }
  try {
    return JSON.parse(parts.length ? parts.join('') + input.slice(last) : input) as T;
  } catch {
    return null;
  }
};
