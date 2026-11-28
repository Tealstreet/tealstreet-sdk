# Mobile app docs: screenshots and clips

This file is excluded from the built site (`docs.exclude` in `docusaurus.config.js`).

All screenshots were captured on 2026-11-28 from a Release simulator build of tealstreet-mobile master (build 43,
OTA 6) and live under `static/img/trading/mobile/app/<id>.png`. Each is imported as a module and rendered in a `text--center`
div at `width: 250`. Retake a shot here when its screen changes.

The pages present the app as the product. Getting an invite is one line (Discord or support email). Web screens
appear only where a flow needs its desktop half: the **Cloud sync for API keys** switch and the **QR Code Sync**
modal, recorded on tester1 in an isolated browser, with the QR blurred past scanning.

Annotated steps use `src/components/AnnotatedStep` with stills in `static/img/trading/mobile/steps/`. Callout
boxes are in iPhone points (402 × 874), copied from the app's view hierarchy (Maestro `hierarchy`), or 960 × 608
for desktop stills. They're overlays, so a re-capture only needs the coordinates checked.

Two placeholders were removed rather than captured, and their text kept:

- `signed-in-no-account`: the team test logins all have exchange accounts.
- `update-available`: it only appears when an OTA newer than the running bundle is pending, and faking one would
  show something users never see.

The #245 and #246 "older versions" fallbacks were dropped from `connect-accounts.mdx` in the same change.

## Capture rules

- Use the team test login `sam+tester1@tealstreet.io`. Never use a personal account.
- **Never show an API key, secret, passphrase, wallet key, login email or user id**, even for team test accounts.
  Forms are captured empty. Clips are cut so no such frame is kept: `add-key` jumps from the empty key fields to the
  Connected list, `signin` jumps from the empty sign-in page to the signed-in app, and `cloud-jobs` skips the side
  panel (its header shows the email). Web stills blur the Settings header's User ID, and `app/side-panel.png` has
  its email blurred. Scan every clip frame by frame before publishing.
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
