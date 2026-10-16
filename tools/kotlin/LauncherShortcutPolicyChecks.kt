package dev.orgtree.hubchat

fun main() {
  val rows = (1..8).map { LauncherContact("peer$it.123456", "Contact $it") }
  check(launcherContacts(rows) == rows.take(4))
  check(launcherContacts(emptyList()).isEmpty())
  check(launcherContacts(listOf(rows[0], rows[0]) + rows.drop(1)) == rows.take(4))
  val clean = launcherContacts(listOf(LauncherContact("", "skip"), LauncherContact("p.123456", "\u0000")))
  check(clean == listOf(LauncherContact("p.123456", "p.123456")))
  check(launcherContacts(rows.reversed()).map {it.peer} == rows.reversed().take(4).map {it.peer})
  println("PASS launcher policy 5/5: four contacts, fewer/empty, duplicates, labels, supplied message order")
}
