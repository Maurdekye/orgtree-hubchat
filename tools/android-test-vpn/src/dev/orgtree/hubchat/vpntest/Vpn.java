package dev.orgtree.hubchat.vpntest;

import android.content.Intent;
import android.net.VpnService;
import android.os.ParcelFileDescriptor;

/** Shaped like Tailscale's VPN: an address in 100.64.0.0/10 and a route for
 *  that range only, so other traffic keeps its usual way. Nothing is
 *  forwarded: whatever enters it goes nowhere. */
public class Vpn extends VpnService {
  private ParcelFileDescriptor tun;

  @Override
  public int onStartCommand(Intent intent, int flags, int id) {
    if (intent != null && "off".equals(intent.getAction())) {
      close();
      stopSelf();
      return START_NOT_STICKY;
    }
    if (tun == null) {
      try {
        tun = new Builder().setSession("Tailscale stand-in (Hubchat tests)").addAddress("100.90.1.2", 32).addRoute("100.64.0.0", 10).establish();
      } catch (Exception e) {
        stopSelf();
      }
    }
    return START_STICKY;
  }

  private void close() {
    try {
      if (tun != null) tun.close();
    } catch (Exception e) {
      // already closed
    }
    tun = null;
  }

  @Override
  public void onRevoke() {
    close();
    stopSelf();
  }

  @Override
  public void onDestroy() {
    close();
    super.onDestroy();
  }
}
