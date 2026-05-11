# iOS URL Scheme Configuration (`codeg://`)

The codeg mobile iOS shell registers a custom URL scheme so that:

- Links shared via chat channels (`codeg://session?session=<id>`) open the
  app directly in the relevant session.
- Pairing URLs (`codeg://pair?url=<pairing_url>`) pre-fill the pairing
  page on the phone.

Both shapes are parsed by `src/lib/deep-link.ts` on the webview side; the
Rust shell forwards plugin events to the webview via a
`"deep-link"` app event (see `src-tauri/src/mobile/app.rs`).

## Scheme

- **Scheme name**: `codeg`
- **Full form**: `codeg://<path>?<query>` (no host segment).
- **Paths handled today**:
  - `codeg://session?host=<host-id>&session=<session-id>`
    `host` is optional and falls back to the active host.
  - `codeg://pair?url=<url-encoded-pairing-url>`

## Info.plist patch

Tauri 2's `tauri.conf.json` does not yet have a first-class field for iOS
`CFBundleURLTypes`. After running `cargo tauri ios init` for the first
time, open the generated `src-tauri/gen/apple/codeg_iOS/Info.plist` and
merge in the snippet below.

```xml
<key>CFBundleURLTypes</key>
<array>
    <dict>
        <key>CFBundleURLName</key>
        <string>app.codeg.deeplink</string>
        <key>CFBundleTypeRole</key>
        <string>Editor</string>
        <key>CFBundleURLSchemes</key>
        <array>
            <string>codeg</string>
        </array>
    </dict>
</array>
```

Notes:

- `CFBundleURLName` can be any reverse-DNS identifier; using
  `app.codeg.deeplink` keeps it consistent with the bundle identifier.
- `CFBundleTypeRole` should be `Editor` so iOS treats the app as the
  primary handler for `codeg://` URLs.
- Do **not** add the `applinks:` entitlement — Universal Links require a
  paid Apple Developer account and are explicitly out of scope for the
  MVP (see `.trellis/tasks/05-11-mobile-app-prd-analysis/prd.md`).

## Keeping the patch in CI

Until Tauri exposes a config key for this, the iOS build workflow in
`.github/workflows/ios-build.yml` should either:

1. Commit the patched Info.plist alongside the generated Xcode project
   (once `tauri ios init` is run and its output is checked in), **or**
2. Apply the patch in CI via a `plutil -insert` step before running
   `cargo tauri ios build --no-sign`:

```bash
plutil -insert CFBundleURLTypes \
  -xml '<array><dict><key>CFBundleURLName</key><string>app.codeg.deeplink</string><key>CFBundleTypeRole</key><string>Editor</string><key>CFBundleURLSchemes</key><array><string>codeg</string></array></dict></array>' \
  src-tauri/gen/apple/codeg_iOS/Info.plist
```

Pick whichever fits the repo's preferred workflow. For the MVP we
recommend option (1): let the developer run `cargo tauri ios init` once
locally, hand-edit the plist, then commit the patched file.

## Verifying the registration

After installing the IPA:

1. Tap `codeg://session?session=1` from iMessage or Notes. iOS should
   prompt "Open in codeg". Accept.
2. The codeg mobile app should open directly on the session page. If the
   session id does not match any paired daemon, the app displays a
   "session not found" message and offers a link back to the sessions
   list.
3. `codeg://pair?url=codeg-pair%3A%2F%2F...` should jump straight to the
   pairing page with the URL prefilled.

If nothing happens when the link is tapped, verify that:

- The IPA is freshly installed (AltStore / Sideloadly / TrollStore).
- The Info.plist in the installed IPA contains the `CFBundleURLTypes`
  block above. On a jailbroken device, inspect with
  `/usr/libexec/PlistBuddy -c "Print :CFBundleURLTypes" Info.plist`.
- No other installed app claims the `codeg` scheme (iOS picks one of
  several competing handlers arbitrarily).
