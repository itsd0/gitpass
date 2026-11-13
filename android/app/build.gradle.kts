plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// App version = workspace version in the root Cargo.toml ([workspace.package] version), so the release
// PR that bumps the Rust crates also bumps the APK (#479). versionCode = major*10000 + minor*100 + patch;
// a pre-release suffix (-rc1) does not change the code.
val workspaceVersion: String = run {
    val cargo = rootProject.file("../Cargo.toml").readText()
    val section = cargo.substringAfter("[workspace.package]", "")
    Regex("""(?m)^version\s*=\s*"([^"]+)"""").find(section)?.groupValues?.get(1)
        ?: throw GradleException("version not found under [workspace.package] in Cargo.toml")
}
val workspaceVersionCode: Int = run {
    val m = Regex("""^(\d+)\.(\d+)\.(\d+)""").find(workspaceVersion)
        ?: throw GradleException("workspace version '$workspaceVersion' is not x.y.z")
    val (major, minor, patch) = m.destructured.toList().map { it.toInt() }
    if (minor > 99 || patch > 99) throw GradleException("minor/patch must be < 100 for versionCode")
    major * 10000 + minor * 100 + patch
}

// Release signing (#479): never committed. Read from environment variables (CI) or Gradle properties
// (e.g. ~/.gradle/gradle.properties on the owner's Mac). Without them a release build is unsigned.
fun signingValue(env: String, prop: String): String? =
    System.getenv(env)?.takeIf { it.isNotEmpty() } ?: (findProperty(prop) as String?)?.takeIf { it.isNotEmpty() }
val releaseKeystore = signingValue("GITPASS_ANDROID_KEYSTORE", "gitpass.android.keystore")
val releaseSigningValues = listOf(
    releaseKeystore,
    signingValue("ANDROID_KEYSTORE_PASSWORD", "gitpass.android.keystorePassword"),
    signingValue("ANDROID_KEY_ALIAS", "gitpass.android.keyAlias"),
    signingValue("ANDROID_KEY_PASSWORD", "gitpass.android.keyPassword"),
)
if (releaseSigningValues.any { it != null } && releaseSigningValues.any { it == null }) {
    throw GradleException("Android release signing is partly configured: set the keystore path, keystore password, key alias and key password, or none of them")
}

android {
    namespace = "app.gitpass.android"
    compileSdk = 35

    defaultConfig {
        applicationId = "app.gitpass.android"
        minSdk = 26
        targetSdk = 35
        versionCode = workspaceVersionCode
        versionName = workspaceVersion

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    signingConfigs {
        if (releaseSigningValues.all { it != null }) {
            create("release") {
                storeFile = file(releaseKeystore!!)
                storePassword = releaseSigningValues[1]
                keyAlias = releaseSigningValues[2]
                keyPassword = releaseSigningValues[3]
                // minSdk 26: APK Signature Scheme v2 + v3 (v1/JAR signing is not needed).
                enableV1Signing = false
                enableV2Signing = true
                enableV3Signing = true
            }
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.findByName("release")
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
        debug {
            isMinifyEnabled = false
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }
    kotlinOptions {
        jvmTarget = "21"
    }
    buildFeatures {
        compose = true
    }
    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
        getByName("androidTest") {
            // Shared encrypted fixtures (issue #451): single canonical copy
            // under crates/gitpass-core/tests/fixtures/shared, packaged as
            // test assets so instrumented tests can open them (see
            // SharedFixturesTest). No copies are kept under android/.
            assets.srcDir("../../crates/gitpass-core/tests/fixtures/shared")
        }
    }
    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
        jniLibs {
            useLegacyPackaging = true
        }
    }
    lint {
        // UniFFI generated bindings include runtime reflection fallback for java.lang.ref.Cleaner (API 33+)
        disable += "NewApi"
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.process)
    implementation(libs.androidx.activity.compose)
    implementation(libs.kotlinx.coroutines.android)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.ui)
    implementation(libs.androidx.ui.graphics)
    implementation(libs.androidx.ui.tooling.preview)
    implementation(libs.androidx.material3)
    // Passkey CredentialProviderService (#477); the provider classes are only used on API 34+.
    implementation(libs.androidx.credentials)
    // QR device pairing (#458): CameraX preview + analysis, ZXing core to encode/decode QR (on-device only).
    implementation(libs.androidx.camera.camera2)
    implementation(libs.androidx.camera.lifecycle)
    implementation(libs.androidx.camera.view)
    implementation(libs.zxing.core)

    // JNA AAR dependency required by UniFFI generated Kotlin bindings (pinned in libs.versions.toml)
    implementation(libs.net.java.dev.jna) {
        artifact {
            type = "aar"
        }
    }

    testImplementation(libs.junit)
    testImplementation("org.json:json:20240303") // Android platform JSON implementation for local JVM tests only
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.espresso.core)
    androidTestImplementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(libs.androidx.ui.test.junit4)
    debugImplementation(libs.androidx.ui.tooling)
    debugImplementation(libs.androidx.ui.test.manifest)
}
