# Running Hubchat Web (the server)

Hubchat Web is Hubchat as a web app, for people who have no Hubchat app for
their device: mainly an iPhone or iPad, and any computer where nothing can
be installed. You run one small server; people open its address in a
browser and, on a phone, add it to the Home Screen.

**What the server does:** it hands out the web app's files. That is all.

**What it does not do:** it holds no accounts, no identities, no keys, no
messages and no database. It sets no cookie, needs no volume and connects
to nothing. Each person's identity and keys are made and kept in their own
browser, and that browser talks to the mail hub directly. The server never
sees a message or a hub request.

Whoever runs the server controls the code those browsers run, so use a
server you or your organisation run (the app says this too, in Settings,
About).

How the app itself works, and what is not in it yet: `docs/hubchat-web.md`.

## How it differs from the other two packages

| Package | What it is | Holds data? |
|---|---|---|
| `ghcr.io/maurdekye/hubchat-web` (this) | a static web server for the Hubchat web app | no |
| `ghcr.io/maurdekye/orgtree-mailhub` | the mail hub: carries mail between orgs and people; Hubchat (every app, and this web app) connects to one | yes: PostgreSQL and a data volume |
| `ghcr.io/maurdekye/orgtree-engine` | the headless Orgtree engine: runs an organisation of AI agents without the desktop app | yes: its organisations |

Hubchat Web needs a mail hub to be of use, but not on the same machine, and
it never talks to the hub itself.

## Run it

```
docker run -d --name hubchat-web --read-only --cap-drop ALL \
  -p 127.0.0.1:8080:8080 ghcr.io/maurdekye/hubchat-web:latest
```

or with `compose.web.yaml` from this repository:

```
docker compose -f compose.web.yaml up -d
```

The image is one program and nothing else (no shell). It runs as a
non-root user, writes nothing, and answers on port 8080 over plain HTTP.
Its own health check (`hubchat-web healthcheck`) marks the container
healthy. Tags: a version (`1.3.7`, the Hubchat release it came with) and
`latest`, the newest stable release. The image is for 64-bit x86 (amd64).
Each Hubchat release also carries the image as a file
(`hubchat-web-<version>-docker-amd64.tar.gz`, for `docker load`) and the
plain files (`hubchat-web-<version>-static.tar.gz`) for any web server of
your own.

## HTTPS is required, twice

A browser gives a page its storage, its camera and a Home Screen icon only
over HTTPS, and a page loaded over HTTPS can only call a hub over HTTPS. So:

1. **The page needs HTTPS.** Put a proxy in front (Tailscale, Caddy, nginx,
   a cloud load balancer), or give the server a certificate (below).
2. **The hub needs HTTPS** at the address people type. Put its relay-only
   door behind the proxy, never its main port (below).
3. **The hub must allow the page.** In the mail hub's settings, add the
   page's address to `HUB_WEB_ORIGINS` (Mail hub 2.5.0 or later). Without
   it the app says "This hub doesn't allow this web app yet" and shows the
   address to add.

