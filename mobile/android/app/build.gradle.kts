plugins { id("com.android.application"); id("org.jetbrains.kotlin.android") }
android {
    namespace = "com.eyeflow.mobile"
    compileSdk = 35
    ndkVersion = "27.0.12077973"
    defaultConfig {
        applicationId = "com.eyeflow.mobile"
        minSdk = 26
        targetSdk = 35
        versionCode = 4
        versionName = "0.3.0"
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
        externalNativeBuild { cmake { cppFlags += "-std=c++17" } }
    }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    kotlinOptions { jvmTarget = "17" }
    externalNativeBuild { cmake { path = file("src/main/cpp/CMakeLists.txt"); version = "3.22.1" } }
    // Explicitly package the Rust runtime. Do not rely solely on AGP discovering
    // transitive CMake imports: the JNI bridge requires this DSO at runtime.
    sourceSets.getByName("main").jniLibs.srcDir("src/main/jniLibs")
    // AGP can also discover the identical CMake import. Resolve only that duplicate.
    packaging { jniLibs { pickFirsts += "**/libeyeflow_core.so" } }
}

// Check final ZIP contents after packaging, including recursive ELF dependencies.
// CI and local assembleDebug both fail if the Rust runtime is absent.
val verifyDebugNativeClosure by tasks.registering(Exec::class) {
    val windows = System.getProperty("os.name").startsWith("Windows")
    val python = providers.gradleProperty("nativeVerifierPython").getOrElse(if (windows) "python" else "python3")
    val host = if (windows) "windows-x86_64" else if (System.getProperty("os.name").startsWith("Mac")) "darwin-x86_64" else "linux-x86_64"
    val readelf = android.sdkDirectory.resolve("ndk/${android.ndkVersion}/toolchains/llvm/prebuilt/$host/bin/llvm-readelf${if (windows) ".exe" else ""}")
    commandLine(python, rootProject.file("tests/verify-apk-native.py"),
        layout.buildDirectory.file("outputs/apk/debug/app-debug.apk").get().asFile,
        "--readelf", readelf)
}
tasks.matching { it.name == "assembleDebug" }.configureEach { finalizedBy(verifyDebugNativeClosure) }
