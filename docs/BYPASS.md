# Bypass local: estado e instalación

Bypass usa la conexión autenticada de TYU Core sobre QUIC entre Android y Windows. Cámara, micrófono, almacenamiento y archivos son servicios independientes. El tráfico del teléfono viaja por la red local; no necesita servidor de Internet. Las implementaciones de Mirror y Monitor se conservan.

## Cámara propia de TYU

`desktop/native/camera` contiene la fuente COM y el publicador de cámara virtual de TYU. No depende de OBS ni de otra aplicación de cámara. Requiere Windows 11 (build 22000 o posterior) y permiso de cámara de Windows. Publica **TYU Camera (Windows Virtual Camera)** mientras el servicio está activo. Android captura con Camera2, codifica H.264 a 1280×720 y 30 fps solicitados; Windows decodifica y entrega NV12 mediante Media Foundation. La frecuencia efectiva depende del dispositivo y de la carga.

Compilar desde la raíz con las herramientas C++ de Visual Studio y Windows SDK instaladas:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/build-tyu-camera.ps1
cargo build -p tyu-desktop
```

Registrar el componente una vez desde PowerShell **como administrador**:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/install-tyu-camera.ps1
```

El instalador copia el componente propio a `Program Files\TYU\Camera`, registra su CLSID y conserva las DLL de versiones anteriores que Windows pueda tener cargadas. No cambia permisos generales de cámara. `-Uninstall` desregistra únicamente la cámara TYU. Al actualizar se debe volver a ejecutar el instalador; no es necesario instalar OBS. La aplicación busca el publicador junto al ejecutable y después en la ubicación instalada.

Conecta Android, entra en Bypass, activa Cámara y concede su permiso en el móvil. En la aplicación del PC selecciona TYU Camera. Al desconectar se detiene la captura y se retira la cámara de esa sesión. Los datos pasan por una tubería local restringida al usuario, SYSTEM y Local Service; no se expone un servidor de video en la LAN.

Referencia de la API de Windows: [MFCreateVirtualCamera](https://learn.microsoft.com/en-us/windows/win32/api/mfvirtualcamera/nf-mfvirtualcamera-mfcreatevirtualcamera).

## Almacenamiento y archivos

Al activar Almacenamiento y autorizar el acceso a archivos en Android, se registra **TYU: nombre del móvil** en Este equipo y en el árbol del Explorador. El adaptador WebDAV escucha exclusivamente en `127.0.0.1`, con una ruta aleatoria; Windows WebClient lo consume y las operaciones se reenvían por QUIC al teléfono vinculado. No se abre WebDAV a la red.

Permite listar por páginas, leer, crear carpetas, escribir, renombrar y borrar. La escritura se prepara en un archivo temporal hermano y se renombra al terminar. Incluye los bloqueos exclusivos que usa el Explorador. Al apagar el servicio se revoca el proveedor y se retira la entrada del Explorador.

El permiso de Android permite el almacenamiento compartido (DCIM, Download, Documents, etc.). No elimina las restricciones del sistema sobre datos privados de aplicaciones, `Android/data` o `Android/obb`. La versión actual exporta el almacenamiento compartido principal, no una tarjeta SD adicional. No conserva fechas ni atributos de Windows al copiar. El servicio WebClient debe estar disponible. Su límite de tamaño depende de la configuración de Windows: en el PC de validación era 50.000.000 bytes. TYU no modifica ese ajuste global silenciosamente; para archivos mayores utiliza la transferencia directa de la aplicación (límite del Core: 8 GiB).

La transferencia directa usa selectores reales y progreso del Core. En Windows los archivos recibidos se guardan en **Documentos\TYU**, incluyendo Documentos redirigido a OneDrive. La opción de desarrollo `--data-dir` utiliza su subcarpeta `Received`. En Android los recibidos se publican en **Descargas/TYU**. Los archivos se procesan por bloques, con integridad BLAKE3; no se cargan enteros en RAM. El envío Android prepara una copia temporal en caché, por lo que necesita espacio libre.

## Micrófono propio de TYU: pendiente de firma y prueba del dispositivo

AudioRecord captura PCM mono de 48 kHz y 16 bits y lo envía por QUIC. El adaptador Windows escribe directamente al controlador WaveRT de `desktop/native/microphone`, que publica **TYU Microphone**. Se eliminó la dependencia de VB-CABLE; no se reproduce el audio en altavoces.

El controlador, instalador y prueba WASAPI compilan; pasan las pruebas del búfer PCM, el análisis estático, la validación INF y la creación del catálogo. **El paquete local aún no tiene firma de Microsoft, no está instalado y su salida real no está validada. El micrófono todavía no funciona como periférico en este PC.** La aplicación informa el error si falta el controlador y detiene el servicio. No se modificaron las opciones de seguridad de Windows.

La instalación del paquete firmado y las pruebas pendientes están documentadas en [MICROPHONE.md](MICROPHONE.md). Bypass no se considera al 100 % hasta comprobar esta ruta de extremo a extremo.

## Validación reproducible

- `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- `target/tyu-camera/TyuCameraHost.exe --self-test`: 70 muestras NV12 verificadas, incluida una pausa y reconexión de tubería.
- `Android/gradlew.bat :app:assembleDebug :app:assembleDebugAndroidTest :app:lintDebug` con Java 17 y `ANDROID_HOME`. El APK incluye ARM64 y x86_64.
- `desktop/examples/bypass_probe.rs` es un host explícito de pruebas sobre el mismo Core; acepta automáticamente **sólo en ese ejecutable de prueba**, nunca en TYU Desktop. Recibe un directorio temporal, publica allí `invitation.txt` para el alias de emulador `10.0.2.2` y se cierra a los 180 segundos.
- `BypassIntegrationTest` recibe esa invitación mediante el argumento de instrumentación `tyuPairingUri`. El emulador de prueba necesita CAMERA, RECORD_AUDIO y acceso a todos los archivos concedidos. Durante la ventana de 60 segundos, escribir `written from Windows` en `Documents/TYUBypassValidation/pc.txt` mediante la entrada TYU del Explorador. El test verifica el contenido en Android, una transferencia directa, sesiones de cámara y audio, y que no haya sesiones Mirror/Monitor. Sin invitación explícita, esta prueba se omite.

Se verificó Windows WebClient contra Android: lectura, escritura, carpeta nueva, renombrado y eliminación; Android confirmó el archivo escrito desde Windows. Una aplicación DirectShow recibió imágenes de la cámara Android a través de TYU Camera y pudo cerrar y reabrir el dispositivo. FFmpeg se utilizó únicamente como consumidor de prueba, no es dependencia del producto. La cámara del emulador produce una imagen de prueba; no se ha validado aún con un teléfono físico en esta ejecución. Tampoco se ha validado la salida del micrófono virtual, porque el controlador propio de TYU aún no está firmado ni instalado.