A hub that is reached only on a home or office network gets its certificate
from an authority of your own: [On a LAN only](#on-a-lan-only).

**Notifications** come from the hub, not from this server: a hub running
Mail hub 2.5.1 or later sends them to each browser that turned them on
(its `HUB_PUSH_CONTACT` setting is in the mail hub's `docs/OPERATIONS.md`). With an
older hub everything else works and the app names the hub that "needs an
update before it can send notifications to this browser". This server has
no setting for them.

With Tailscale, on the machine that runs both:

```
tailscale serve --bg --https=443  http://127.0.0.1:8080     (Hubchat Web)
tailscale serve --bg --https=8443 http://127.0.0.1:7378     (the mail hub's relay-only door)
```

Port 7378 is the standalone mail hub's **relay-only door**: the hub's API
and nothing else. Never put the hub's main port (7370) behind an address
other people can reach: it also serves the operator page, which shows all
mail. (Orgtree's built-in hub has its door on 7371.) In the hub's `.env`:

```
HUB_PUBLIC=1                                          (turns the door on)
HUB_BIND=127.0.0.1                                    (the main port stays on this machine)
HUB_PUBLIC_BIND=127.0.0.1                             (the door too: only Tailscale reaches it)
HUB_WEB_ORIGINS=https://<machine>.<tailnet>.ts.net    (the page may call the hub)
```

Leave `HUB_PUBLIC_BIND` out if Hubchat apps on other devices reach the door
directly at `<machine>:7378`. For a one-tap first hub, give Hubchat Web
`HUBCHAT_WEB_SUGGEST_HUBS=<machine>.<tailnet>.ts.net:8443`.

On an iPhone: open the address in Safari, tap **Share**, then **Add to
Home Screen**, and open Hubchat from there. (In a Safari tab the app only
explains this: an iPhone deletes a web page's storage after some days
unless it is on the Home Screen.)

### Why HTTPS

The hub is served over HTTPS for browsers so that Hubchat Web keeps
notifications (Web Push) and linking a device with the camera. Browsers
allow both only on a secure page, and a page loaded over HTTPS can't call a
hub over plain HTTP. A reduced mode over plain HTTP, without those two, was
considered and rejected. This holds on a LAN as well: there the certificate
comes from a certificate authority of your own
([On a LAN only](#on-a-lan-only)).

The decision and its reason, in the project owner's words (2026-11-16,
11:18:51 UTC): "i would highly prefer we have notifications and camera
linking, those make the app's usage much cleaner". And at 11:20:29 UTC:
"make sure this is the recorded reason for why we are using https to serve
the mailhub".

The Hubchat apps are not affected: they keep working with a hub over plain
`http://`. From 1.3.8 they also accept a hub's certificate from an
authority that is installed on the device.

## On a LAN only

A hub that people reach only inside a home or office network, at an address
such as `192.168.1.200`, has no public name, and no public certificate
authority gives a certificate for a private address. Hubchat Web needs
HTTPS there all the same ([Why HTTPS](#why-https)). The way is a
certificate authority of your own: you make one, give the machine a
certificate from it, and install the authority's certificate on every
device. Browsers then accept the machine's certificate, and so do the
Hubchat apps from 1.3.8 on: beside the public authorities they are built
with, they trust the ones the device trusts. (Older apps refuse it; they
can stay on `http://`.)

The example is one machine at `192.168.1.200` that runs the mail hub, with
its relay-only door on port 7378, and Hubchat Web on `127.0.0.1:8080` (as
under [Run it](#run-it)). Write your machine's address wherever
`192.168.1.200` stands.

**1. Make the authority and the machine's certificate.** With OpenSSL 3, in
an empty folder:

```
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
  -keyout ca-key.pem -out ca.crt -days 3650 -subj "/CN=My LAN authority" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign"

openssl req -new -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
  -keyout hub-key.pem -out hub.csr -subj "/CN=192.168.1.200"
printf 'subjectAltName=IP:192.168.1.200\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\n' > hub.ext
openssl x509 -req -in hub.csr -CA ca.crt -CAkey ca-key.pem -CAcreateserial \
  -days 800 -extfile hub.ext -out hub.pem
```

(In Git Bash on Windows, run `export MSYS_NO_PATHCONV=1` first, or the
`-subj` values are rewritten as file paths.)

- `ca.crt` is the authority's certificate, the file every device installs.
  It is no secret.
- `ca-key.pem` is the authority's key. Whoever holds it can make a
  certificate those devices accept for any address, so keep it off the
  server and out of reach. You need it again only for the next certificate.
- `hub.pem` and `hub-key.pem` are the machine's certificate and key, for
  the proxy. The certificate is good for the one address it names, so
  people have to use that address. It lasts 800 days (an iPhone accepts at
  most 825); before then, make a new one with the last three commands. The
  devices need nothing new for it.
- Make both: the authority, and a separate certificate for the machine.
  One self-signed certificate for everything, which is what a single
  `openssl req -x509` gives, is refused by the Hubchat apps even when the
  device trusts it.

**2. Put a proxy with that certificate in front of both.** For
[Caddy](https://caddyserver.com), a file named `Caddyfile`:

```
{
	default_sni 192.168.1.200
}

https://192.168.1.200 {
	tls /certs/hub.pem /certs/hub-key.pem
	reverse_proxy 127.0.0.1:8080
}

https://192.168.1.200:8443 {
	tls /certs/hub.pem /certs/hub-key.pem
	reverse_proxy 127.0.0.1:7378
}
```

The first block is Hubchat Web, at `https://192.168.1.200`. The second is
the hub, at `https://192.168.1.200:8443`: its relay-only door, never the
main port 7370, as above. On Linux, with `hub.pem` and `hub-key.pem` in a
folder `certs` beside that file:

```
docker run -d --name hubchat-lan-proxy --restart unless-stopped --network host \
  -v "$PWD/Caddyfile:/etc/caddy/Caddyfile:ro" -v "$PWD/certs:/certs:ro" caddy:2
```

`--network host` puts the proxy on the machine's own network, where it
reaches the two ports on `127.0.0.1`. Where Docker has no such network
(Docker Desktop on Windows or a Mac), run Caddy on the machine itself with
the same file and the certificate's real paths. `default_sni` tells Caddy
which certificate to show a caller that names no server, as every caller of
a bare address does. Caddy needs it whenever its own address is not the
certificate's, for one when Docker publishes its ports. Caddy also answers
on port 80, only to send people on to `https://`.

**3. Tell the hub and Hubchat Web.** In the hub's `.env`, followed by
`docker compose up -d`:

```
HUB_PUBLIC=1                              (turns the door on, port 7378)
HUB_BIND=127.0.0.1                        (the main port stays on this machine)
HUB_WEB_ORIGINS=https://192.168.1.200     (the page may call the hub)
```

And Hubchat Web, started with the hub's `https://` address as its
suggestion:

```
docker run -d --name hubchat-web --read-only --cap-drop ALL \
  -p 127.0.0.1:8080:8080 -e HUBCHAT_WEB_SUGGEST_HUBS=192.168.1.200:8443 \
  ghcr.io/maurdekye/hubchat-web:latest
```

The suggestion does two things. Someone new is offered the hub with one
tap, and a browser that is linked from an app finds the hub's `https://`
address through it (step 5).

The door stays open to the LAN at `http://192.168.1.200:7378` for the apps
that use it, and for Orgtree, whose own connection to a hub is not part of
this: leave it on `http://`. Once nothing uses the door that way,
`HUB_PUBLIC_BIND=127.0.0.1` leaves the proxy as the only way in.

**4. Install `ca.crt` on each device.** Hand the file out any way you like.

- **Windows:** open the file, **Install Certificate**, **Current User**,
  then **Place all certificates in the following store** and **Trusted Root
  Certification Authorities**. Chrome, Edge and the Hubchat app use it.
- **macOS:** open the file to add it to the keychain, open it there, and
  under **Trust** choose **Always Trust**.
- **Linux:** on Debian or Ubuntu, `sudo cp ca.crt
  /usr/local/share/ca-certificates/my-lan-authority.crt` and `sudo
  update-ca-certificates`. That serves the Hubchat app. A browser on Linux
  keeps a list of its own: import the file in its certificate settings.
- **Android:** in Settings, search for **CA certificate**, choose **Install
  anyway** and pick the file. Chrome and the Hubchat app use it. Android
  may then show a notice that the network may be monitored: it says only
  that an authority was added.
- **iPhone, iPad:** open the file and allow the profile, install it under
  **Settings › General › VPN & Device Management**, then turn it on under
  **Settings › General › About › Certificate Trust Settings**. Without that
  last switch Safari keeps refusing the page.

A Hubchat app that is running accepts the hub at its next attempt, with no
restart. Until the authority is installed it says "this device does not
trust the hub's certificate".

**5. Use it.**

- **Hubchat Web:** people open `https://192.168.1.200` and are offered the
  hub. On an iPhone, from the Home Screen, as above.
- **The apps** (1.3.8 or later): add the hub as
  `https://192.168.1.200:8443` under **Settings › Hubs › Add a hub**. An
  app that has the hub as `http://192.168.1.200:7378` can keep it.
- **Linking a browser from an app** ("I already use Hubchat", then the
  app's QR code or its code) works with either. When the app has the hub
  by its `http://` address, the browser looks for the same hub among the
  suggested ones (step 3) and takes the `https://` address it finds. That
  needs Hubchat Web 1.3.8 or later, whatever the app's version. A proxy
  that answers `https://` on the very port the apps use for `http://`
  needs no suggestion: the app and the browser both try `https://` at the
  same address and port.
- **Notifications** travel through the browser maker's push service, so
  the hub machine and the device need the internet for them. Everything
  else stays inside the LAN.

**What was tried.** The certificate commands and the Caddyfile were run as
written (with another address) in containers: Mail hub 2.5.1, Hubchat Web
1.3.7, Caddy 2.11 and OpenSSL 3.5, called from a second container that
trusted only that authority. The page, the hub, a request that waits 55
seconds and the apps' WebSocket stream all went through the proxy. The
apps' trust in a device's authority is tested on Windows, macOS, Linux and
an Android emulator, with the authority installed by command
(`docs/ci-release.md`). Not tried: Caddy outside a container, the install
steps of step 4 by hand, a browser with such an authority installed, and
an iPhone.

## Settings

All in the environment. A setting that makes no sense stops the server at
the start, with a line that names it.

| Variable | Default | Meaning |
|---|---|---|
| `HUBCHAT_WEB_PORT` | `8080` | the port |
| `HUBCHAT_WEB_BIND` | `0.0.0.0` | the interface, as an IP address |
| `HUBCHAT_WEB_TLS_CERT`, `HUBCHAT_WEB_TLS_KEY` | unset | PEM files (the certificate with its chain, and its key). With both, the server speaks HTTPS itself; with neither, plain HTTP for a proxy that adds HTTPS; with only one it refuses to start. The files are read at the start: restart the server after renewing them. |
| `HUBCHAT_WEB_SUGGEST_HUBS` | empty | the hubs offered to someone adding their first hub: a comma-separated list, each `https://host[:port]` or a bare `host[:port]` (taken as https). At most 8. `http://` is accepted only for `localhost`, `127.0.0.1` and `[::1]`. |
| `HUBCHAT_WEB_HUB_SUGGESTIONS` | `true` | `false`: no hub is suggested at all, whatever the list says. Only `true`, `false`, `1`, `0`. |
| `HUBCHAT_WEB_NAME` | empty | a label under the title on the welcome screen, for example the family's or the organisation's name. One line, at most 48 characters. |
| `HUBCHAT_WEB_LOG` | `requests` | `requests`: one JSON line per request on standard output (`t`, `path`, `status`, `ms`; no address, no query, no header). `off`: none. |

### Which hubs are suggested

Someone who has just made a NEW identity and has no hub yet is offered hubs
at "Add your hub". Nothing is ever added without a tap.

- `HUBCHAT_WEB_HUB_SUGGESTIONS=false`: nothing is offered and no address is
  checked. This wins over the list (the server says at the start that the
  list is ignored).
- A list in `HUBCHAT_WEB_SUGGEST_HUBS`: exactly those hubs, in that order,
  each as its own card. The server's own address is not added to them. A
  listed hub that does not answer is shown too, as "can't be reached right
  now", with **Try again**.
- Neither: the app tries the address it is served from (a hub behind the
  same address, then `https://` plus that host). If a hub answers there it
  is offered; otherwise nothing is.

The hub must allow the page (`HUB_WEB_ORIGINS`) for any of this to find it.

## What it answers

Only `GET` and `HEAD`:

| Path | Answer |
|---|---|
| `/`, `/index.html`, `/assets/*`, `/manifest.webmanifest`, `/sw.js`, `/icons/*` | the web app's files |
| `/config.json` | `{"name", "suggested_hubs", "hub_suggestions", "version"}` from the settings |
| `/healthz` | `{"ok": true, "app": "hubchat-web", "version": "<version>"}` |
| anything else | 404; any other method 405 |

Every answer carries a Content Security Policy (the page may load only its
own files and call only hubs over https, or on the same machine), `X-Content-Type-Options:
nosniff`, `Referrer-Policy: no-referrer`, `Cross-Origin-Opener-Policy:
same-origin` and `Permissions-Policy: camera=(self), microphone=(),
geolocation=()` (the camera is for scanning a link's QR code); with its own
HTTPS also `Strict-Transport-Security`. Files under `/assets/` have a hash
in their name and are kept by browsers for good; everything else is asked
for again each time (and answered `304` when unchanged). Files are stored
compressed (Brotli and gzip) inside the program.

All the app's addresses are relative, so it also works under a path behind
a proxy that strips the prefix (`https://example.org/chat/`).

## Updating

Pull the new image and start it again. People who have the app open are
shown "A new version is ready" with **Update**; the app keeps working on
the old files until they tap it. Their identity and messages are in their
browser and are not touched.

Run a Hubchat Web at least as new as the newest one your people have used.
If the server goes back to an older build, a browser that used the newer
one shows "Update the Hubchat Web server" and waits: it does not open, or
change, the data the newer build wrote. Nothing is lost; putting the newer
build back and reloading brings everything back.

## Commands

```
hubchat-web                 serve
hubchat-web healthcheck     exit 0 when the server on this machine answers /healthz
hubchat-web version         hubchat-web <version>
hubchat-web files           the files inside, with their sizes (as they are, Brotli, gzip)
```

## Building it yourself

1. Build the page: `npm ci && npm run build:web` (`docs/hubchat-web.md`,
   Building). It lands in `dist-web/`.
2. Build the server with the page inside:
   `HUBCHAT_WEB_DIST=$PWD/dist-web cargo build --release --locked -p hubchat-web-server`
   (for the image: `--target x86_64-unknown-linux-musl`, a static program).
   Without `HUBCHAT_WEB_DIST` a small stand-in page goes in, for the
   server's own tests; `hubchat-web version` then says so.
3. The image: `docker build -f crates/hubchat-web-server/Dockerfile -t
   hubchat-web <a folder holding the program as hubchat-web>`.

Tests: `cargo test -p hubchat-web-server` (its settings, every answer, and
the built program asked over real sockets, HTTPS included);
`.github/ci/web/smoke.sh binary|image <program or image> <version> <log
folder>`; and `tools/web-e2e.mjs --server <program>` opens the app from the
program in a browser.
