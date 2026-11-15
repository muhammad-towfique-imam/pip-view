package io.github.muhammadtowfiqueimam.webcamviewpip

/**
 * The saved settings. [area] is in camera buffer coordinates, before the
 * rotation to the screen. The buffer size is the camera image size in pixels.
 * The window values are in physical pixels.
 */
data class Config(
    val cameraId: String,
    val area: NormRect,
    val windowX: Int,
    val windowY: Int,
    val windowW: Int,
    val windowH: Int,
    val border: Border,
    val bufferW: Int,
    val bufferH: Int,
) {
    fun toMap(): Map<String, String> = mapOf(
        "camera_id" to cameraId,
        "area_x" to area.x.toString(),
        "area_y" to area.y.toString(),
        "area_w" to area.w.toString(),
        "area_h" to area.h.toString(),
        "window_x" to windowX.toString(),
        "window_y" to windowY.toString(),
        "window_w" to windowW.toString(),
        "window_h" to windowH.toString(),
        "border" to border.label,
        "buffer_w" to bufferW.toString(),
        "buffer_h" to bufferH.toString(),
    )

    companion object {
        /** The config in [map], or null if a value is missing or not valid. */
        fun fromMap(map: Map<String, *>): Config? {
            fun str(key: String) = map[key] as? String
            fun float(key: String) = str(key)?.toFloatOrNull()
            fun int(key: String) = str(key)?.toIntOrNull()

            val cameraId = str("camera_id") ?: return null
            val area = NormRect(
                float("area_x") ?: return null,
                float("area_y") ?: return null,
                float("area_w") ?: return null,
                float("area_h") ?: return null,
            )
            if (!area.isValid()) return null
            val w = int("window_w") ?: return null
            val h = int("window_h") ?: return null
            if (w <= 0 || h <= 0) return null
            val bufferW = int("buffer_w") ?: return null
            val bufferH = int("buffer_h") ?: return null
            if (bufferW <= 0 || bufferH <= 0) return null
            return Config(
                cameraId = cameraId,
                area = area,
                windowX = int("window_x") ?: return null,
                windowY = int("window_y") ?: return null,
                windowW = w,
                windowH = h,
                border = Border.parse(str("border")),
                bufferW = bufferW,
                bufferH = bufferH,
            )
        }
    }
}
