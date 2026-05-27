export const jsonParse = <T extends Record<string, any>>(str: string): T | null => {
  try {
    return JSON.parse(str);
  } catch {
    return null;
  }
};

// Pre-compiled regex for better performance
const BIG_INT_TEST_REGEX = /[0-9]{16,}/;
// Match big integers after colon (object values), comma, or bracket (array elements)
const BIG_INT_REPLACE_REGEX = /([:,[\]]\s*)([0-9]{16,})(?![0-9"])/g;

export const bigIntAsStringJsonParse = <T extends Record<string, any>>(str: string): T | null => {
  if (typeof str !== 'string') {
    return str as T;
  }
  let safeStr = str;
  try {
    // Quick check: only run expensive replace if string might contain big integers
    if (BIG_INT_TEST_REGEX.test(str)) {
      safeStr = str.replace(BIG_INT_REPLACE_REGEX, '$1"$2"');
    }
  } catch (e: any) {
    console.log(str);
    console.error('Error parsing big int as string', e);
  }
  return jsonParse<T>(safeStr);
};
