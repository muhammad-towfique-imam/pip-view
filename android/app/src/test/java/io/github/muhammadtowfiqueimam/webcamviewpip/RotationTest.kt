package io.github.muhammadtowfiqueimam.webcamviewpip

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RotationTest {
    private val a = NormRect(0.1f, 0.2f, 0.3f, 0.4f)

    private fun assertClose(expected: NormRect, actual: NormRect) {
        val ok = listOf(expected.x - actual.x, expected.y - actual.y, expected.w - actual.w, expected.h - actual.h)
            .all { kotlin.math.abs(it) < 1e-5f }
        assertTrue("expected $expected, was $actual", ok)
    }

    @Test fun rotate0IsSame() = assertClose(a, a.rotate(0))
    @Test fun rotate90() = assertClose(NormRect(0.4f, 0.1f, 0.4f, 0.3f), a.rotate(90))
    @Test fun rotate180() = assertClose(NormRect(0.6f, 0.4f, 0.3f, 0.4f), a.rotate(180))
    @Test fun rotate270() = assertClose(NormRect(0.2f, 0.6f, 0.4f, 0.3f), a.rotate(270))
    @Test fun rotateNegativeIsSameAsPositive() = assertClose(a.rotate(270), a.rotate(-90))
    @Test fun flipX() = assertClose(NormRect(0.6f, 0.2f, 0.3f, 0.4f), a.flipX())

    @Test fun displayAndBufferRoundTrip() {
        for (deg in listOf(0, 90, 180, 270)) for (mirror in listOf(false, true)) {
            assertClose(a, displayToBuffer(bufferToDisplay(a, deg, mirror), deg, mirror))
            assertClose(a, bufferToDisplay(displayToBuffer(a, deg, mirror), deg, mirror))
        }
    }

    @Test fun displaySizeSwapsAt90() {
        assertEquals(Pair(640, 480), displaySize(640, 480, 0))
        assertEquals(Pair(480, 640), displaySize(640, 480, 90))
        assertEquals(Pair(480, 640), displaySize(640, 480, 270))
    }

    @Test fun fitWindowSameOrientationKeepsWidth() {
        assertEquals(Pair(400, 300), fitWindowToAspect(400, 299, 4f / 3f, 100, 1080, 2000))
    }

    @Test fun fitWindowFlippedOrientationUsesHeightAsWidth() {
        assertEquals(Pair(300, 400), fitWindowToAspect(400, 300, 0.75f, 100, 1080, 2000))
    }
}
