package io.github.muhammadtowfiqueimam.webcamviewpip

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ConfigTest {
    private val sample = Config(
        cameraId = "1",
        area = NormRect(0.1f, 0.2f, 0.3f, 0.4f),
        windowX = 10, windowY = 20, windowW = 320, windowH = 180,
        border = Border.RED,
        bufferW = 640, bufferH = 480,
    )

    @Test fun roundTrip() { assertEquals(sample, Config.fromMap(sample.toMap())) }

    @Test fun emptyIsNull() { assertNull(Config.fromMap(emptyMap<String, Any?>())) }

    @Test fun missingKeyIsNull() { assertNull(Config.fromMap(sample.toMap() - "area_w")) }

    @Test fun missingBorderIsNone() {
        assertEquals(Border.NONE, Config.fromMap(sample.toMap() - "border")?.border)
    }

    @Test fun unknownBorderIsNone() {
        assertEquals(Border.NONE, Config.fromMap(sample.toMap() + ("border" to "Purple"))?.border)
    }

    @Test fun invalidAreaIsNull() {
        assertNull(Config.fromMap(sample.toMap() + ("area_w" to "5")))
        assertNull(Config.fromMap(sample.toMap() + ("area_x" to "abc")))
    }

    @Test fun invalidBufferSizeIsNull() {
        assertNull(Config.fromMap(sample.toMap() - "buffer_w"))
        assertNull(Config.fromMap(sample.toMap() + ("buffer_h" to "-1")))
    }

    @Test fun invalidWindowSizeIsNull() {
        assertNull(Config.fromMap(sample.toMap() + ("window_w" to "0")))
    }
}
