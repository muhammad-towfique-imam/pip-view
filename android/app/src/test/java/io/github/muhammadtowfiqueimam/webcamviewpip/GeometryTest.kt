package io.github.muhammadtowfiqueimam.webcamviewpip

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class GeometryTest {
    private fun close(a: Float, b: Float) = kotlin.math.abs(a - b) < 1e-5f

    @Test fun forwardDrag() {
        val r = NormRect.fromDrag(0.1f, 0.2f, 0.5f, 0.6f)
        assertTrue("$r", close(r.x, 0.1f) && close(r.y, 0.2f) && close(r.w, 0.4f) && close(r.h, 0.4f))
    }

    @Test fun reverseDragEqualsForward() {
        assertEquals(NormRect.fromDrag(0.1f, 0.2f, 0.5f, 0.6f), NormRect.fromDrag(0.5f, 0.6f, 0.1f, 0.2f))
    }

    @Test fun dragOutsideIsClamped() {
        assertEquals(NormRect(0f, 0.5f, 1f, 0.5f), NormRect.fromDrag(-0.3f, 0.5f, 1.4f, 2.0f))
    }

    @Test fun tapGetsMinSize() {
        val r = NormRect.fromDrag(0.5f, 0.5f, 0.5f, 0.5f)
        assertTrue(r.w >= NormRect.MIN_SIDE && r.h >= NormRect.MIN_SIDE)
    }

    @Test fun minSizeStaysInsideAtEdge() {
        val r = NormRect.fromDrag(1f, 1f, 1f, 1f)
        assertTrue("$r", r.x + r.w <= 1f + 1e-6f && r.y + r.h <= 1f + 1e-6f && r.w >= NormRect.MIN_SIDE)
    }

    @Test fun aspectUsesPixels() {
        assertTrue(close(NormRect(0f, 0f, 0.5f, 0.5f).aspect(1920, 1080), 16f / 9f))
    }

    @Test fun validRect() {
        assertTrue(NormRect.FULL.isValid())
        assertTrue(!NormRect(0.5f, 0f, 0.6f, 1f).isValid())
        assertTrue(!NormRect(0f, 0f, 0.001f, 1f).isValid())
        assertTrue(!NormRect(Float.NaN, 0f, 0.5f, 0.5f).isValid())
    }

    @Test fun fitWideContainerLetterboxesSides() {
        assertEquals(Box(50f, 0f, 100f, 100f), fitAspect(200f, 100f, 1f))
    }

    @Test fun fitTallContainerLetterboxesTop() {
        assertEquals(Box(0f, 50f, 100f, 100f), fitAspect(100f, 200f, 1f))
    }

    @Test fun pipSizeNormalArea() {
        assertEquals(Pair(320, 180), pipSize(16f / 9f, 320, 1080, 2000))
    }

    @Test fun pipSizeTallStripFitsScreen() {
        val (w, h) = pipSize(0.089f, 320, 1080, 2000)
        assertTrue("h = $h", h <= 2000)
        assertTrue(kotlin.math.abs(w.toFloat() / h - 0.089f) < 0.01f)
    }

    @Test fun pipSizeWideStripFitsNarrowScreen() {
        assertEquals(Pair(200, 10), pipSize(20f, 320, 200, 2000))
    }

    @Test fun innerBoxShowsOnlyTheArea() {
        // A window of 300x200 that shows the area (0.25, 0.5, 0.5, 0.25).
        val b = innerBox(NormRect(0.25f, 0.5f, 0.5f, 0.25f), 300, 200)
        assertEquals(Box(-150f, -400f, 600f, 800f), b)
    }

    @Test fun clampKeepsWindowOnScreen() {
        assertEquals(Pair(0, 0), clampPosition(-50, -10, 300, 200, 1080, 2000))
        assertEquals(Pair(780, 1800), clampPosition(5000, 5000, 300, 200, 1080, 2000))
        assertEquals(Pair(100, 100), clampPosition(100, 100, 300, 200, 1080, 2000))
    }

    @Test fun clampWindowLargerThanScreenGoesToZero() {
        assertEquals(Pair(0, 40), clampPosition(40, 40, 2000, 200, 1080, 2000))
    }

    @Test fun resizeKeepsAspectAndLimits() {
        assertEquals(Pair(400, 225), resizeKeepAspect(400, 16f / 9f, 100, 1080, 2000))
        assertEquals(Pair(100, 56), resizeKeepAspect(10, 16f / 9f, 100, 1080, 2000))
        assertEquals(Pair(1080, 608), resizeKeepAspect(5000, 16f / 9f, 100, 1080, 2000))
        // A tall area hits the screen height first.
        assertEquals(Pair(500, 2000), resizeKeepAspect(900, 0.25f, 100, 1080, 2000))
    }
}
