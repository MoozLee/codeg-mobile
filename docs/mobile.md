# codeg Mobile (iOS)

codeg Mobile is a Tauri 2 iOS companion that lets you view and drive the
AI coding agents running on your desktop codeg install from anywhere.
It reuses the existing Next.js static bundle with a responsive layout
for narrow screens, connecting to the desktop daemon over an end-to-end
encrypted Cloudflare Workers relay (with optional LAN fallback). No
data, prompts, or agent output passes through the relay in plaintext.

For the full MVP scope, roadmap, and design rationale see the parent
PRD at
[`.trellis/tasks/05-11-mobile-app-prd-analysis/prd.md`](../.trellis/tasks/05-11-mobile-app-prd-analysis/prd.md).

## Overview

```
┌──────────────────────────────┐       ┌───────────────────────────────┐
│ codeg Desktop (Tauri/server) │       │ codeg Mobile (Tauri 2 iOS)    │
│ ---------------------------- │       │ ----------------------------- │
│ - runs AI coding agents      │       │ - responsive Next.js bundle   │
│ - reads local session files  │       │ - scans pairing QR codes      │
│ - pairs phones (Curve25519)  │       │ - handles codeg:// deep links │
│ - emits session events to    │       │ - unlocks token via biometric │
│   chat channels + relay      │       │                               │
└──────────────┬───────────────┘       └───────────────┬───────────────┘
               │ WebSocket (outbound, E2EE)            │ WebSocket (E2EE)
               ▼                                       ▼
       ┌───────────────────────────────────────────────────────┐
       │ Cloudflare Workers Relay (Durable Object per session) │
       │ zero-trust: sees only ciphertext, routes by session id │
       └───────────────────────────────────────────────────────┘
```

Key properties:

- **Thin mobile client**: the phone never runs a daemon or parses agent
  files. It only speaks the relay's E2EE protocol and renders results.
