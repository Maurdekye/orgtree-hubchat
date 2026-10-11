package dev.orgtree.hubchat

/** Main-thread policy. A refused start is retried only after another foreground visit. */
internal class FileServicePolicy {
  var wanted = false
  var starting = false
  var blocked = false
  fun foreground() { blocked = false }
  fun shouldStart(visible: Boolean, connection: Boolean, running: Boolean): Boolean =
    wanted && visible && !connection && !running && !starting && !blocked
  fun refused() { starting = false; blocked = true }
}
