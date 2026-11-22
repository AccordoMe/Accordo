plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

android {
    namespace = "me.accordo.mobile"
    compileSdk = 36

    defaultConfig {
        applicationId = "me.accordo.mobile"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        debug {
            // A dev server on the local network is http://, not https://.
            isMinifyEnabled = false
        }
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlin {
        // A full JDK 21 (Gradle fetches it) compiles; the bytecode target stays at 17 — Android's.
        jvmToolchain(21)
        compilerOptions {
            jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
        }
    }

    // The Rust library: `cargo ndk -t arm64-v8a -t x86_64 -o app/src/main/jniLibs build --release`
    // puts libaccordo_core.so here; the app loads it from the APK.
    sourceSets["main"].jniLibs.srcDirs("src/main/jniLibs")

    buildFeatures {
        compose = true
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2025.09.00")
    implementation(composeBom)

    implementation("androidx.activity:activity-compose:1.11.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.9.4")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material:material-icons-core")

    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.9.0")

    // Avatars and other server images (Coil handles the caching and the network).
    implementation("io.coil-kt:coil-compose:2.7.0")

    // uniffi's runtime on Android: it calls the Rust library through JNA.
    implementation("net.java.dev.jna:jna:5.17.0@aar")

    debugImplementation("androidx.compose.ui:ui-tooling")
}
