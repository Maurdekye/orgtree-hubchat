package dev.orgtree.hubchat

import org.junit.Assert.*
import org.junit.Test

class FileServicePolicyTest {
  @Test fun admissionRequiresVisibilityAndDoesNotDuplicateConnectionCoverage() {
    val p = FileServicePolicy()
    assertFalse(p.shouldStart(true, false, false))
    p.wanted = true
    assertFalse(p.shouldStart(false, false, false))
    assertFalse(p.shouldStart(true, true, false))
    assertFalse(p.shouldStart(true, false, true))
    assertTrue(p.shouldStart(true, false, false))
    p.starting = true
    assertFalse(p.shouldStart(true, false, false))
  }
  @Test fun refusalOrTimeoutCannotLoopOnWorkerEventsOrBackgroundWakes() {
    val p = FileServicePolicy()
    p.wanted = true
    p.starting = true
    p.refused()
    repeat(10) { assertFalse(p.shouldStart(true, false, false)) }
    p.wanted = false
    p.wanted = true
    assertFalse(p.shouldStart(true, false, false))
    p.foreground()
    assertFalse(p.shouldStart(false, false, false))
    assertTrue(p.shouldStart(true, false, false))
  }
}
