---
title: Releases
sidebar_position: 8
---

# Releases

Where the CLI binaries come from, how to install them, and how to keep
yours up to date.

Rust exchange accounts run an engine embedded in the standalone executable.
There is no separate Rust runtime or WASM asset to install beside the binary.

---

## Where releases live

Standalone binaries are published to
[`Tealstreet/cli`](https://github.com/Tealstreet/cli/releases) on
GitHub. Every release ships the same set of artifacts:

| Asset                        | Platform           |
| ---------------------------- | ------------------ |
| `tealstreet-darwin-arm64`    | Apple Silicon Macs |
| `tealstreet-darwin-x64`      | Intel Macs         |
| `tealstreet-linux-x64`       | Linux x86_64       |
| `tealstreet-linux-arm64`     | Linux arm64        |
| `tealstreet-windows-x64.exe` | Windows x64        |

The Disrupt build of the same CLI is published separately to
[`DisruptTrade/cli`](https://github.com/DisruptTrade/cli/releases) with the
same five assets named `disrupt-<platform>`. It is GitHub-only: no npm
package and no Homebrew formula. Install it by downloading the binary for
your platform from that release page. It logs in through `app.disrupt.trade`,
keeps its files under `~/.disrupt/`, `disrupt update` polls that repo, and its
built-in help links to that repository rather than to this site.

A fifth asset, `tealstreet-windows-x64.exe`, covers Windows x64.
`bun build --compile` appends the `.exe` itself on a Windows target, so the
suffix is part of the asset name, the `SHA256SUMS` entry and the cached
filename — `assetName()` in `scripts/install-binary.cjs` is the single place
that derives it.

**Every release is smoke-tested on real Windows.** `.github/workflows/cli-windows-smoke.yml`
runs on a GitHub-hosted `windows-latest` runner after the release workflow
finishes: it installs from npm, checks `--version`, `--help` and
`account list`, and separately downloads the `.exe` and runs it. It is a
separate workflow so a hosted-runner outage cannot fail a release that has
already published. It does NOT cover the REPL's rendering — that needs an
interactive terminal and stays a human check.

**Windows self-update replaces the binary by rename, not by overwrite.**
Windows locks a running `.exe`: it cannot be deleted or written over, but it
CAN be renamed, and the open handle follows. So the update stages the
download beside the current binary, renames the running one to `.old`,
renames the download into its place, and deletes the `.old` on the next
launch. If the second rename fails the first is rolled back, because a
half-applied update would leave the user with no `tealstreet.exe` at all.
npm installs take the `npm update -g` path instead. There is no Homebrew or
one-liner channel for Windows.

---

## Install channels

| Channel             | Command                                                                               |
| ------------------- | ------------------------------------------------------------------------------------- |
| One-liner installer | `curl -fsSL https://raw.githubusercontent.com/Tealstreet/cli/main/install.sh \| bash` |
| Homebrew            | `brew install tealstreet/tap/tealstreet`                                              |
| Direct download     | Pick the matching asset from the releases page                                        |
| List releases       | `gh release list --repo Tealstreet/cli`                                               |

The one-liner detects platform/arch, fetches the matching binary from
the latest release, and drops it into `/usr/local/bin`. The Homebrew
formula lives in
[`Tealstreet/homebrew-tap`](https://github.com/Tealstreet/homebrew-tap).

Both channels install the same binary. Use whichever fits how you
manage CLIs on your machine. See [Install](../install.md) for the full
first-run walkthrough.

---

## Updating

The binary updates itself in place. From a shell:

```bash
tealstreet update           # download + replace the running binary
tealstreet --update         # same — top-level flag
tealstreet update -c        # check only, don't fetch
tealstreet --check-update   # same — top-level flag
```

From inside the REPL:

```text
> update           # fetch and replace
> update -c        # check only
> update --check   # same
```

After `tealstreet update`, restart the CLI for the new binary to take
effect. The current process keeps running the old binary it started
with — only the on-disk file is replaced.

---

## Background update check

Every REPL launch runs an asynchronous version check and prints a
banner when a newer release is available. The banner stays visible
until you run `update` (which fetches) or `update -c` (which just
confirms what's available). It re-evaluates on the next launch — the
suppression is per-process.

If you'd rather skip the check entirely for a session, the network
hit is cheap enough that there's no flag for that. Run offline if you
need silence.

---

## Changelog

Per-version release notes — the user-visible changes for each build —
are published on the
[GitHub releases page](https://github.com/Tealstreet/cli/releases); the
[latest release](https://github.com/Tealstreet/cli/releases/latest) is
always what `tealstreet update` fetches. In practice you don't have to
go looking: the [background update check](#background-update-check)
prints a banner in the REPL whenever a newer version is out, and the
release page is one click away for the details.

---

## See also

- [Install](../install.md) — first-run setup.
- [Troubleshooting](./troubleshooting.md) — what to do when the update
  banner sticks or the binary won't replace itself.