- **Apache-2.0 compatible crypto stack**: Curve25519 + XSalsa20-Poly1305
  via `crypto_box` (Rust) and `tweetnacl` (web). We do not link against
  or copy any AGPL-licensed code (e.g. Paseo's relay protocol).
- **Single codebase**: the mobile shell loads the same static Next.js
  export the desktop app does. Compact screens activate mobile-only
  components via Tailwind breakpoints and `useIsCompactFormFactor()`.
- **Chat-channel push**: there is no APNs / FCM integration in the MVP.
  Instead the desktop daemon forwards `turn_complete` / `error` events
  to your existing Telegram / Feishu / WeChat chat channel, with a
  `codeg://session?session=<id>` deep link that opens the mobile app
  on the matching session.

## Pairing

The pairing ceremony exchanges a Curve25519 long-term key (daemon) and
an ephemeral key (phone) so both sides can derive a shared NaCl `box`
secret. The relay only sees ciphertext; even a fully compromised relay
cannot decrypt the traffic.

Step by step:

1. **On the desktop**, start codeg and open
   `Settings → Mobile Devices`. This page lists all phones currently
   paired and exposes a `Pair a new device` button.
2. Click **Pair a new device**. The desktop generates a random relay
   session id and renders a QR code whose URL has the shape:

   ```
   codeg-pair://<relay-host>/r/<session-id>#<base64(daemon-pubkey)>
   ```

   The public key rides in the URL fragment so the relay itself never
   receives it. The URL is also displayed in text form so you can copy
   it manually if you prefer not to scan.
3. **On the phone**, you have two entry paths:
   - Tap **the QR code** from the iOS Camera app. Because the codeg
     mobile shell registers the `codeg-pair://` scheme, Camera will
     offer "Open in codeg" and deep-link you straight into the pairing
     page.
   - Or open codeg mobile → bottom nav **Pair** → paste the URL you
     copied from the desktop, enter a human-friendly device name, and
     tap **Connect**.
4. The phone generates an ephemeral keypair, connects to the relay
   Durable Object for that session id, sends an `e2ee_hello {client_pub}`
   frame, and waits for the daemon's `e2ee_ready` response. Once the
   shared secret is derived, the phone sends `device_register { nickname }`
   and the daemon writes a row into `paired_devices`.
5. **Back on the desktop**, the newly connected device shows up in the
   list with its nickname, the first eight characters of its public
   key, last-active timestamp, and status (`connected`).
6. The phone navigates to **Sessions** and starts receiving live
   updates for the currently-active daemon.

To revoke a device, click **Remove** on its row. The daemon immediately
stops the associated relay client and drops its key material, so the
phone loses the connection within a few seconds and can no longer
establish a new one with the same keypair.

## Installing unsigned IPA

Because we do not maintain an Apple Developer Program account, the CI
workflow (`.github/workflows/ios-build.yml`) produces an **unsigned**
IPA named `codeg-mobile-<ref>-ios-unsigned.ipa` and attaches it to
GitHub Releases when a `mobile-v*` tag is pushed. You bring your own
signing solution.

Pick one of the three paths below based on how long you want the app to
last between re-signs and what your device supports.

### Option 1 — AltStore (recommended for beginners)

AltStore re-signs the IPA with your personal Apple ID and refreshes it
every seven days over Wi-Fi. It is the least-invasive option and works
on any supported iPhone / iPad.

1. Install **AltServer** on your desktop:
   [https://altstore.io](https://altstore.io) (macOS and Windows
   builds available). On macOS you also need iTunes or the Apple
   Devices app for the USB pairing handshake.
2. Plug your iPhone into the desktop over USB, trust the computer,
   then open AltServer → `Install AltStore → <your device>`.
3. On the phone, open `Settings → General → VPN & Device Management`
   and trust the developer certificate tied to your Apple ID.
4. Download `codeg-mobile-<ref>-ios-unsigned.ipa` from GitHub Releases
   on to the desktop.
5. Drag the IPA on to the AltServer menu-bar icon, or open AltStore on
   the phone → `My Apps → + → select the IPA`.
6. Enter your Apple ID password. AltServer re-signs the IPA locally
   and pushes it to the device.
7. Keep AltServer running on the desktop with your phone on the same
   Wi-Fi network; AltStore will quietly renew the 7-day signature in
   the background.

### Option 2 — Sideloadly (desktop one-shot)

Sideloadly is similar to AltStore but performs the re-sign interactively
from the desktop, without installing a companion app on the phone.

1. Download Sideloadly: [https://sideloadly.io](https://sideloadly.io)
   (macOS / Windows).
2. Plug in the iPhone; trust the computer.
3. Open Sideloadly, drag the IPA into the main window, enter your
   Apple ID, and click **Start**.
4. When prompted, trust the developer certificate under
   `Settings → General → VPN & Device Management` on the phone.
5. Re-run the process roughly every seven days. There is no automatic
   refresh — set a calendar reminder.

### Option 3 — TrollStore (permanent signature)

If your device runs an iOS version still vulnerable to the TrollStore
installer (roughly iOS 14.0 through 17.0 before Apple patched CoreTrust),
TrollStore will install the IPA with a permanent signature that never
expires.

1. Check compatibility and follow the installer at
   [https://github.com/opa334/TrollStore](https://github.com/opa334/TrollStore).
   The exact procedure varies by device and iOS version (TrollHelper,
   TrollInstallerMDC, etc.).
2. Once TrollStore is installed, open it, tap the `+` icon, select the
   IPA, and confirm.
3. The app launches directly; no certificate trust prompt and no
   expiry.

## Troubleshooting

- **Camera does not offer "Open in codeg" when scanning the QR code.**
  The iOS Camera app only surfaces the handoff for URL schemes it has
  seen at least once. If nothing happens:
  - Confirm the URL on screen actually starts with `codeg-pair://`
    (not `https://`).
  - Fall back to copying the URL text below the QR code and pasting it
    into `codeg mobile → Pair`.
  - On a freshly-installed mobile build, open the app once before
    scanning so iOS learns about the scheme.

- **Phone cannot reach the relay.** The desktop daemon logs the full
  relay origin on startup. Verify:
  - The origin is reachable from the phone (try opening `https://`
    variant in mobile Safari — you should see a Cloudflare worker
    response page).
  - Your corporate firewall or VPN is not blocking outbound WSS to
    `*.workers.dev`.
  - `Settings → Mobile Devices` shows the daemon as `connected`; if it
    says `connecting`, wait ~30s and check again.
  - If you changed the relay origin in the desktop settings, reset it
    to the default `wss://codeg-relay.workers.dev` and retry.

- **"device already paired" error on the desktop.** Each pairing
  generates a fresh keypair. If the daemon sees a `device_register`
  whose public key is already in `paired_devices`, it rejects the new
  session. Remove the older row from the Mobile Devices list and
  re-run pairing.

- **App stops launching after ~7 days.** Personal Apple ID
  certificates expire weekly. Open AltStore → `My Apps → Refresh`, or
  re-run Sideloadly. Users who prefer zero maintenance should switch
  to TrollStore when compatible.

- **Tapping a `codeg://` deep link does nothing.** iOS may route the
  link to a different app if another app also claims the scheme.
  Check `Settings → <app> → Default Browser App` and similar handler
  settings, or manually open codeg mobile and paste the link into the
  pairing / session page as a workaround.

- **Message stream lags by more than five seconds.** Cloudflare Workers
  run on the edge location closest to the *relay initiator*, which is
  the desktop daemon. If you travel far from your home network the
  round-trip suffers. Tap the refresh control in the sessions list to
  force a reconnect; we are tracking "preferred region" settings as a
  post-MVP feature.

- **Desktop `cargo tauri dev` is running but the phone can't connect.**
  Development builds default to a localhost relay for debugging. Open
  `Settings → Mobile Devices → Relay origin` and confirm it points at
  a publicly reachable relay URL rather than `ws://127.0.0.1:...`. See
  `src-tauri/src/app_state.rs` for how the default is resolved.

## Known limitations

- **No native push notifications.** The MVP relies entirely on the
  chat-channel deep-link flow. If you have no Telegram / Feishu /
  WeChat channel configured, the phone receives nothing until you open
  the app. Post-MVP we may revisit APNs or a third-party Tauri push
  plugin.
- **Unsigned IPA requires self-signing.** Users without AltStore,
  Sideloadly, or TrollStore cannot install the app. This is by design —
  we are not joining the Apple Developer Program for the MVP.
- **No photo / media attachments.** `tauri-plugin-image-picker` does
  not exist yet, and the iOS file picker is too unstable to ship. The
  composer is text-only.
- **No LAN mDNS auto-discovery.** All connections go through the relay,
  even on the same Wi-Fi network. LAN fallback is tracked for a later
  milestone.

See `.trellis/tasks/05-11-mobile-app-prd-analysis/prd.md` under
"Out of Scope" for the full list of features deferred past the MVP.
