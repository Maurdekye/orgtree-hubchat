# A Tailscale stand-in for the emulator

Hubchat's Android tests need Tailscale on the emulator for two things: the
Tailscale-off notice, which depends on whether Android reports a VPN, and
Scan setup code, whose Tailscale check looks for the Tailscale app. Real
Tailscale can't sign in on a test emulator, so the tests use this small
stand-in instead. It takes Tailscale's package name (`com.tailscale.ipn`),
so Hubchat finds it installed and Open Tailscale opens it, and opening it
turns on a VPN shaped like Tailscale's: its own address in 100.64.0.0/10 and
a route for that range only. Nothing is forwarded, so a hub at a 100.x
address stays unreachable while it is on, which is the "VPN on, hub down
for another reason" case.

**Emulator only.** Never install it on a real phone: it would stand in the
way of the real Tailscale.

    source E:/hubchat-toolchain/env.sh      # or any JDK + Android SDK
    bash tools/android-test-vpn/build.sh <out dir>
    adb install -r <out dir>/hubchat-test-vpn.apk
    adb shell appops set com.tailscale.ipn ACTIVATE_VPN allow
    adb shell am start -n com.tailscale.ipn/dev.orgtree.hubchat.vpntest.Main --es cmd on    # or off

`tools/e2e-android-tailscale.mjs` uses it.
