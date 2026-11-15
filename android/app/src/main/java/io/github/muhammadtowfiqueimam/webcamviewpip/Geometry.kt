package io.github.muhammadtowfiqueimam.webcamviewpip

import kotlin.math.roundToInt

/** A rectangle in normalized image coordinates (0..1 on each axis). */
data class NormRect(val x: Float, val y: Float, val w: Float, val h: Float) {
    /** Aspect ratio (width / height) of this area in image pixels. */
    fun aspect(imgW: Int, imgH: Int): Float = (w * imgW) / maxOf(h * imgH, Float.MIN_VALUE)

    /** True if the area is finite, inside the image, and not too small. */
    fun isValid(): Boolean {
        val eps = 1e-4f
        return listOf(x, y, w, h).all { it.isFinite() } &&
            x >= 0f && y >= 0f && w >= MIN_SIDE - eps && h >= MIN_SIDE - eps &&
            x + w <= 1f + eps && y + h <= 1f + eps
    }

    companion object {
        const val MIN_SIDE = 0.02f
        val FULL = NormRect(0f, 0f, 1f, 1f)

        /** The area between 2 drag points, clamped to the image, with a minimum side. */
        fun fromDrag(ax: Float, ay: Float, bx: Float, by: Float): NormRect {
            val (x, w) = axis(ax, bx)
            val (y, h) = axis(ay, by)
            return NormRect(x, y, w, h)
        }

        private fun axis(a: Float, b: Float): Pair<Float, Float> {
            val lo = minOf(a, b).coerceIn(0f, 1f)
            val hi = maxOf(a, b).coerceIn(0f, 1f)
            if (hi - lo >= MIN_SIDE) return Pair(lo, hi - lo)
            val start = ((lo + hi) / 2f - MIN_SIDE / 2f).coerceIn(0f, 1f - MIN_SIDE)
            return Pair(start, MIN_SIDE)
        }
    }
}

/** A rectangle in pixels. */
data class Box(val left: Float, val top: Float, val width: Float, val height: Float)

/** The largest box with the given aspect that fits in the container, centered. */
fun fitAspect(containerW: Float, containerH: Float, aspect: Float): Box {
    val (w, h) = if (containerW / containerH > aspect) {
        Pair(containerH * aspect, containerH)
    } else {
        Pair(containerW, containerW / aspect)
    }
    return Box((containerW - w) / 2f, (containerH - h) / 2f, w, h)
}

/** The first PiP size: [baseW] wide, made smaller if it does not fit the screen. */
fun pipSize(aspect: Float, baseW: Int, maxW: Int, maxH: Int): Pair<Int, Int> {
    val w = baseW.toFloat()
    val h = w / aspect
    val scale = minOf(maxW / w, maxH / h, 1f)
    return Pair((w * scale).roundToInt(), (h * scale).roundToInt())
}

/** The PiP size for a resize to [desiredW], with the aspect kept and the size limited. */
fun resizeKeepAspect(desiredW: Int, aspect: Float, minW: Int, maxW: Int, maxH: Int): Pair<Int, Int> {
    val w = desiredW.coerceIn(minW, maxOf(minW, maxW))
    val h = (w / aspect).roundToInt()
    if (h <= maxH) return Pair(w, h)
    return Pair((maxH * aspect).roundToInt(), maxH)
}

/**
 * The box of the full camera image inside a PiP window of winW x winH,
 * so that only [area] is visible in the window.
 */
fun innerBox(area: NormRect, winW: Int, winH: Int): Box {
    val width = winW / area.w
    val height = winH / area.h
    return Box(-area.x * width, -area.y * height, width, height)
}

/** The window position, moved so that the window stays fully on the screen. */
fun clampPosition(x: Int, y: Int, w: Int, h: Int, screenW: Int, screenH: Int): Pair<Int, Int> =
    Pair(x.coerceIn(0, maxOf(0, screenW - w)), y.coerceIn(0, maxOf(0, screenH - h)))

/** This area after a clockwise rotation of the image by [degrees] (a multiple of 90). */
fun NormRect.rotate(degrees: Int): NormRect = when (((degrees % 360) + 360) % 360) {
    90 -> NormRect(1f - y - h, x, h, w)
    180 -> NormRect(1f - x - w, 1f - y - h, w, h)
    270 -> NormRect(y, 1f - x - w, h, w)
    else -> this
}

/** This area after a horizontal mirror of the image. */
fun NormRect.flipX(): NormRect = NormRect(1f - x - w, y, w, h)

/**
 * The area as it shows on the screen. [degrees] is the clockwise rotation that
 * makes the camera buffer upright, and [mirror] is true for a front camera.
 */
fun bufferToDisplay(area: NormRect, degrees: Int, mirror: Boolean): NormRect {
    val r = area.rotate(degrees)
    return if (mirror) r.flipX() else r
}

/** The opposite of [bufferToDisplay]. */
fun displayToBuffer(area: NormRect, degrees: Int, mirror: Boolean): NormRect =
    (if (mirror) area.flipX() else area).rotate(-degrees)

/** The size of the camera image on the screen, after the rotation. */
fun displaySize(bufferW: Int, bufferH: Int, degrees: Int): Pair<Int, Int> =
    if (degrees % 180 == 0) Pair(bufferW, bufferH) else Pair(bufferH, bufferW)

/**
 * The window size for a new aspect. If the orientation (wide or tall) changes,
 * the old height becomes the new width, so the window turns with the image.
 */
fun fitWindowToAspect(w: Int, h: Int, aspect: Float, minW: Int, maxW: Int, maxH: Int): Pair<Int, Int> {
    val width = if ((aspect >= 1f) == (w >= h)) w else h
    return resizeKeepAspect(width, aspect, minW, maxW, maxH)
}
