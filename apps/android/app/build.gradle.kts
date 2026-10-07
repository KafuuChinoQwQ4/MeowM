import org.gradle.api.tasks.Copy
import org.gradle.api.tasks.Exec
import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

val repoRoot = rootProject.projectDir.resolve("../..").canonicalFile
val localProperties = Properties().apply {
    rootProject.file("local.properties").takeIf { it.isFile }?.inputStream()?.use(::load)
}
val sdkRoot = System.getenv("ANDROID_SDK_ROOT")
    ?: System.getenv("ANDROID_HOME")
    ?: localProperties.getProperty("sdk.dir")
    ?: error("Set ANDROID_SDK_ROOT, ANDROID_HOME, or sdk.dir in apps/android/local.properties")
val ndkRoot = System.getenv("ANDROID_NDK_ROOT")
    ?: System.getenv("ANDROID_NDK_HOME")
    ?: localProperties.getProperty("ndk.dir")
    ?: file("$sdkRoot/ndk").listFiles()?.filter { it.isDirectory }?.maxByOrNull { it.name }?.absolutePath
    ?: error("Set ANDROID_NDK_ROOT, ANDROID_NDK_HOME, ndk.dir, or install an NDK under the Android SDK")
val hostOs = providers.gradleProperty("androidNdkHostOs").orElse(System.getProperty("os.name")).get()
val hostArch = providers.gradleProperty("androidNdkHostArch").orElse(System.getProperty("os.arch")).get()
val hostTag = when {
    hostOs.equals("linux", ignoreCase = true) -> if (hostArch == "aarch64") "linux-aarch64" else "linux-x86_64"
    hostOs.lowercase() in setOf("mac os x", "darwin") -> "darwin-x86_64"
    hostOs.startsWith("windows", ignoreCase = true) -> {
        require(hostArch.lowercase() in setOf("amd64", "x86_64")) {
            "Only the Windows x86_64 NDK host is supported"
        }
        "windows-x86_64"
    }
    else -> error("Unsupported NDK host: $hostOs $hostArch")
}
val toolchainBin = file("$ndkRoot/toolchains/llvm/prebuilt/$hostTag/bin")
val isWindowsHost = hostTag.startsWith("windows-")
val clangExecutable = toolchainBin.resolve(if (isWindowsHost) "clang.exe" else "clang")
val llvmArExecutable = toolchainBin.resolve(if (isWindowsHost) "llvm-ar.exe" else "llvm-ar")
val rustTargetDir = providers.environmentVariable("CARGO_TARGET_DIR").map { file(it) }.orElse(layout.buildDirectory.dir("cargo-target").map { it.asFile })
val nativeLibDir = layout.buildDirectory.dir("generated/jniLibs/main")
val filteredResDir = layout.buildDirectory.dir("generated/res/main")
tasks.register("printAndroidNdkHostConfiguration") {
    doLast {
        println("NDK host tag: $hostTag")
        println("NDK clang executable: $clangExecutable")
        println("Cargo target directory: ${rustTargetDir.get()}")
    }
}
val prepareAndroidResources by tasks.registering(Copy::class) {
    from("src/main/res")
    into(filteredResDir)
    exclude("**/tree")
}

val rustAbis = listOf(
    Triple("x86_64", "x86_64-linux-android", "x86_64-linux-android"),
    Triple("arm64-v8a", "aarch64-linux-android", "aarch64-linux-android"),
)
val cargoBuildRustTasks = rustAbis.map { (abi, target, linkerPrefix) ->
    val taskName = "cargoBuildRust${if (abi == "arm64-v8a") "Arm64" else "X86_64"}"
    tasks.register<Exec>(taskName) {
        // NDK's target-prefixed Windows launchers are .cmd wrappers. Use clang.exe
        // directly there and tell both Rust and cc-rs which Android target to emit.
        val linker = if (isWindowsHost) clangExecutable.absolutePath
            else toolchainBin.resolve("${linkerPrefix}24-clang").absolutePath
        workingDir(repoRoot)
        commandLine("cargo", "build", "--release", "--target", target, "-p", "ledger-android-bridge", "-p", "ledger-slint-ui", "--lib")
        environment("CARGO_TARGET_DIR", rustTargetDir.get().absolutePath)
        environment("CARGO_BUILD_JOBS", System.getenv("CARGO_BUILD_JOBS") ?: "1")
        environment("CARGO_TARGET_${target.uppercase().replace('-', '_')}_LINKER", linker)
        environment("CC_${target}", linker)
        if (isWindowsHost) {
            environment("CFLAGS_${target}", "--target=${target}24")
            environment("AR_${target}", llvmArExecutable.absolutePath)
            environment("RUSTFLAGS", "-C link-arg=--target=${target}24")
        }
        doLast {
            for (library in listOf("ledger_android_bridge", "ledger_slint_ui")) {
                val output = rustTargetDir.get().resolve("$target/release/lib$library.so")
                val destination = nativeLibDir.get().file("$abi/lib$library.so").asFile
                destination.parentFile.mkdirs()
                output.copyTo(destination, overwrite = true)
            }
        }
    }
}

android {
    namespace = "com.example.localledger"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.example.localledger"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    sourceSets["main"].jniLibs.srcDir(nativeLibDir)
    sourceSets["main"].res.setSrcDirs(listOf(filteredResDir))
}

dependencies {
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
}

tasks.named("preBuild").configure { dependsOn(cargoBuildRustTasks) }
tasks.named("preBuild").configure { dependsOn(prepareAndroidResources) }
