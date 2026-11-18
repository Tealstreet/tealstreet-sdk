/**
 * Secondary (read-only) API keys are bypassed: a saved read key is ignored, signed
 * reads use the primary key and its route, and only the account's unsigned public
 * GETs keep the read egress (`secondaryProxyUrl`). Every secondary path stays in
 * place; set this to true to restore them.
 */
export const SECONDARY_READ_KEYS_ENABLED = false;
