# TYU · App Mirror

Duplica, extiende y comparte tu PC con tu teléfono Android a través de la red local. Sin nube, sin telemetría y sin runtime web.

TYU se compone de un **Core en Rust** compartido y dos clientes nativos:

- **Desktop**: aplicación nativa en Rust + Slint.
- **Android**: aplicación en Kotlin + Jetpack Compose que usa el mismo Core por JNI.

La comunicación utiliza **TyuLink** sobre QUIC/TLS 1.3, con descubrimiento por mDNS, emparejamiento por QR, aprobación explícita en la PC y confianza persistente para reconectar.

## Modos

| Modo | Dirección | Descripción |
| --- | --- | --- |
| Monitor | PC → teléfono | Usa el teléfono como pantalla extendida o duplicada |
| Espejo | teléfono → PC | Muestra la pantalla del teléfono en la PC |
| Bypass | teléfono ↔ PC | Solo servicios: almacenamiento, cámara, micrófono, envío de archivos y portapapeles |

> **Estado:** la negociación de protocolo, el emparejamiento, la transferencia de archivos y el almacenamiento funcionan. La captura de pantalla, cámara y micrófono, la inyección de entrada y los controladores virtuales aún no existen. Consulta [BACKEND_STATUS](docs/BACKEND_STATUS.md).

## Estructura

```text
Android/          App Android (Gradle, un único módulo `app`)
desktop/          App de escritorio (Slint)
core/
  tyu-types/      Tipos serializables (IDs, capacidades, dispositivos, sesiones)
  tyu-protocol/   Esquema binario v1, límites y validación
  tyu-core/       Actor Tokio, QUIC/TLS, descubrimiento, sesiones y archivos
  tyu-ffi/        ABI C para JNI/Android
tools/
  tyu-test-peer/  Par de pruebas nativo (CLI)
  *.ps1           Scripts de compilación e instalación
docs/             Arquitectura, protocolo, seguridad y más
```

## Requisitos

- Rust (Edition 2024) y Cargo
- Android: JDK 17, Android SDK 36, NDK y los targets `aarch64-linux-android` y `x86_64-linux-android`

## Inicio rápido

### Desktop

```powershell
cargo run -p tyu-desktop
```

Desde la app, escanea el QR con TYU en Android o selecciona la PC en la lista de dispositivos, y aprueba la solicitud en el escritorio.

### Par de pruebas (CLI)

```powershell
cargo run -p tyu-test-peer -- capabilities
cargo run -p tyu-test-peer -- pair 'tyu://pair/...'
```

### Android

Abre la carpeta `Android` en Android Studio, o desde la terminal:

```powershell
cd Android
.\gradlew.bat :app:assembleDebug
```

El APK queda en `Android/app/build/outputs/apk/debug/app-debug.apk` (Android 8.0 / API 26 o posterior). Guía completa: [ANDROID_CONNECTION](docs/ANDROID_CONNECTION.md).

## Verificación

```powershell
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p tyu-desktop -- --smoke
```

## Documentación

- [Arquitectura](docs/ARCHITECTURE.md)
- [Protocolo](docs/PROTOCOL.md)
- [Seguridad](docs/SECURITY.md)
- [Estado del backend](docs/BACKEND_STATUS.md)
- [Conexión Android](docs/ANDROID_CONNECTION.md)
- [Bypass](docs/BYPASS.md) · [Micrófono](docs/MICROPHONE.md)
- [Desktop](desktop/README.md) · [Android](Android/README.md)
