// stdout carries the MCP protocol, so anything safe-cex logs goes to stderr.
// Imported first, before any module that might log while loading.
console.log = console.error;
console.info = console.error;
console.debug = console.error;
console.warn = console.error;
