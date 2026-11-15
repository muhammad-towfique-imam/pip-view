package io.github.muhammadtowfiqueimam.webcamviewpip

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class BorderTest {
    @Test fun allInMenuOrder() {
        assertEquals(listOf("None", "White", "Green", "Red", "Blue", "Yellow"), Border.entries.map { it.label })
    }

    @Test fun noneHasNoColor() { assertNull(Border.NONE.argb) }

    @Test fun colors() {
        assertEquals(0xFFFFFFFF.toInt(), Border.WHITE.argb)
        assertEquals(0xFF00FF00.toInt(), Border.GREEN.argb)
        assertEquals(0xFFFF0000.toInt(), Border.RED.argb)
        assertEquals(0xFF0000FF.toInt(), Border.BLUE.argb)
        assertEquals(0xFFFFFF00.toInt(), Border.YELLOW.argb)
    }

    @Test fun parseUnknownIsNone() {
        assertEquals(Border.RED, Border.parse("Red"))
        assertEquals(Border.NONE, Border.parse("Purple"))
        assertEquals(Border.NONE, Border.parse(null))
    }
}
