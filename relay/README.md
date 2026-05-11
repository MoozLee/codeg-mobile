# codeg relay

Zero-trust WebSocket forwarder for the codeg mobile app. Deployed as a
Cloudflare Worker + Durable Object; every application frame is already
end-to-end encrypted by the phone and the daemon before it reaches the
Worker, so the relay only ever sees opaque ciphertext.

## Protocol

```
GET /session/<sessionId>/<role>   Upgrade: websocket
```

- `role` is `daemon` or `client`.
- The Worker routes every WebSocket with the same `sessionId` to a
  single Durable Object instance (`env.RELAY.idFromName(sessionId)`),
  which pairs the two sockets and forwards messages verbatim.
- If one side drops, the other is closed with `code=1001, reason="peer
  gone"`.
- 15 minutes of silence tears down the Durable Object.
- Up to 16 messages are buffered while waiting for the peer to connect;
  anything beyond that drops the oldest.

## Local development

```sh
cd relay
npm install
npm run typecheck
npm test
npm run dev          # miniflare dev server
npm run deploy:dry   # offline deploy check
```

## Deployment

```sh
cd relay
npx wrangler login   # interactive OAuth; one-off
npx wrangler deploy
```

On success Cloudflare assigns a free `*.workers.dev` subdomain, e.g.:

```
https://codeg-relay.<your-account>.workers.dev
```

Put that origin into the codeg pairing configuration on the daemon side
(`wss://codeg-relay.<your-account>.workers.dev`). A custom domain is
not required for MVP.

## Scope

- **Does**: pair two WebSocket endpoints by `sessionId`; forward bytes
  verbatim; close the peer when one side drops; time out idle sessions.
- **Does not**: inspect payloads; log or persist message content;
  authenticate clients; provide replay protection; guarantee ordering
  beyond what the underlying WebSocket transport already does.

Because the relay runs on Cloudflare, the operator has no access to the
end-to-end encrypted payload. The daemon and the phone derive a shared
key via X25519 before any application traffic flows, and every frame is
sealed with XSalsa20-Poly1305. See `src-tauri/src/crypto/` for the
details.
