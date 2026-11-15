import webcamviewpip.signing.ReleaseSigning

// The release key: decrypted from keystore-backup/webcam-view-pip-key.enc with WEBCAMVIEWPIP_KEY_PASSPHRASE
// (environment or the git-ignored android/.env), or else read from keystore.properties. See README.
val releaseSigning: ReleaseSigning.Result? = ReleaseSigning.load(rootProject.projectDir, System.getenv())
val releaseRequested = gradle.startParameter.taskNames.any { it.contains("Release", ignoreCase = true) }
if (releaseRequested) {
    if (releaseSigning == null) {
        logger.warn("webcam-view-pip: no release key. Set WEBCAMVIEWPIP_KEY_PASSPHRASE in android/.env. The release APK is unsigned.")
    } else {
        logger.lifecycle("webcam-view-pip: release key from ${releaseSigning.source}")
    }
}

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// The release workflow sets VERSION_NAME from the tag, for example "0.3.0".
val appVersionName: String = System.getenv("VERSION_NAME") ?: "0.0.0-dev"
val appVersionCode: Int = appVersionName.substringBefore('-').split('.')
    .map { it.toIntOrNull() ?: 0 }
    .let { (it + listOf(0, 0, 0)).take(3) }
    .let { (major, minor, patch) -> maxOf(1, major * 10000 + minor * 100 + patch) }

android {
    namespace = "io.github.muhammadtowfiqueimam.webcamviewpip"
    compileSdk = 35

    defaultConfig {
        applicationId = "io.github.muhammadtowfiqueimam.webcamviewpip"
        minSdk = 26
        targetSdk = 35
        versionCode = appVersionCode
        versionName = appVersionName
    }

    signingConfigs {
        releaseSigning?.let { key ->
            create("release") {
                storeFile = key.storeFile
                storePassword = key.storePassword
                keyAlias = key.keyAlias
                keyPassword = key.keyPassword
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
            signingConfig = signingConfigs.findByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    lint {
        abortOnError = true
        checkReleaseBuilds = true
    }
}

dependencies {
    val camerax = "1.4.1"
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.activity:activity-ktx:1.9.3")
    implementation("androidx.lifecycle:lifecycle-service:2.8.7")
    implementation("androidx.camera:camera-core:$camerax")
    implementation("androidx.camera:camera-camera2:$camerax")
    implementation("androidx.camera:camera-lifecycle:$camerax")
    implementation("androidx.camera:camera-view:$camerax")
    testImplementation("junit:junit:4.13.2")
}

tasks.register("encryptReleaseKey") {
    group = "signing"
    description = "Encrypts keystore.properties and its keystore into keystore-backup/webcam-view-pip-key.enc."
    doLast {
        val file = ReleaseSigning.encrypt(rootProject.projectDir, System.getenv())
        println("Wrote $file. Commit this file. Keep WEBCAMVIEWPIP_KEY_PASSPHRASE in a password manager.")
    }
}
