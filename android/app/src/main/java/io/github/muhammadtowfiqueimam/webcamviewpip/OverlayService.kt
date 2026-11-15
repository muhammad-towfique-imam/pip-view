package io.github.muhammadtowfiqueimam.webcamviewpip

import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.content.res.Configuration
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.PixelFormat
import android.os.Build
import android.view.ContextThemeWrapper
import android.view.Gravity
import android.view.Menu
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.PopupMenu
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleService
import kotlin.math.abs
import kotlin.math.roundToInt

/** Shows the camera area in a floating window on top of other apps. */
class OverlayService : LifecycleService() {
    private lateinit var wm: WindowManager
    private var root: FrameLayout? = null
    private var previewView: PreviewView? = null
    private var borderView: BorderView? = null
    private var params: WindowManager.LayoutParams? = null
    private var config: Config? = null
    private var provider: ProcessCameraProvider? = null

    override fun onCreate() {
        super.onCreate()
        wm = getSystemService(WindowManager::class.java)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        startInForeground()
        if (intent?.action == ACTION_EXIT) {
            stopSelf()
            return START_NOT_STICKY
        }
        val saved = Prefs.load(this)
        if (saved == null) {
            openSetup(SetupActivity.STEP_CAMERA)
            return START_NOT_STICKY
        }
        config = saved
        showWindow(saved)
        bindCamera(saved.cameraId)
        return START_NOT_STICKY
    }

