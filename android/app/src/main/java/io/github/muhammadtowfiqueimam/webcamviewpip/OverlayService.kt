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
import android.hardware.display.DisplayManager
import android.os.Build
import android.provider.Settings
import android.view.ContextThemeWrapper
import android.view.Display
import android.view.Gravity
import android.view.Menu
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.PopupMenu
import android.widget.TextView
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.CameraState
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
    private var messageView: TextView? = null
    private var menu: PopupMenu? = null
    private var config: Config? = null
    private var provider: ProcessCameraProvider? = null
    private var preview: Preview? = null
    private var camera: Camera? = null
    private var rotation = -1

    /** Follows the screen rotation, also a 180 degree turn that gives no configuration change. */
    private val displayListener = object : DisplayManager.DisplayListener {
        override fun onDisplayAdded(displayId: Int) {}
        override fun onDisplayRemoved(displayId: Int) {}
        override fun onDisplayChanged(displayId: Int) {
            if (displayId == Display.DEFAULT_DISPLAY) onRotation()
        }
    }

    override fun onCreate() {
        super.onCreate()
        wm = getSystemService(WindowManager::class.java)
        getSystemService(DisplayManager::class.java).registerDisplayListener(displayListener, null)
    }

    @Suppress("DEPRECATION")
    private fun displayRotation(): Int =
        getSystemService(DisplayManager::class.java).getDisplay(Display.DEFAULT_DISPLAY)?.rotation ?: 0

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        startInForeground()
        when (intent?.action) {
            ACTION_EXIT -> {
                stopSelf()
                return START_NOT_STICKY
            }
        }
        val saved = Prefs.load(this)
        if (saved == null) {
            openSetup(SetupActivity.STEP_CAMERA)
            return START_NOT_STICKY
        }
        config = saved
        if (!showWindow(saved)) {
            openSetup(SetupActivity.STEP_AREA)
            return START_NOT_STICKY
        }
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
        // The setup screen stops this service, so that only 1 of them uses the camera.
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
    /** Adds the PiP window. Returns false if Android does not let the app show it. */
    private fun showWindow(c: Config): Boolean {
        removeWindow()
        if (!Settings.canDrawOverlays(this)) return false
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
        val pad = (8 * resources.displayMetrics.density).roundToInt()
        val message = TextView(this).apply {
            setTextColor(0xFFFFFFFF.toInt())
            gravity = Gravity.CENTER
            setPadding(pad, pad, pad, pad)
            visibility = View.GONE
        }
        val match = FrameLayout.LayoutParams.MATCH_PARENT
        val frame = FrameLayout(this).apply {
            setBackgroundColor(0xFF000000.toInt())
            clipChildren = true
            addView(pv)
            addView(message, FrameLayout.LayoutParams(match, match))
            addView(bv, FrameLayout.LayoutParams(match, match))
        }
        frame.setOnTouchListener(TouchHandler())
        frame.setOnClickListener { showMenu(it) }
        root = frame
        previewView = pv
        borderView = bv
        messageView = message
        params = lp
        layoutPreview()
        try {
            wm.addView(frame, lp)
        } catch (e: RuntimeException) {
            root = null
            return false
        }
        return true
    }

    /** The clockwise rotation from the camera buffer to the screen, or null before the camera is bound. */
    private fun degrees(): Int? = camera?.cameraInfo?.getSensorRotationDegrees(displayRotation())

    private fun mirror() = camera?.cameraInfo?.lensFacing == CameraSelector.LENS_FACING_FRONT

    /** Places the full camera image so that only the selected area is visible. */
    private fun layoutPreview() {
        val lp = params ?: return
        val c = config ?: return
        val deg = degrees()
        val pv = previewView ?: return
        // Before the camera is bound, the rotation is not known.
        pv.visibility = if (deg == null) View.INVISIBLE else View.VISIBLE
        if (deg == null) return
        val box = innerBox(bufferToDisplay(c.area, deg, mirror()), lp.width, lp.height)
        pv.layoutParams = FrameLayout.LayoutParams(box.width.roundToInt(), box.height.roundToInt()).apply {
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
            preview?.let { p.unbind(it) }
            val r = displayRotation()
            rotation = r
            val newPreview = Preview.Builder().setTargetRotation(r).build()
            newPreview.surfaceProvider = pv.surfaceProvider
            preview = newPreview
            try {
                val cam = p.bindToLifecycle(this, selectorFor(id), newPreview)
                camera = cam
                cam.cameraInfo.cameraState.observe(this) { showCameraState(it) }
                fitWindow()
            } catch (e: Exception) {
                openSetup(SetupActivity.STEP_CAMERA)
            }
        }, ContextCompat.getMainExecutor(this))
    }

    /** Shows a message on the window if the camera is not available. */
    private fun showCameraState(state: CameraState) {
        val view = messageView ?: return
        val error = state.error
        val text = when {
            error?.code == CameraState.ERROR_CAMERA_IN_USE || error?.code == CameraState.ERROR_MAX_CAMERAS_IN_USE ->
                getString(R.string.camera_in_use)
            error != null -> getString(R.string.camera_stopped)
            state.type == CameraState.Type.CLOSED -> getString(R.string.camera_stopped)
            else -> null
        }
        view.text = text
        view.visibility = if (text == null) View.GONE else View.VISIBLE
    }

    /** Sets the window aspect to the aspect of the area on the screen, for the current rotation. */
    private fun fitWindow() {
        val lp = params ?: return
        val c = config ?: return
        val view = root ?: return
        val deg = degrees() ?: return
        val (imgW, imgH) = displaySize(c.bufferW, c.bufferH, deg)
        val aspect = bufferToDisplay(c.area, deg, mirror()).aspect(imgW, imgH)
        val (screenW, screenH) = screenSize(this)
        val (w, h) = fitWindowToAspect(lp.width, lp.height, aspect, minWidth(), screenW, screenH)
        val (x, y) = clampPosition(lp.x, lp.y, w, h, screenW, screenH)
        lp.width = w; lp.height = h; lp.x = x; lp.y = y
        layoutPreview()
        wm.updateViewLayout(view, lp)
    }

    private fun onRotation() {
        val r = displayRotation()
        if (r == rotation) return
        rotation = r
        preview?.targetRotation = r
        fitWindow()
        saveGeometry()
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
        this.menu?.dismiss()
        val menu = PopupMenu(themed, anchor, Gravity.CENTER)
        this.menu = menu
        menu.setOnDismissListener { if (this.menu === it) this.menu = null }
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
        onRotation()
        // The screen size can change without a rotation, for example on a foldable.
        fitWindow()
    }

    private fun removeWindow() {
        menu?.dismiss()
        menu = null
        root?.let { wm.removeView(it) }
        root = null
        previewView = null
        borderView = null
        messageView = null
    }

    override fun onDestroy() {
        getSystemService(DisplayManager::class.java).unregisterDisplayListener(displayListener)
        // LifecycleService unbinds its own camera use cases. Do not unbind all: the setup screen can use the camera now.
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
                MotionEvent.ACTION_CANCEL -> if (moved) saveGeometry()
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
