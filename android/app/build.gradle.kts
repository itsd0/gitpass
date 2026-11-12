plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "app.gitpass.android"
    compileSdk = 35

    defaultConfig {
        applicationId = "app.gitpass.android"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    buildTypes {
        release {
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
