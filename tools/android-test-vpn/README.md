# Test VPN for the emulator

The Tailscale-off notice (Android) depends on whether Android reports a VPN.
Tailscale itself can't sign in on a test emulator, so the device tests use
this small stand-in, which is shaped like Tailscale's VPN: its own address
in 100.64.0.0/10 and a route for that range only. Nothing is forwarded, so a
hub at a 100.x address stays unreachable while it is on, which is the "VPN
on, hub down for another reason" case.

    source E:/hubchat-toolchain/env.sh      # or any JDK + Android SDK
    bash tools/android-test-vpn/build.sh <out dir>
    adb install -r <out dir>/hubchat-test-vpn.apk
    adb shell appops set dev.orgtree.hubchat.vpntest ACTIVATE_VPN allow
    adb shell am start -n dev.orgtree.hubchat.vpntest/.Main --es cmd on    # or off

`tools/e2e-android-tailscale.mjs` uses it.
