# Mobile app docs: screenshots and clips

This file is excluded from the built site (`docs.exclude` in `docusaurus.config.js`).

All screenshots were captured on 2026-11-28 from a Release simulator build of tealstreet-mobile master (build 43,
OTA 6) and live under `static/img/trading/mobile/app/<id>.png`. Each is imported as a module and rendered in a `text--center`
div at `width: 250`. Retake a shot here when its screen changes.

The pages describe the app as the product. They carry no web-app steps, web links or web screenshots: getting an
invite is one line (Discord or support email), and accounts are added with an API key in the app.

Two placeholders were removed rather than captured, and their text kept:

- `signed-in-no-account`: the team test logins all have exchange accounts.
- `update-available`: it only appears when an OTA newer than the running bundle is pending, and faking one would
  show something users never see.

The #245 and #246 "older versions" fallbacks were dropped from `connect-accounts.mdx` in the same change.

## Capture rules

- Use the team test login `sam+tester1@tealstreet.io`. Never use a personal account.
- **Never show a full API secret, passphrase or wallet key**, even a team test key's. Forms are captured empty, and
  the secret field in the `add-key` clip is masked.
- Use small positions and orders on the test account. Crop out the status bar's carrier name and notifications.
- iPhone shots come from the iOS simulator or TestFlight build 43 or later. Use the same device frame throughout.

## Clips

`index.mdx` renders `src/components/MobileShowcase`, which plays clips from `static/video/mobile/<name>.{webm,mp4,jpg}`.
They were recorded on 2026-11-28 from the same Release build on the "Tealstreet Docs iPhone 17 Pro" simulator (dark
mode, status bar overridden to 9:41), driven by Maestro, on the team Bybit row-4 test key with 0.2 SOL orders that
were closed straight after. Each clip is cut to its taps, has frozen stretches squeezed out (`mpdecimate`), and is
600 px wide: H.264 MP4, VP9 WebM, and a JPEG poster of the last frame. Keep each under ~1.5 MB.

Gotchas when re-recording:

- A TWAP splits into 10 slices and Bybit rejects any slice under its minimum, so size the position at about $60+.
- iOS shows **Save Password?** after a secure field is submitted; cut it out between segments.
- The secure field shows each character briefly while it's typed. Blur the field for the whole typing window, and
  leave password typing out of `signin` entirely.
- The News **Everything** feed is unfiltered. Record **Traders** only.

| name          | Shows                                                                     |
| ------------- | ------------------------------------------------------------------------- |
| `trade-order` | Market order type, size 0.2, Buy, fill toast.                             |
| `trade-tpsl`  | Dragging TP and SL off the position line on the chart.                    |
| `trade-close` | Position swiped left, **Close**, fill, **No Open Positions**.             |
| `markets`     | Market tab: **Perp** filter, then a symbol search for ETH.                |
| `portfolio`   | Portfolio with the position, then its two stops on **Orders**.           |
| `news`        | News **Traders** feed. Never record **Everything**: it is unfiltered.     |
| `alert`       | Creating a SOLUSDT price alert. Delete it afterwards.                     |
| `cloud-jobs`  | **TWAP 1m** close of 1,100 DOGE, then the **Cloud jobs** screen.          |
| `signin`      | **Log In** → sign-in page (shows **Sign up**) → email → back in the app.  |
| `add-key`     | **Add account** → **Manually Enter API Keys** → Bybit → **Connected**.    |

## Shot list

| id                      | Page             | Screen and state                                                                                        |
| ----------------------- | ---------------- | ------------------------------------------------------------------------------------------------------- |
| `signin-top-right-icon` | getting-started  | Trade tab, signed out. Top-right sign-in icon circled.                                                  |
| `signed-in-no-account`  | getting-started  | Trade tab, signed in, no account: empty top-right circle and the **No exchange account** prompt. |
| `side-panel`            | getting-started  | Side panel open from the top-left logo: Settings … Log Out. Test-account email only.                    |
| `update-available`      | getting-started  | Full-screen **Update available** with **Update now** / **Later · 3 remaining**.                         |
| `order-form-limit`      | first-trade      | Trade tab, **Limit**: Buy/Sell pills, Price, Size, $ buttons, Post/Reduce, order button and info card. |
| `positions-tab`         | first-trade      | **Positions** tab, one small test position swiped left: Actions / BE / Close. |
| `quick-actions`         | first-trade      | **Quick Actions** sheet on its **Close** tab.                                                           |
| `orders-tab`            | first-trade      | **Orders** tab, one resting order swiped left to show **Cancel**.                                       |
| `trade-chart`           | first-trade      | Trade tab with the account bar open (bell visible) and the chart's top bar. |
| `tab-trade`             | feature-tour     | Full Trade tab.                                                                                         |
| `tab-market`            | feature-tour     | Market tab on **All Markets**.                                                                          |
| `tab-portfolio`         | feature-tour     | Portfolio tab.                                                                                          |
| `tab-news`              | feature-tour     | News tab on **Everything**.                                                                             |
| `cloud-jobs`            | feature-tour     | **Cloud jobs** screen with intro text and one test account.                                             |
