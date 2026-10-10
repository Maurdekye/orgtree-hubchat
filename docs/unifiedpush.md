# UnifiedPush on Android

Hubchat's background connection remains the default. Optional UnifiedPush
uses an installed distributor app to wake Hubchat when mail arrives. Your
mail hub sends the wake directly to that distributor's server. There is no
Hubchat relay or project-operated push server.

1. Install and set up a UnifiedPush distributor, such as
   [ntfy](https://ntfy.sh/docs/subscribe/phone/). Its server may be public or
   self-hosted.
2. Open **Settings → Notifications**, select the distributor, and turn on
   **Use UnifiedPush**.
3. Wait for **Push is on**. Every configured hub must support the
   `unifiedpush` feature and accept this phone's registration before Hubchat
   drops its ongoing connection notification.

While push is active, Hubchat fetches messages when woken and connects normally
while the app is visible. Turning push off restores the saved **Stay connected**
choice. If setup is incomplete, Hubchat uses that choice too. Android can
delay background work; keep the distributor working and allow it the battery
access its own setup instructions recommend.

The hub defers wakes while this device reports itself in use. Going into the
background clears that signal. If the app crashes before clearing it, pending
wakes resume when the signal expires, within 90 seconds.

If the distributor disappears or rejects registration, Settings explains what
needs attention. Open Hubchat and retry after fixing it. A distributor requiring
VAPID is not supported by this version; choose one such as ntfy that works
without VAPID. If Android prevents restarting Hubchat's foreground service while
the app is hidden, periodic checks remain scheduled until you open the app.

## Tailnets and self-hosted distributors

A tailnet-only hub works if it can reach the distributor's server. The phone
still needs its usual connection, including Tailscale or another VPN, to fetch
the actual messages from that hub.

By default, mail hubs accept only public HTTPS distributor endpoints. A hub
operator using a private LAN or tailnet distributor must opt in with
`HUB_PUSH_ALLOW`, listing exact hosts or CIDRs, for example
`push.example.ts.net,100.64.0.0/10`. Prefer the narrowest entry that works.
Allowed endpoints still need HTTPS with a valid certificate; redirects are not
followed. See the mail hub's operations guide for its configuration.

## What is stored and sent

The hub stores a separate endpoint and Web Push key pair metadata for each
device: the public encryption key (`p256dh`) and authentication secret (`auth`).
It sends only RFC 8291 encrypted bytes containing the constant `wake`. No message
text, sender, address, or message identifier is in that payload. The distributor
can still observe delivery timing and network metadata.

The Android connector manages the receiving key. Hubchat seals its endpoint
and upload metadata with Android Keystore before storing them. These values
do not appear in the settings API, device listings, or application logs.
Disabling push removes the local capability immediately and queues hub removal;
unreachable hubs are retried. Revoking the device also removes its hub-side
registration. The hub drops dead endpoints when the distributor returns 404
or 410.

## Development checks

`cargo test -p hubchat-core --test push_suspend` exercises suspension and
resumption against a local TCP listener, including closing a stalled request.
The mail hub's `unifiedpush` integration suite uses disposable PostgreSQL and a
local mock distributor to decrypt the real encrypted wake and exercise retries,
coalescing, registration replacement and revocation. Its `push-test` feature
permits loopback HTTP only for debug tests and is rejected in release builds.

The browser mock supports `?platform=android&distributor=ntfy` for settings
layout checks and `?platform=android` for the missing-distributor state. These
checks do not establish delivery behavior on a phone. Device verification must
also cover a killed app process, Android background restrictions, distributor
endpoint rotation/removal, and receipt of a message while the app is hidden.
