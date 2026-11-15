package io.github.muhammadtowfiqueimam.webcamviewpip

import android.annotation.SuppressLint
import android.content.Context
import android.os.Build
import android.util.DisplayMetrics
import android.view.WindowManager
import androidx.camera.camera2.interop.Camera2CameraInfo
import androidx.camera.core.CameraInfo
import androidx.camera.core.CameraSelector
import androidx.camera.lifecycle.ProcessCameraProvider

/** Loads and saves the [Config] in the app's shared preferences. */
object Prefs {
    private fun prefs(ctx: Context) = ctx.getSharedPreferences("config", Context.MODE_PRIVATE)

    fun load(ctx: Context): Config? = Config.fromMap(prefs(ctx).all)

    fun save(ctx: Context, config: Config) {
        val editor = prefs(ctx).edit().clear()
        config.toMap().forEach { (key, value) -> editor.putString(key, value) }
        editor.apply()
    }
}

/** The full screen size in physical pixels, for the current rotation. */
fun screenSize(ctx: Context): Pair<Int, Int> {
    val wm = ctx.getSystemService(WindowManager::class.java)
    return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        val bounds = wm.currentWindowMetrics.bounds
        Pair(bounds.width(), bounds.height())
    } else {
        val metrics = DisplayMetrics()
        @Suppress("DEPRECATION")
        wm.defaultDisplay.getRealMetrics(metrics)
        Pair(metrics.widthPixels, metrics.heightPixels)
    }
}

/** A camera that the user can select. */
data class CameraEntry(val id: String, val label: String)

/** The Camera2 ID of a CameraX camera. */
@SuppressLint("UnsafeOptInUsageError")
fun cameraId(info: CameraInfo): String = Camera2CameraInfo.from(info).cameraId

/** All cameras, with labels such as "Back camera" or "USB camera 2". */
fun listCameras(ctx: Context, provider: ProcessCameraProvider): List<CameraEntry> {
    val infos = provider.availableCameraInfos
    val names = infos.map { info ->
        when (info.lensFacing) {
            CameraSelector.LENS_FACING_BACK -> ctx.getString(R.string.camera_back)
            CameraSelector.LENS_FACING_FRONT -> ctx.getString(R.string.camera_front)
            else -> ctx.getString(R.string.camera_external)
        }
    }
    return infos.mapIndexed { i, info ->
        val sameName = names.count { it == names[i] }
        val label = if (sameName > 1) "${names[i]} ${names.take(i + 1).count { it == names[i] }}" else names[i]
        CameraEntry(cameraId(info), label)
    }
}

/** A selector for the camera with this Camera2 ID. */
fun selectorFor(id: String): CameraSelector =
    CameraSelector.Builder().addCameraFilter { infos -> infos.filter { cameraId(it) == id } }.build()
