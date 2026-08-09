import { checkCapabilities } from './checkCapabilities';
import { checkRuntimeImports } from './checkRuntimeImports';
import type { ValidateScriptOptions, ValidationResult } from './validateScript';
import { validateScript } from './validateScript';

/**
 * The single validation gate: typecheck ++ runtime-import check ++ capability
 * check.
 *
 * Everything downstream consumes this one function — the MCP `validate_script`
 * tool, the AI generate → repair loop, and editor diagnostics — so that a
 * script can never be "valid" in one surface and invalid in another.
 *
 * Issues from both passes share `ValidationIssue`, so they compose into a
 * repair prompt without any per-source special-casing.
 */
export const validate = (source: string, options: ValidateScriptOptions): ValidationResult => {
  const typecheck = validateScript(source, options);

  // On a parse failure the AST is unreliable, and speculative import/capability
  // findings layered on top of a syntax error just bury the one thing the
  // author (or the repair loop) actually needs to fix.
  if ((typecheck.syntacticErrorCount ?? 0) > 0) return typecheck;

  const importIssues = checkRuntimeImports(source);
  const capabilityIssues = checkCapabilities(source);
  const errors = [...typecheck.errors, ...importIssues, ...capabilityIssues];
  return { valid: errors.length === 0, errors };
};
