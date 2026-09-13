# Android ↔ Desktop: conexión nativa

La app Android usa el mismo `tyu-core` que Desktop. Compose envía comandos a `ConnectionViewModel`, JNI ejecuta el Core y sus eventos actualizan la conexión. El tráfico es TyuLink binario sobre QUIC/TLS 1.3; el QR no abre un servidor web.

## Compilar e instalar en Windows

Requisitos: JDK 17, Android SDK 36, Android NDK, Rust y los targets siguientes. El helper detecta el SDK en `ANDROID_HOME` o `%LOCALAPPDATA%/Android/Sdk`; el NDK en `ANDROID_NDK_HOME` o en la carpeta `ndk` del SDK. En esta máquina se usó NDK r30.

```powershell
rustup target add aarch64-linux-android x86_64-linux-android
$env:ANDROID_HOME = Join-Path $env:LOCALAPPDATA 'Android\Sdk'
cd Android
.\gradlew.bat :app:assembleDebug
```

APK: `Android/app/build/outputs/apk/debug/app-debug.apk`, Android 8/API 26 o posterior, ARM64 y x86_64. Gradle compila Rust automáticamente; `tools/build-android-core.ps1` también puede ejecutarse por separado. El helper actual es para Windows. `-PtyuAbis=arm64-v8a` limita la compilación solicitada; para cambiar las ABIs de un APK ya generado, usar un directorio de build nuevo o la tarea Gradle `clean` primero.

Instala el APK en el teléfono. Con ADB y un único dispositivo conectado:

```powershell
& "$env:ANDROID_HOME\platform-tools\adb.exe" install -r app/build/outputs/apk/debug/app-debug.apk
```

## Conectar

1. Pon PC y teléfono en la misma red local. Ejecuta `cargo run -p tyu-desktop` desde la raíz.
2. Abre TYU en Android: **START → Continuar**. La búsqueda empieza automáticamente. En **Tu próximo enlace**, pulsa **Conectar** junto al equipo: no requiere un QR ni confianza previa.
3. Como alternativa, entra en **Conectar mediante QR → Escanear QR**. Concede cámara y escanea el código actual de Desktop. Caduca en 120 segundos y se renueva; cada ticket se usa una sola vez. No hay campo para pegar enlaces.
4. Comprueba el dispositivo solicitante y pulsa **Aceptar** en Desktop. Android muestra el nombre real de la PC al recibir `Connected` del Core.
5. Toca el nombre de la PC y **Desconectar**. El equipo guardado permite **Conectar** sin otro QR. **Olvidar dispositivo** revoca su identidad; no es un simple borrado de la lista.

El QR elige la IP de la ruta IPv4 activa, evitando elegir la primera tarjeta virtual solo por su orden. `TYU_ADVERTISE_IP` permite una selección explícita. Al conectar, Core prueba en paralelo hasta ocho direcciones anunciadas y usa la primera que verifica la identidad TLS; envía una sola solicitud de aprobación. Permite tráfico UDP de TYU en la red privada cuando Windows lo solicite. El aislamiento de clientes del router puede impedir el enlace. El QR proporciona el endpoint sin depender de multicast.

Actualiza **Desktop y Android juntos**: el Desktop anterior no admite la solicitud inicial por búsqueda. Instalar solo el APK nuevo no actualiza el servidor del PC.

La confianza sobrevive a reinicios de la app. La conexión se conserva al girar o recrear la Activity; Android puede terminar el proceso en segundo plano. Tras ese cierre se reconecta desde el equipo guardado. No se implementó todavía un servicio en primer plano ni reconexión automática.

## Prueba instrumentada contra Desktop

Separa la fixture visual de la prueba que necesita un servidor real. Compila `:app:assembleDebug :app:assembleDebugAndroidTest` e instala ambos APK. `TyuJourneyTest` se ejecuta sin servidor; `NativeConnectionTest` usa `-e tyuNearby true` para búsqueda real en LAN o una invitación vigente para probar la entrada de un QR.

Para un emulador estándar, inicia un Desktop de prueba con perfil aislado, `TYU_ADVERTISE_IP=10.0.2.2` y `TYU_PAIRING_URI_FILE` apuntando a un archivo temporal privado. El alias `10.0.2.2` es solo para el emulador, no para un teléfono físico. Lee la invitación inmediatamente antes de ejecutar la prueba:

```powershell
$uri = (Get-Content -Raw $env:TYU_PAIRING_URI_FILE).Trim()
& "$env:ANDROID_HOME\platform-tools\adb.exe" -s emulator-5556 shell am instrument -w -r -e class com.tyu.app.NativeConnectionTest -e tyuPairingUri $uri com.tyu.app.test/androidx.test.runner.AndroidJUnitRunner
```

Aprueba la solicitud en la ventana Desktop durante la ejecución. La variante QR verifica que no existe el campo de texto y entrega una invitación ya decodificada a `ConnectionViewModel.pair`, la misma función que invoca el escáner. La decodificación con cámara y los intents externos se comprueban por separado. La variante `tyuNearby=true` toca un resultado real de mDNS sin ticket ni confianza previa. Ambas esperan conexión, desconectan, reconectan con confianza y recrean la Activity. El archivo temporal contiene una invitación sensible; elimínalo al terminar. No publiques ese URI en logs.

Para verificar persistencia entre procesos, conserva el mismo Desktop, ejecuta `adb -s emulator-5556 shell am force-stop com.tyu.app` y repite la clase con `-e tyuReconnectOnly true` en lugar de `-e tyuPairingUri $uri`. Esta variante exige que el emparejamiento anterior ya haya terminado y no usa otra invitación ni aprobación.

## Alcance

Conexión, identidad, confianza, descubrimiento y eventos nativos están integrados. El Core también puede recibir archivos en su inbox privado, pero Android aún no expone selector, historial ni exportación a SAF. Los botones de Monitor, Espejo, Cámara, Micrófono y Almacenamiento no activan adaptadores inexistentes. Captura, codecs, render, SAF, portapapeles del sistema y drivers virtuales quedan en la siguiente fase.

La validación concreta de esta ejecución se registra en [BACKEND_STATUS.md](BACKEND_STATUS.md); no sustituye las pruebas en un teléfono físico y una LAN con varios equipos.
