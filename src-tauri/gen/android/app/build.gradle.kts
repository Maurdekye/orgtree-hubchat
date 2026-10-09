import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// Release signing: a keystore.properties OUTSIDE the repo, named by the
// HUBCHAT_ANDROID_KEYSTORE environment variable (storeFile, storePassword,
// keyAlias, keyPassword). Without it a release build is unsigned.
val releaseSigning = Properties().apply {
    System.getenv("HUBCHAT_ANDROID_KEYSTORE")?.let { path ->
        val f = file(path)
        if (f.exists()) f.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 37
    namespace = "dev.orgtree.hubchat"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "true" // hubs on a LAN speak plain http (design: trust note)
        manifestPlaceholders["appLabel"] = "Hubchat"
        applicationId = "dev.orgtree.hubchat"
        minSdk = 24
        targetSdk = 37
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        if (releaseSigning.getProperty("storeFile") != null) {
            create("release") {
                storeFile = file(releaseSigning.getProperty("storeFile"))
                storePassword = releaseSigning.getProperty("storePassword")
                keyAlias = releaseSigning.getProperty("keyAlias")
                keyPassword = releaseSigning.getProperty("keyPassword")
            }
        }
    }
    buildTypes {
        getByName("debug") {
            // Test builds install beside the user's real Hubchat, never over it.
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            manifestPlaceholders["appLabel"] = "Hubchat Test"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            signingConfigs.findByName("release")?.let { signingConfig = it }
            // HUBCHAT_TEST_BUILD=1: a release-configured build that installs
            // beside the user's real Hubchat (to prove release-only issues).
            if (System.getenv("HUBCHAT_TEST_BUILD") != null) {
                applicationIdSuffix = ".test"
                manifestPlaceholders["appLabel"] = "Hubchat Test"
            }
            // No R8 shrinking: it broke the barcode scanner plugin's
            // reflection in release (NPE when a scan returned; user crash
            // 2026-10-08 19:11Z). The code is small; correctness wins.
            optimization {
               enable = false
            }
            proguardFiles(
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    buildFeatures {
        buildConfig = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    // the periodic check when 'Stay connected' is off (design D6)
    implementation("androidx.work:work-runtime-ktx:2.10.5")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = file("tauri.build.gradle.kts"))

// Pictures from the keyboard (KeyboardImages.kt; user 2026-10-09): wry
// generates RustWebView, the app's WebView, on every build, so its input
// connection is routed to KeyboardImages here, after the Rust build and just
// before Kotlin compiles. The same in a local build and in CI.
val patchRustWebView by tasks.registering {
    val webView = file("src/main/java/dev/orgtree/hubchat/generated/RustWebView.kt")
    doLast {
        if (!webView.exists()) return@doLast
        val text = webView.readText()
        if (text.contains("KeyboardImages")) return@doLast
        val end = text.lastIndexOf('}')
        check(end > 0 && text.contains("class RustWebView(") && !text.contains("onCreateInputConnection")) {
            "RustWebView.kt has changed shape; route its input connection to KeyboardImages by hand"
        }
        webView.writeText(text.substring(0, end) +
            "\n    // Hubchat: pictures from the keyboard (app/build.gradle.kts, patchRustWebView)\n" +
            "    override fun onCreateInputConnection(outAttrs: android.view.inputmethod.EditorInfo): android.view.inputmethod.InputConnection? =\n" +
            "        dev.orgtree.hubchat.KeyboardImages.wrap(this, outAttrs, super.onCreateInputConnection(outAttrs))\n" +
            text.substring(end))
    }
}
patchRustWebView.configure { mustRunAfter(tasks.matching { it.name.startsWith("rustBuild") }) }
tasks.matching { it.name.startsWith("compile") && it.name.endsWith("Kotlin") }.configureEach { dependsOn(patchRustWebView) }
