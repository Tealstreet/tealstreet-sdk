# Documentation Scripts

This folder contains helper scripts and migration-time artifacts for the `apps/documentation` workspace.

## Structure

- `doc-drift/`
  - `01-build-scope.ps1`: builds a V3-to-V2 comparison scope from docs paths.
  - `02-score-drift.ps1`: scores each page for drift risk using content/metadata checks.
  - `03-render-reports.ps1`: converts scored JSON output into human-readable markdown reports.
  - `run-terminal-doc-drift-pass.ps1`: orchestration wrapper for the 3-step drift pass.
  - `artifacts/`: stored outputs from drift analysis runs (scope, report JSON, report markdown, remediation plan).
- `prune/`
  - `artifacts/`: one-off pruning audit outputs kept for historical reference.
- `vercel-ignore.sh`: helper to manage docs Vercel ignore behavior during deployment workflows.

## High-Level Context

These scripts were used during docs migration hardening to:

1. compare current docs against legacy/versioned docs coverage,
2. identify likely drift or missing parity areas,
3. generate ranked remediation outputs to prioritize updates.

The artifacts are retained for traceability but are not part of normal docs runtime/build behavior.
