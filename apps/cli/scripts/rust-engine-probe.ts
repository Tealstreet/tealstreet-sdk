import '../src/rust-engine';

// Reuse the package artifact's strict documented public input. The CLI loader
// registers first; the MCP probe's optional sibling-file loader is absent here.
import '../../../packages/exchange-mcp/scripts/rust-probe';
