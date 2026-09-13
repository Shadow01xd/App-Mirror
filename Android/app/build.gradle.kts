plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}
android {
    namespace = "com.tyu.app"
    compileSdk = 36
    defaultConfig {
        applicationId = "com.tyu.app"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    // Preserve the requested frontend folders as source roots of the single app module.
    sourceSets["main"].java.srcDirs("../ui", "../feature", "../model", "../mock", "../preview")
    buildFeatures { compose = true; buildConfig = true }
    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("generated/tyuJniLibs"))
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildTypes {
        release { isMinifyEnabled = false }
    }
    packaging { resources.excludes += "/META-INF/{AL2.0,LGPL2.1}" }
}
dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(composeBom)
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("com.journeyapps:zxing-android-embedded:4.3.0")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.animation:animation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    debugImplementation("androidx.compose.ui:ui-tooling")
    androidTestImplementation(composeBom)
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    // Android 16 removed the reflective InputManager API used by Espresso 3.6.
    androidTestImplementation("androidx.test.espresso:espresso-core:3.7.0")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}

val buildTyuCore by tasks.registering(Exec::class) {
    val abis = providers.gradleProperty("tyuAbis").getOrElse("arm64-v8a,x86_64")
    workingDir(rootProject.projectDir.parentFile)
    commandLine("powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
        rootProject.projectDir.parentFile.resolve("tools/build-android-core.ps1").absolutePath,
        "-Abis", abis)
    inputs.property("abis", abis)
    inputs.file(rootProject.projectDir.parentFile.resolve("tools/build-android-core.ps1"))
    inputs.file(rootProject.projectDir.parentFile.resolve("Cargo.toml"))
    inputs.files(fileTree(rootProject.projectDir.parentFile.resolve("core")) { include("**/*.rs", "**/Cargo.toml") })
    inputs.file(rootProject.projectDir.parentFile.resolve("Cargo.lock"))
    outputs.dir(layout.buildDirectory.dir("generated/tyuJniLibs"))
}
tasks.named("preBuild") { dependsOn(buildTyuCore) }
