package dev.orgtree.hubchat

import org.junit.Assert.assertEquals
import org.junit.Test

/** Stay connected: only a service Android accepted replaces the periodic
 *  check; a refused one leaves it running (push recheck, 2026-10-10). */
class StartOrKeepChecksTest {
  private val calls = mutableListOf<String>()

  private fun run(start: () -> Unit) = startOrKeepChecks(
    start = { calls += "start"; start() },
    cancelChecks = { calls += "cancel" },
    keepChecks = { calls += "keep" },
  )

  @Test fun acceptedServiceReplacesTheCheck() {
    run {}
    assertEquals(listOf("start", "cancel"), calls)
  }

  @Test fun refusedInTheBackgroundKeepsTheCheck() {
    // Android 12+: ForegroundServiceStartNotAllowedException is one of these
    run { throw IllegalStateException("startForegroundService() not allowed") }
    assertEquals(listOf("start", "keep"), calls)
  }

  @Test fun refusedForAPermissionKeepsTheCheck() {
    run { throw SecurityException("missing foreground service permission") }
    assertEquals(listOf("start", "keep"), calls)
  }

  @Test fun anyOtherFailureStillSurfaces() {
    // only Android's refusals are expected here; anything else is a bug to see
    val thrown = runCatching { run { throw IllegalArgumentException("bug") } }.exceptionOrNull()
    assertEquals(IllegalArgumentException::class.java, thrown?.javaClass)
    assertEquals(listOf("start"), calls)
  }
}
