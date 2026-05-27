# Tealstreet CLI Service Examples

This directory contains example configuration files for running `tealstreet serve` as a background service using various process supervisors.

## Quick Start: Daemon Mode

The simplest way to run in the background:

```bash
tealstreet serve --daemon
```

This will:
- Fork to background
- Log to `~/.tealstreet/serve.log`
- Write PID to `~/.tealstreet/serve.pid`

Stop with: `kill $(cat ~/.tealstreet/serve.pid)`

## systemd (Linux)

For Linux systems using systemd:

1. Copy the service file:
   ```bash
   cp tealstreet-serve.service ~/.config/systemd/user/
   ```

2. Reload and enable:
   ```bash
   systemctl --user daemon-reload
   systemctl --user enable tealstreet-serve
   systemctl --user start tealstreet-serve
   ```

3. Check status:
   ```bash
   systemctl --user status tealstreet-serve
   journalctl --user -u tealstreet-serve -f
   ```

## launchd (macOS)

For macOS:

1. Copy and edit the plist (replace YOUR_USERNAME):
   ```bash
   cp io.tealstreet.serve.plist ~/Library/LaunchAgents/
   sed -i '' "s/YOUR_USERNAME/$USER/g" ~/Library/LaunchAgents/io.tealstreet.serve.plist
   ```

2. Load and start:
   ```bash
   launchctl load ~/Library/LaunchAgents/io.tealstreet.serve.plist
   launchctl start io.tealstreet.serve
   ```

3. Check status:
   ```bash
   launchctl list | grep tealstreet
   tail -f ~/.tealstreet/serve.log
   ```

## pm2 (Cross-platform)

Using pm2 process manager:

```bash
# Install pm2
npm install -g pm2

# Start tealstreet serve
pm2 start "tealstreet serve --local-only" --name tealstreet

# Save for auto-start on reboot
pm2 save
pm2 startup

# View logs
pm2 logs tealstreet

# Restart/stop
pm2 restart tealstreet
pm2 stop tealstreet
```

## Docker

```dockerfile
FROM oven/bun:latest
RUN bun install -g @tealstreet/cli
CMD ["tealstreet", "serve", "--local-only"]
```

Run with:
```bash
docker run -d --name tealstreet --restart=unless-stopped \
  -v ~/.tealstreet:/root/.tealstreet \
  -p 23456:23456 \
  tealstreet-cli
```

## Supervisor Configuration

All examples include:
- **Auto-restart on crash** with backoff
- **Logging** to file or journal
- **Auto-start on boot/login**

Adjust the `--local-only` flag based on your needs:
- `--local-only` - Only local connections (recommended for most users)
- Remove flag - Enable relay for remote connections (requires pairing)
