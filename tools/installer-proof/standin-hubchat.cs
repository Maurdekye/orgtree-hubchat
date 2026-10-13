// A stand-in for Hubchat's hubchat.exe, used ONLY to prove the Windows installer's
// Upgrade mechanics against a disposable product ("Hubchat Upgrade Test") without
// running the real app. It behaves like Hubchat where the installer cares:
//   * one instance per executable path (named mutex);
//   * `hubchat.exe --quit-for-upgrade` signals the running instance and exits;
//   * an invisible top-level window that WM_CLOSE reaches.
// The mode comes from standin-mode.txt next to the executable (default graceful):
//   graceful  quits on the quit request and on WM_CLOSE (exit-on-close Hubchat)
//   hide      quits on the quit request; WM_CLOSE only hides (tray Hubchat)
//   legacy    ignores the quit request (shows itself, like an old Hubchat); WM_CLOSE hides
//   stubborn  ignores everything (only a terminate ends it)
// Every start, close and quit is appended to %TEMP%\hubchat-standin.log.
//   csc /target:winexe /out:hubchat.exe /r:System.Windows.Forms.dll standin-hubchat.cs
using System;
using System.Diagnostics;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Windows.Forms;

class StandIn : Form {
  static string mode = "graceful";
  static string Log(string m) {
    try {
      File.AppendAllText(Path.Combine(Path.GetTempPath(), "hubchat-standin.log"),
        string.Format("{0:HH:mm:ss.fff} pid={1} {2}\r\n", DateTime.Now, Process.GetCurrentProcess().Id, m));
    } catch { }
    return m;
  }
  static string Key() {
    var exe = Application.ExecutablePath.ToLowerInvariant();
    using (var sha = SHA1.Create())
      return BitConverter.ToString(sha.ComputeHash(Encoding.UTF8.GetBytes(exe))).Replace("-", "").Substring(0, 16);
  }
  bool quitting;

  protected override void SetVisibleCore(bool value) { base.SetVisibleCore(false); }
  // WM_CLOSE handled directly: a form that was never shown does not run its
  // Closing events.
  protected override void WndProc(ref Message m) {
    if (m.Msg == 0x0010) {
      if (quitting || mode == "graceful") {
        Log("WM_CLOSE: quitting (mode " + mode + ")");
        quitting = true;
        Application.Exit();
      } else {
        Log("WM_CLOSE: ignored, stays running (mode " + mode + ")");
      }
      return;
    }
    base.WndProc(ref m);
  }

  [STAThread]
  static void Main(string[] args) {
    var dir = Path.GetDirectoryName(Application.ExecutablePath);
    var modeFile = Path.Combine(dir, "standin-mode.txt");
    if (File.Exists(modeFile)) mode = File.ReadAllText(modeFile).Trim();
    var evName = "Local\\HubchatStandIn-" + Key();
    bool quitArg = Array.IndexOf(args, "--quit-for-upgrade") >= 0;

    bool created;
    var mutex = new Mutex(true, "Local\\HubchatStandInMutex-" + Key(), out created);
    if (!created) {
      if (quitArg && mode != "legacy" && mode != "stubborn") {
        Log("second launch with --quit-for-upgrade: signalling the running instance");
        try { EventWaitHandle.OpenExisting(evName).Set(); } catch (Exception ex) { Log("signal failed: " + ex.Message); }
      } else {
        Log("second launch (" + string.Join(" ", args) + "): shows the running window (mode " + mode + ")");
      }
      return;
    }
    if (quitArg) { Log("started as --quit-for-upgrade with nothing running: exits"); return; }

    Log("started: " + Application.ExecutablePath + " mode=" + mode + " args=[" + string.Join(" ", args) + "]");
    var form = new StandIn();
    form.Text = "Hubchat stand-in";
    // (not ShowInTaskbar=false: that makes WinForms's hidden parking window the form's owner, and posting WM_CLOSE to it would destroy the form unseen)
    var handle = form.Handle; // create the (invisible) top-level window
    var ev = new EventWaitHandle(false, EventResetMode.AutoReset, evName);
    var t = new Thread(() => {
      while (true) {
        ev.WaitOne();
        if (mode == "legacy" || mode == "stubborn") { Log("quit request ignored (mode " + mode + ")"); continue; }
        Log("quit request: quitting");
        form.quitting = true;
        form.BeginInvoke((MethodInvoker)delegate { Application.Exit(); });
        return;
      }
    });
    t.IsBackground = true; t.Start();
    Application.Run(new ApplicationContext());
    Log("exited");
  }
}
