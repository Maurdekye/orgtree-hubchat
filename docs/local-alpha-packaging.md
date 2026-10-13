# Local Windows alpha packaging

The first local 1.2.0 prerelease used a forward-slash absolute Windows path
for `build.frontendDist`. Tauri's pinned `tauri-utils` 2.10.1 deserializes
`FrontendDist` as an untagged URL first, then a directory. `C:/...` is a
valid `c:` URL: it does not embed the frontend. The installed app displayed
a local directory listing. Checking native versions and installer payload
hashes alone did not detect that packaging error.

Build frontend assets into the designated output directory, then generate
the local override using `scripts/local-tauri-config.mjs`. It uses the
Windows verbatim directory form, refuses URL/relative input, and checks the
actual index and referenced entry assets. The production source config
keeps its ordinary relative `../dist` for CI. No dependencies or runtime
behavior change for this correction.

Before delivery, run `frontend-dist-parser-probe.rs` against the actual
locked Tauri dependencies and effective override. Its negative control
must classify the old `C:/...` as a URL; the corrected config must be a
Directory with index/assets. Also verify actual rendered startup in an
isolated `.test` identity with disposable app/WebView data and no hubs or
credentials. That smoke configuration differs from production in identity
and disposable paths; record that limit and separately check production
config and embedded assets. Do not start the installed app or rely on
APPDATA environment spoofing as isolation. Keep the faulty installer and
its original evidence; deliver the replacement with a new prerelease.

Parser source: <https://docs.rs/tauri-utils/2.10.1/src/tauri_utils/config.rs.html>.
