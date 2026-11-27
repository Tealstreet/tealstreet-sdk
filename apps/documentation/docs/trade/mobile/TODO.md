# Mobile app docs: screenshots to capture

This file is excluded from the built site (`docs.exclude` in `docusaurus.config.js`).

Every page in this folder has `SCREENSHOT PLACEHOLDER: <id>` admonitions where an image belongs. Replace each one
with an image under `static/img/trading/mobile/app/<id>.png`, imported the way `index.mdx` imports
`mobile-addtohomescreen.png` and rendered in a `text--center` div at `width: 250`.

## Before promoting to production

The `SCREENSHOT PLACEHOLDER` admonitions render on the public pages. Replace each with a capture, or remove it,
before this section is promoted.

## Follow-ups from tealstreet-mobile

- **#246 (Accounts settings cleanup):** merged. The pages lead with its layout (tap a row to edit, **Delete** in the
  edit form, **Add account**) and keep the old row **Edit** / **Delete** and header **Add** as "older versions"
  fallbacks in `connect-accounts.mdx`. Drop that fallback wording once #246 has reached every tester.
- **#245 (Sign In on the signed-out import alert):** merged. Drop the "older versions only have OK" sentence in
  `connect-accounts.mdx` once that update has reached every tester.

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
