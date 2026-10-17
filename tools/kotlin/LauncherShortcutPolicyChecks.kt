package dev.orgtree.hubchat

fun main() {
  val rows = (1..8).map { LauncherContact("peer$it.123456", "Contact $it") }
  check(launcherContacts(rows) == rows.take(4))
  check(launcherContacts(emptyList()).isEmpty())
  check(launcherContacts(listOf(rows[0], rows[0]) + rows.drop(1)) == rows.take(4))
  val clean = launcherContacts(listOf(LauncherContact("", "skip"), LauncherContact("p.123456", "\u0000")))
  check(clean == listOf(LauncherContact("p.123456", "p.123456")))
  check(launcherContacts(rows.reversed()).map {it.peer} == rows.reversed().take(4).map {it.peer})
  val pictured = LauncherContact("p.123456", "Person", "org", "normalized-png")
  check(launcherContacts(listOf(pictured)).single() == pictured)
  for (night in listOf(true, false)) {
    check(LauncherAvatarRecipe.colors("org", night) != LauncherAvatarRecipe.colors("person", night))
    check(LauncherAvatarRecipe.colors("unknown", night) == LauncherAvatarRecipe.colors("person", night))
  }
  check(LauncherAvatarRecipe.colors("person", true) != LauncherAvatarRecipe.colors("person", false))
  checkLauncherAvatarParity()
  println("PASS launcher policy8/8: last4, empty, dedupe, labels, order, avatar payload, kind palette, theme palette")
}
