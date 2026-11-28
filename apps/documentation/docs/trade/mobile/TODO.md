# Mobile app docs: screenshots

This file is excluded from the built site (`docs.exclude` in `docusaurus.config.js`).

All screenshots were captured on 2026-11-28 from a Release simulator build of tealstreet-mobile master (build 43,
OTA 6) and live under `static/img/trading/mobile/app/<id>.png`. Each is imported the way `index.mdx` imports
`mobile-addtohomescreen.png` and rendered in a `text--center` div at `width: 250`, except the landscape
`web-qr-modal`, which renders at `width: 500` so its text stays readable. Retake a shot here when its screen changes.

Two placeholders were removed rather than captured, and their text kept:

- `signed-in-no-account`: the team test logins all have exchange accounts.
- `update-available`: it only appears when an OTA newer than the running bundle is pending, and faking one would
  show something users never see.

The #245 and #246 "older versions" fallbacks were dropped from `connect-accounts.mdx` in the same change.

## Capture rules

- Use the team test login `sam+tester1@tealstreet.io`. Never use a personal account.
- **Never show a real API key, secret, passphrase, wallet key or a revealed account QR code.** Forms are captured
  empty, and the web QR modal is captured before **Show QR code** is pressed.
- Use small positions and orders on the test account. Crop out the status bar's carrier name and notifications.
- iPhone shots come from the iOS simulator or TestFlight build 43 or later. Use the same device frame throughout.

## Shot list (19)

| id                      | Page             | Screen and state                                                                                        |
| ----------------------- | ---------------- | ------------------------------------------------------------------------------------------------------- |
| `web-request-invite`    | getting-started  | Web app: **Updates** panel, **Mobile** tab, request form open (Platform iOS/Android, **Send request**). |
| `signin-top-right-icon` | getting-started  | Trade tab, signed out. Top-right sign-in icon circled.                                                  |
| `signed-in-no-account`  | getting-started  | Trade tab, signed in, no account: empty top-right circle and the **No exchange account** prompt. |
| `side-panel`            | getting-started  | Side panel open from the top-left logo: Settings … Log Out. Test-account email only.                    |
| `update-available`      | getting-started  | Full-screen **Update available** with **Update now** / **Later · 3 remaining**.                         |
| `setup-your-account`    | connect-accounts | **Set Up Your Account** sheet with all four options.                                                    |
| `web-qr-modal`          | connect-accounts | Web **QR Code Sync** modal BEFORE **Show QR code** (instruction and warning visible, no code).          |
| `qr-scanner`            | connect-accounts | Phone **QR Scanner** screen pointed at a blank surface.                                                 |
| `manual-key-form`       | connect-accounts | **Set Up _Exchange_** form, all fields empty.                                                           |
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
