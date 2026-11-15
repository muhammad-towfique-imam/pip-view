package io.github.muhammadtowfiqueimam.webcamviewpip

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.view.MotionEvent
import android.view.View

/**
 * A transparent view on top of the camera preview. The user drags on it to
 * select the area. The camera image fills the view with FIT_CENTER.
 */
class AreaSelectView(context: Context) : View(context) {
    /** Aspect ratio (width / height) of the camera image. Null until it is known. */
    var imageAspect: Float? = null
        set(value) { field = value; invalidate() }

    var area: NormRect = NormRect.FULL
        set(value) { field = value; invalidate() }

    private var start: Pair<Float, Float>? = null
    private val density = resources.displayMetrics.density
    private val dim = Paint().apply { color = Color.argb(140, 0, 0, 0) }
    private val line = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 2f * density
        color = Color.WHITE
    }
    private val path = Path()

    private fun imageBox(): Box? = imageAspect?.let {
        if (width == 0 || height == 0) null else fitAspect(width.toFloat(), height.toFloat(), it)
    }

    private fun normalize(box: Box, x: Float, y: Float) =
        Pair((x - box.left) / box.width, (y - box.top) / box.height)

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(event: MotionEvent): Boolean {
        val box = imageBox() ?: return false
        val (nx, ny) = normalize(box, event.x, event.y)
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                parent.requestDisallowInterceptTouchEvent(true)
                start = Pair(nx, ny)
            }
            MotionEvent.ACTION_MOVE, MotionEvent.ACTION_UP -> start?.let { (sx, sy) ->
                area = NormRect.fromDrag(sx, sy, nx, ny)
                if (event.actionMasked == MotionEvent.ACTION_UP) start = null
            }
            MotionEvent.ACTION_CANCEL -> start = null
        }
        return true
    }

    override fun onDraw(canvas: Canvas) {
        val box = imageBox() ?: return
        val l = box.left + area.x * box.width
        val t = box.top + area.y * box.height
        val r = l + area.w * box.width
        val b = t + area.h * box.height
        path.reset()
        path.fillType = Path.FillType.EVEN_ODD
        path.addRect(box.left, box.top, box.left + box.width, box.top + box.height, Path.Direction.CW)
        path.addRect(l, t, r, b, Path.Direction.CW)
        canvas.drawPath(path, dim)
        canvas.drawRect(l, t, r, b, line)
    }
}