    private fun startInForeground() {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.channel_name), NotificationManager.IMPORTANCE_LOW),
        )
        val flags = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        val exit = PendingIntent.getService(
            this, 1, Intent(this, OverlayService::class.java).setAction(ACTION_EXIT), flags,
        )
        val open = PendingIntent.getActivity(
            this, 2, SetupActivity.intent(this, SetupActivity.STEP_AREA), flags,
        )
        val notification: Notification = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(getString(R.string.notification_title))
            .setContentText(getString(R.string.notification_text))
            .setContentIntent(open)
            .addAction(0, getString(R.string.exit), exit)
            .setOngoing(true)
            .build()
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA else 0
        ServiceCompat.startForeground(this, NOTIFICATION_ID, notification, type)
    }

    @SuppressLint("ClickableViewAccessibility") // The touch handler calls performClick for a tap.
    private fun showWindow(c: Config) {
        removeWindow()
        val (screenW, screenH) = screenSize(this)
        val (w, h) = resizeKeepAspect(c.windowW, c.windowW.toFloat() / c.windowH, minWidth(), screenW, screenH)
        val (x, y) = clampPosition(c.windowX, c.windowY, w, h, screenW, screenH)
        val lp = WindowManager.LayoutParams(
            w, h,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.TOP or Gravity.START
            this.x = x
            this.y = y
        }
        val pv = PreviewView(this).apply {
            implementationMode = PreviewView.ImplementationMode.COMPATIBLE
            scaleType = PreviewView.ScaleType.FILL_CENTER
        }
        val bv = BorderView(this).apply { border = c.border }
        val frame = FrameLayout(this).apply {
            setBackgroundColor(0xFF000000.toInt())
            clipChildren = true
            addView(pv)
            addView(bv, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        }
        frame.setOnTouchListener(TouchHandler())
        frame.setOnClickListener { showMenu(it) }
        root = frame
        previewView = pv
        borderView = bv
        params = lp
        layoutPreview()
        wm.addView(frame, lp)
    }

    /** Places the full camera image so that only the selected area is visible. */
    private fun layoutPreview() {
        val lp = params ?: return
        val c = config ?: return
        val box = innerBox(c.area, lp.width, lp.height)
        previewView?.layoutParams = FrameLayout.LayoutParams(box.width.roundToInt(), box.height.roundToInt()).apply {
            leftMargin = box.left.roundToInt()
            topMargin = box.top.roundToInt()
        }
    }

    private fun bindCamera(id: String) {
        val future = ProcessCameraProvider.getInstance(this)
        future.addListener({
            val p = future.get()
            provider = p
            val pv = previewView ?: return@addListener
            if (p.availableCameraInfos.none { cameraId(it) == id }) {
                openSetup(SetupActivity.STEP_CAMERA)
                return@addListener
            }
            val preview = Preview.Builder().build()
            preview.surfaceProvider = pv.surfaceProvider
            try {
                p.unbindAll()
                p.bindToLifecycle(this, selectorFor(id), preview)
            } catch (e: Exception) {
                openSetup(SetupActivity.STEP_CAMERA)
            }
        }, ContextCompat.getMainExecutor(this))
    }

    private fun minWidth() = (MIN_WIDTH_DP * resources.displayMetrics.density).roundToInt()

    private fun saveGeometry() {
        val lp = params ?: return
        val c = config ?: return
        val updated = c.copy(windowX = lp.x, windowY = lp.y, windowW = lp.width, windowH = lp.height)
        config = updated
        Prefs.save(this, updated)
    }

    private fun showMenu(anchor: View) {
        val themed = ContextThemeWrapper(this, android.R.style.Theme_DeviceDefault)
        val menu = PopupMenu(themed, anchor, Gravity.CENTER)
        menu.menu.add(Menu.NONE, MENU_AREA, 1, R.string.select_area)
        menu.menu.add(Menu.NONE, MENU_CAMERA, 2, R.string.select_camera)
        val sub = menu.menu.addSubMenu(Menu.NONE, Menu.NONE, 3, R.string.border)
        Border.entries.forEachIndexed { i, b ->
            sub.add(GROUP_BORDER, MENU_BORDER_FIRST + i, i, b.label).isChecked = b == config?.border
        }
        sub.setGroupCheckable(GROUP_BORDER, true, true)
        menu.menu.add(Menu.NONE, MENU_EXIT, 4, R.string.exit)
        menu.setOnMenuItemClickListener { item ->
            when (item.itemId) {
                MENU_AREA -> openSetup(SetupActivity.STEP_AREA)
                MENU_CAMERA -> openSetup(SetupActivity.STEP_CAMERA)
                MENU_EXIT -> stopSelf()
                in MENU_BORDER_FIRST until MENU_BORDER_FIRST + Border.entries.size -> {
                    val b = Border.entries[item.itemId - MENU_BORDER_FIRST]
                    config = config?.copy(border = b)
                    borderView?.border = b
                    saveGeometry()
                }
                else -> return@setOnMenuItemClickListener false
            }
            true
        }
        menu.show()
    }

    private fun openSetup(step: String) {
        startActivity(SetupActivity.intent(this, step))
        stopSelf()
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        val lp = params ?: return
        val view = root ?: return
        val (screenW, screenH) = screenSize(this)
        val (w, h) = resizeKeepAspect(lp.width, lp.width.toFloat() / lp.height, minWidth(), screenW, screenH)
        val (x, y) = clampPosition(lp.x, lp.y, w, h, screenW, screenH)
        lp.width = w; lp.height = h; lp.x = x; lp.y = y
        layoutPreview()
        wm.updateViewLayout(view, lp)
    }

    private fun removeWindow() {
        root?.let { wm.removeView(it) }
        root = null
        previewView = null
        borderView = null
    }

    override fun onDestroy() {
        provider?.unbindAll()
        removeWindow()
        super.onDestroy()
    }

    /** Drag moves the window, drag on the bottom-right corner resizes it, and a tap opens the menu. */
    private inner class TouchHandler : View.OnTouchListener {
        private val slop = ViewConfiguration.get(this@OverlayService).scaledTouchSlop
        private val grip = GRIP_DP * resources.displayMetrics.density
        private var downX = 0f
        private var downY = 0f
        private var startX = 0
        private var startY = 0
        private var startW = 0
        private var startH = 0
        private var resizing = false
        private var moved = false

        @SuppressLint("ClickableViewAccessibility")
        override fun onTouch(v: View, event: MotionEvent): Boolean {
            val lp = params ?: return false
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    downX = event.rawX; downY = event.rawY
                    startX = lp.x; startY = lp.y; startW = lp.width; startH = lp.height
                    resizing = event.x > v.width - grip && event.y > v.height - grip
                    moved = false
                }
                MotionEvent.ACTION_MOVE -> {
                    val dx = event.rawX - downX
                    val dy = event.rawY - downY
                    if (!moved && abs(dx) < slop && abs(dy) < slop) return true
                    moved = true
                    val (screenW, screenH) = screenSize(this@OverlayService)
                    if (resizing) {
                        val (w, h) = resizeKeepAspect(
                            (startW + dx).roundToInt(), startW.toFloat() / startH, minWidth(),
                            screenW - startX, screenH - startY,
                        )
                        lp.width = w; lp.height = h
                        layoutPreview()
                    } else {
                        val (x, y) = clampPosition(
                            (startX + dx).roundToInt(), (startY + dy).roundToInt(), lp.width, lp.height, screenW, screenH,
                        )
                        lp.x = x; lp.y = y
                    }
                    wm.updateViewLayout(v, lp)
                }
                MotionEvent.ACTION_UP -> if (moved) saveGeometry() else v.performClick()
            }
            return true
        }
    }

    /** Draws the colored border and the resize grip mark on top of the camera image. */
    private inner class BorderView(context: Context) : View(context) {
        var border: Border = Border.NONE
            set(value) { field = value; invalidate() }
        private val density = resources.displayMetrics.density
        private val lineWidth = Border.WIDTH_DP * density
        private val line = Paint().apply { style = Paint.Style.STROKE; strokeWidth = lineWidth }
        private val gripPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = 0xB3FFFFFF.toInt()
            strokeWidth = 1.5f * density
            setShadowLayer(1.5f * density, 0f, 0f, 0xFF000000.toInt())
        }

        override fun onDraw(canvas: Canvas) {
            border.argb?.let {
                line.color = it
                val half = lineWidth / 2f
                canvas.drawRect(half, half, width - half, height - half, line)
            }
            // 2 short diagonal lines in the bottom-right corner show the resize grip.
            val inset = lineWidth + 3f * density
            val r = width - inset
            val b = height - inset
            for (step in listOf(6f, 11f)) {
                val d = step * density
                canvas.drawLine(r - d, b, r, b - d, gripPaint)
            }
        }
    }

    companion object {
        const val ACTION_EXIT = "exit"
        private const val CHANNEL = "pip"
        private const val NOTIFICATION_ID = 1
        private const val MIN_WIDTH_DP = 64
        private const val GRIP_DP = 32
        private const val MENU_AREA = 1
        private const val MENU_CAMERA = 2
        private const val MENU_EXIT = 3
        private const val MENU_BORDER_FIRST = 100
        private const val GROUP_BORDER = 10
    }
}
