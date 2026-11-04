/** Diagnostic context only: the original signed URL must remain unchanged on the wire. */
export const diagnosticUrl = (url: string): string => {
  if (typeof url !== 'string') return '[invalid URL]';
  try {
    const parsed = new URL(url);
    return `${parsed.origin}${parsed.pathname}`;
  } catch {
    return url.startsWith('/') && !url.startsWith('//') ? url.split(/[?#]/, 1)[0] : '[invalid URL]';
  }
};

/** Also remove queries from URLs embedded in an upstream error message. */
export const stripDiagnosticUrlQueries = (message: string): string =>
  message.replace(/(?:https?|wss?):\/\/[^\s<>"'`]+/gi, (url) => {
    const suffix = url.match(/[)\]},;.!]+$/)?.[0] ?? '';
    return diagnosticUrl(url.slice(0, url.length - suffix.length)) + suffix;
  });
