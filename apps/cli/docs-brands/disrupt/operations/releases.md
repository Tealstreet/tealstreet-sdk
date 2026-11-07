# Releases

Where the CLI binaries come from, how to install them, and how to keep
yours up to date.

---

## Where releases live

Standalone binaries are published to
[`DisruptTrade/cli`](https://github.com/DisruptTrade/cli/releases) on
GitHub. Every release ships the same set of artifacts plus a `SHA256SUMS`
file:

| Asset                     | Platform           |
| ------------------------- | ------------------ |
| `disrupt-darwin-arm64`    | Apple Silicon Macs |
| `disrupt-darwin-x64`      | Intel Macs         |
| `disrupt-linux-x64`       | Linux x86_64       |
| `disrupt-linux-arm64`     | Linux arm64        |
| `disrupt-windows-x64.exe` | Windows x64        |

The Windows build carries the `.exe` suffix in the asset name because the
compiler appends it itself; the checksum line and the cached filename use
the same name.

**Windows self-update replaces the binary by rename, not by overwrite.**
Windows locks a running `.exe`: it cannot be deleted or written over, but it
CAN be renamed, and the open handle follows. So the update stages the
download beside the current binary, renames the running one to `.old`,
renames the download into its place, and deletes the `.old` on the next
launch. If the second rename fails the first is rolled back, because a
half-applied update would leave the user with no `disrupt.exe` at all.

---

## Install channels

| Channel         | How                                            |
| --------------- | ---------------------------------------------- |
| Direct download | Pick the matching asset from the releases page |
| List releases   | `gh release list --repo DisruptTrade/cli`      |

There is no package-manager channel. See [Install](../install.md) for the
full first-run walkthrough.

---

## Updating

The binary updates itself in place. From a shell:

```bash
disrupt update           # download + replace the running binary
disrupt --update         # same — top-level flag
disrupt update -c        # check only, don't fetch
disrupt --check-update   # same — top-level flag
```

From inside the REPL:

```text
> update           # fetch and replace
> update -c        # check only
> update --check   # same
```

After `disrupt update`, restart the CLI for the new binary to take
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
[GitHub releases page](https://github.com/DisruptTrade/cli/releases); the
[latest release](https://github.com/DisruptTrade/cli/releases/latest) is
always what `disrupt update` fetches. In practice you don't have to
go looking: the [background update check](#background-update-check)
prints a banner in the REPL whenever a newer version is out, and the
release page is one click away for the details.

---

## See also

- [Install](../install.md) — first-run setup.
- [Troubleshooting](./troubleshooting.md) — what to do when the update
  banner sticks or the binary won't replace itself.
