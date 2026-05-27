---
title: Releases
sidebar_position: 8
---

# Releases

Where the CLI binaries come from, how to install them, and how to keep
yours up to date.

---

## Where releases live

Standalone binaries are published to
[`Tealstreet/cli`](https://github.com/Tealstreet/cli/releases) on
GitHub. Every release ships the same set of artifacts:

| Asset                     | Platform           |
| ------------------------- | ------------------ |
| `tealstreet-darwin-arm64` | Apple Silicon Macs |
| `tealstreet-darwin-x64`   | Intel Macs         |
| `tealstreet-linux-x64`    | Linux x86_64       |
| `tealstreet-linux-arm64`  | Linux arm64        |

Windows users: a `.exe` may be attached to the release — check the
[releases page](https://github.com/Tealstreet/cli/releases) for the
current build. WSL users can install through the macOS/Linux one-liner
inside WSL.

---

## Install channels

| Channel             | Command                                            |
| ------------------- | -------------------------------------------------- |
| One-liner installer | `curl -fsSL https://get.tealstreet.io/cli \| bash` |
| Homebrew            | `brew install tealstreet/tap/tealstreet`           |
| Direct download     | Pick the matching asset from the releases page     |
| List releases       | `gh release list --repo Tealstreet/cli`            |

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
