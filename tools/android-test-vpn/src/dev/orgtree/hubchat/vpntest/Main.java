package dev.orgtree.hubchat.vpntest;

import android.app.Activity;
import android.content.Intent;
import android.net.VpnService;
import android.os.Bundle;

/** Turns the test VPN on or off, then closes:
 *  am start -n dev.orgtree.hubchat.vpntest/.Main --es cmd on|off
 *  (Android asks once to allow a VPN; `appops set <pkg> ACTIVATE_VPN allow` skips that). */
public class Main extends Activity {
  @Override
  protected void onCreate(Bundle state) {
    super.onCreate(state);
    if ("off".equals(getIntent().getStringExtra("cmd"))) {
      startService(new Intent(this, Vpn.class).setAction("off"));
      finish();
      return;
    }
    Intent ask = VpnService.prepare(this);
    if (ask != null) startActivityForResult(ask, 1);
    else onActivityResult(1, RESULT_OK, null);
  }

  @Override
  protected void onActivityResult(int request, int result, Intent data) {
    if (result == RESULT_OK) startService(new Intent(this, Vpn.class).setAction("on"));
    finish();
  }
}
