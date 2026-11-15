package io.github.muhammadtowfiqueimam.webcamviewpip

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.RadioButton
import android.widget.RadioGroup
import android.widget.ScrollView
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import kotlin.math.roundToInt

/** The camera select and area select screens. */
class SetupActivity : ComponentActivity() {
    private enum class Step { CAMERA, AREA }

    private var provider: ProcessCameraProvider? = null
    private var cameras: List<CameraEntry> = emptyList()
    private var cameraId: String? = null
    /** The selected area in camera buffer coordinates. */
    private var area: NormRect = NormRect.FULL
    /** The camera buffer size in pixels, known after the preview starts. */
    private var bufferSize: Pair<Int, Int>? = null
    /** The clockwise rotation from the camera buffer to the screen. */
    private var degrees = 0
    /** True for a front camera: the preview shows a mirror image. */
    private var mirror = false
    private var preview: Preview? = null
    private var selectView: AreaSelectView? = null
    private var step = Step.CAMERA
    /** Open the PiP window at once if the saved settings are complete. */
    private var autoStart = false
    /** The user went to the app settings to give the camera permission. */
    private var waitingForSettings = false

    private val askPermissions = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) { onPermissions() }

    private val askOverlay = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { if (Settings.canDrawOverlays(this)) confirm() }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        stopPipIfAsked(intent)
        val saved = Prefs.load(this)
        cameraId = saved?.cameraId
        area = saved?.area ?: NormRect.FULL
        step = if (intent.getStringExtra(EXTRA_STEP) == STEP_AREA && cameraId != null) Step.AREA else Step.CAMERA
        autoStart = savedInstanceState == null && intent.getStringExtra(EXTRA_STEP) == null && saved != null
        showMessage(getString(R.string.loading), null)

        val perms = mutableListOf(Manifest.permission.CAMERA)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) perms += Manifest.permission.POST_NOTIFICATIONS
        askPermissions.launch(perms.toTypedArray())
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        stopPipIfAsked(intent)
        autoStart = false
        if (provider == null) return
        if (intent.getStringExtra(EXTRA_STEP) == STEP_AREA && cameraId != null) showArea() else showCameras()
    }

    /** The PiP window and the setup screen must not use the camera at the same time. */
    private fun stopPipIfAsked(intent: Intent) {
        if (intent.getStringExtra(EXTRA_STEP) != null) stopService(Intent(this, OverlayService::class.java))
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        // The rotation changes the image on the screen. Keep the area and start the preview again.
        if (step == Step.AREA && provider != null) {
            selectView?.let { area = displayToBuffer(it.area, degrees, mirror) }
            showArea()
        }
    }

    private fun stopPreview() {
        preview?.let { provider?.unbind(it) }
        preview = null
        selectView = null
    }

    @Suppress("DEPRECATION")
    private fun displayRotation(): Int =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) display?.rotation ?: 0 else windowManager.defaultDisplay.rotation

    private fun hasCamera() =
        ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED

    private fun onPermissions() {
        if (!hasCamera()) {
            waitingForSettings = true
            showMessage(getString(R.string.no_camera_permission), getString(R.string.open_settings)) {
                startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
            }
            return
        }
        val future = ProcessCameraProvider.getInstance(this)
        future.addListener({
            val p = future.get()
            provider = p
            cameras = listCameras(this, p)
            if (cameras.isEmpty()) {
                showMessage(getString(R.string.no_cameras), null)
                return@addListener
            }
            val known = cameras.any { it.id == cameraId }
            if (!known) {
                step = Step.CAMERA
                autoStart = false
            }
            if (autoStart && Settings.canDrawOverlays(this)) {
                startPip()
                return@addListener
            }
            autoStart = false
            if (step == Step.AREA) showArea() else showCameras()
        }, ContextCompat.getMainExecutor(this))
    }

    override fun onResume() {
        super.onResume()
        // The user can come back from the app settings with the camera permission.
        if (waitingForSettings && hasCamera()) {
            waitingForSettings = false
            onPermissions()
        }
    }

    private fun setScreen(view: View) {
        // Add the system bar sizes to the padding of the view.
        val base = intArrayOf(view.paddingLeft, view.paddingTop, view.paddingRight, view.paddingBottom)
        ViewCompat.setOnApplyWindowInsetsListener(view) { v, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            v.setPadding(base[0] + bars.left, base[1] + bars.top, base[2] + bars.right, base[3] + bars.bottom)
            insets
        }
        setContentView(view)
    }

    private fun dp(value: Int) =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value.toFloat(), resources.displayMetrics).roundToInt()

    private fun showMessage(text: String, button: String?, action: (() -> Unit)? = null) {
        val layout = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            setPadding(dp(24), dp(24), dp(24), dp(24))
        }
        layout.addView(TextView(this).apply { this.text = text; textSize = 18f; gravity = Gravity.CENTER })
        if (button != null && action != null) {
            layout.addView(Button(this).apply { this.text = button; setOnClickListener { action() } })
        }
        setScreen(layout)
    }

    private fun showCameras() {
        step = Step.CAMERA
        stopPreview()
        val layout = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(24), dp(24), dp(24), dp(24))
        }
        layout.addView(TextView(this).apply { text = getString(R.string.select_camera); textSize = 22f })
        val group = RadioGroup(this)
        cameras.forEachIndexed { i, cam ->
            group.addView(RadioButton(this).apply {
                id = View.generateViewId()
                text = cam.label
                textSize = 18f
                tag = i
                isChecked = cam.id == cameraId || (cameraId == null && i == 0)
            })
        }
        layout.addView(ScrollView(this).apply { addView(group) }, LinearLayout.LayoutParams(MATCH_PARENT, 0, 1f))
        layout.addView(Button(this).apply {
            text = getString(R.string.next)
            setOnClickListener {
                val checked = group.findViewById<RadioButton>(group.checkedRadioButtonId) ?: return@setOnClickListener
                val chosen = cameras[checked.tag as Int].id
                if (chosen != cameraId) area = NormRect.FULL
                cameraId = chosen
                showArea()
            }
        }, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        setScreen(layout)
    }

    private fun showArea() {
        step = Step.AREA
        val p = provider ?: return
        val id = cameraId ?: return showCameras()
        stopPreview()
        bufferSize = null

        val previewView = PreviewView(this).apply {
            implementationMode = PreviewView.ImplementationMode.COMPATIBLE
            scaleType = PreviewView.ScaleType.FIT_CENTER
        }
        val selectView = AreaSelectView(this)
        this.selectView = selectView
        val stack = FrameLayout(this).apply {
            addView(previewView, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
            addView(selectView, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        }
        val hint = TextView(this).apply {
            text = getString(R.string.drag_hint)
            gravity = Gravity.CENTER
            setPadding(dp(8), dp(8), dp(8), dp(8))
        }
        val confirm = Button(this).apply { text = getString(R.string.confirm); isEnabled = false }
        val bar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            val weight = LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f)
            addView(Button(context).apply { text = getString(R.string.back); setOnClickListener { showCameras() } }, weight)
            addView(Button(context).apply {
                text = getString(R.string.full_image)
                setOnClickListener { selectView.area = NormRect.FULL }
            }, weight)
            addView(confirm, weight)
        }
        val layout = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(0xFF000000.toInt())
            addView(stack, LinearLayout.LayoutParams(MATCH_PARENT, 0, 1f))
            addView(hint, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
            addView(bar, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        }
        hint.setTextColor(0xFFFFFFFF.toInt())
        setScreen(layout)

        val preview = Preview.Builder().setTargetRotation(displayRotation()).build()
        this.preview = preview
        preview.surfaceProvider = previewView.surfaceProvider
        previewView.previewStreamState.observe(this) { state ->
            if (state != PreviewView.StreamState.STREAMING || this.preview !== preview) return@observe
            val r = preview.resolutionInfo?.resolution ?: return@observe
            bufferSize = Pair(r.width, r.height)
            val (w, h) = displaySize(r.width, r.height, degrees)
            selectView.imageAspect = w.toFloat() / h
            confirm.isEnabled = true
        }
        confirm.setOnClickListener {
            area = displayToBuffer(selectView.area, degrees, mirror)
            confirm()
        }
        try {
            val camera = p.bindToLifecycle(this, selectorFor(id), preview)
            degrees = camera.cameraInfo.getSensorRotationDegrees(displayRotation())
            mirror = camera.cameraInfo.lensFacing == CameraSelector.LENS_FACING_FRONT
            selectView.area = bufferToDisplay(area, degrees, mirror)
        } catch (e: Exception) {
            showMessage(getString(R.string.camera_error, e.message ?: e.javaClass.simpleName), getString(R.string.back)) {
                showCameras()
            }
        }
    }

    /** Saves the settings and opens the PiP window. Asks for the overlay permission first. */
    private fun confirm() {
        val id = cameraId ?: return
        val (bufferW, bufferH) = bufferSize ?: return
        if (!Settings.canDrawOverlays(this)) {
            showMessage(getString(R.string.overlay_permission), getString(R.string.open_settings)) {
                askOverlay.launch(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.parse("package:$packageName")))
            }
            return
        }
        val old = Prefs.load(this)
        val (screenW, screenH) = screenSize(this)
        val (imgW, imgH) = displaySize(bufferW, bufferH, degrees)
        val aspect = bufferToDisplay(area, degrees, mirror).aspect(imgW, imgH)
        val shortSide = minOf(screenW, screenH)
        val (w, h) = pipSize(aspect, (shortSide * PIP_FRACTION).roundToInt(), (screenW * MAX_FRACTION).roundToInt(),
            (screenH * MAX_FRACTION).roundToInt())
        val margin = dp(16)
        val (x, y) = clampPosition(old?.windowX ?: (screenW - w - margin), old?.windowY ?: dp(96), w, h, screenW, screenH)
        Prefs.save(this, Config(id, area, x, y, w, h, old?.border ?: Border.NONE, bufferW, bufferH))
        startPip()
    }

    private fun startPip() {
        stopPreview()
        ContextCompat.startForegroundService(this, Intent(this, OverlayService::class.java))
        finish()
    }

    companion object {
        const val EXTRA_STEP = "step"
        const val STEP_CAMERA = "camera"
        const val STEP_AREA = "area"
        /** The first PiP width, as a fraction of the short screen side. */
        private const val PIP_FRACTION = 0.45f
        /** The largest first PiP size, as a fraction of the screen size. */
        private const val MAX_FRACTION = 0.8f

        fun intent(ctx: Context, step: String): Intent =
            Intent(ctx, SetupActivity::class.java)
                .putExtra(EXTRA_STEP, step)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
    }
}
