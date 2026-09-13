# TYU · Android

App nativa en Kotlin, Jetpack Compose y Material 3, con la interfaz existente conservada. La conexión normal utiliza el Core Rust compartido por JNI y TyuLink sobre QUIC/TLS 1.3. Puedes conectar tocando un equipo encontrado por búsqueda o escaneando su QR. Ambas formas solicitan aprobación en la PC, guardan confianza y permiten desconectar y reconectar. No hay campo de texto para enlaces.

Guía de compilación, instalación y conexión: [ANDROID_CONNECTION.md](../docs/ANDROID_CONNECTION.md).

## Diseño y movimiento

La bienvenida y el dispositivo conectado comparten un esquema geométrico de teléfono/PC dibujado en Compose. El enlace se traza una sola vez al entrar. Los botones responden a la pulsación con una compresión del 2 %, las opciones cambian de color suavemente y la navegación combina fundido con un desplazamiento corto. Las animaciones utilizan el ajuste de duración de Android, incluida la opción de quitarlas.

Monitor, Cámara y Micrófono mantienen su acción principal visible mientras se desplazan las opciones. Los servicios y la actividad se presentan como filas, con estados legibles; cámara y audio tienen representaciones visuales específicas. La paleta original, los modos y el comportamiento local se conservan.

La especificación del diseño implementado se documenta en `DESIGN.md`; el alcance del producto, en `PRODUCT.md`.

## Abrir y compilar

Abre **esta carpeta `Android`** en Android Studio y sincroniza Gradle. Requiere JDK 17, Android SDK 36, NDK y Rust con los targets `aarch64-linux-android` y `x86_64-linux-android`. Gradle invoca el helper Rust de Windows y empaqueta ambas bibliotecas; no requiere compilar Desktop. El workspace Rust host sigue compilando sin NDK.

```powershell
# Usa el JDK incluido en tu instalación de Android Studio si JAVA_HOME apunta a un Java antiguo.
.\gradlew.bat :app:assembleDebug
.\gradlew.bat :app:lintDebug
```

APK de desarrollo: `app/build/outputs/apk/debug/app-debug.apk`.
Se puede instalar en Android 8.0 (API 26) o posterior.
`local.properties` contiene la ruta del SDK de esta máquina y está excluido de Git; Android Studio puede regenerarlo en otra.

Las versiones están fijadas para reproducibilidad: AGP 8.12.0, Gradle 8.13, Kotlin/Compose Compiler 2.0.21 y Compose BOM 2024.12.01. AGP 8.12 admite SDK 36 y requiere Gradle 8.13: [compatibilidad oficial](https://developer.android.com/build/releases/agp-8-12-0-release-notes). El compilador Compose usa la misma versión que Kotlin siguiendo la [configuración oficial](https://developer.android.com/develop/ui/compose/setup-compose-dependencies-and-compiler).

## Fixture visual de desarrollo

El recorrido siguiente pertenece exclusivamente a previews y a `TyuJourneyTest`, que habilita `tyu.fixture=true` en una compilación debug. La app normal muestra equipos y estados de conexión reales, no estos ejemplos.

1. START → permisos visuales → agregar dispositivo → lista de equipos.
2. **Rimi-PC** y **Studio** conectan. **PC X** muestra el error; **Reintentar** permite continuar.
3. **Conectar mediante QR** abre el marco visual. **Continuar** conecta el equipo de ejemplo; **Cancelar** vuelve.
4. La pantalla conectada muestra el dispositivo, su estado y la píldora **Monitor | Espejo | Bypass**. Cada opción selecciona el modo y abre sus controles.
5. **Monitor:** PC → teléfono. Ofrece escritorio extendido/duplicado, orientación y calidad; su estado activo es un placeholder. Toca la pantalla para mostrar u ocultar controles.
6. **Espejo:** teléfono → PC. Incluye preparación, estado activo y detener.
7. **Bypass:** teléfono ↔ PC, solo servicios; ninguna pantalla compartida. Abre almacenamiento, cámara, micrófono y envío.
8. Cámara y micrófono tienen configuración, activar y detener. Volver conserva su estado para consultarlo en **Actividad actual**.
9. Envío permite elegir ejemplos, ver 100 % / 53 % / esperando, completar la vista previa o cancelar.
10. Toca el nombre de la PC para ver IP, calidad, proximidad y servicios. **Olvidar dispositivo** solicita confirmación y vuelve al estado vacío.
11. Los iconos superiores abren actividad y configuración. Los controles de configuración solo cambian la UI.

Cambiar entre Monitor, Espejo y Bypass detiene la representación de pantalla del modo anterior. Cámara, micrófono y almacenamiento mantienen estados independientes. La conexión, la navegación y los controles locales se restauran al recrear la actividad o girar el dispositivo. No hay persistencia de cuenta o de equipos entre inicios nuevos de la aplicación.

## Organización

Se conserva la estructura de carpetas solicitada. Hay **un único módulo Android**, `app`; `ui`, `feature`, `model`, `mock` y `preview` son directorios de fuentes registrados en `app/build.gradle.kts`, no módulos Gradle separados.

- `app/src/main/java/com/tyu/app`: actividad, raíz Compose y navegación local con pila de regreso.
- `ui/theme`: paleta, tipografía, espaciado y formas.
- `ui/components`: componentes compartidos, página adaptable, selector de modos, diálogos y hojas inferiores.
- `ui/icons`: equivalentes Material, sin copiar los iconos de Figma.
- `feature`: pantallas y estados visuales; incluye `home` para el dispositivo conectado y `bypass` para servicios.
- `model`, `mock`: datos de presentación y estado local observable.
- `preview`: previews Android Studio para los estados principales, componentes, pantalla compacta, horizontal y fuente grande.

## Iosevka Charon

No se descargó ninguna fuente. El cuerpo y los controles usan `FontFamily.SansSerif` de Android; las etiquetas técnicas usan `FontFamily.Monospace`.
Agrega tu archivo auténtico en `app/src/main/res/font/iosevka_charon.ttf` o `.otf`; `ui/theme/Typography.kt` lo detecta por su nombre al recompilar y lo aplica a las etiquetas técnicas.

## Alcance

La aplicación declara Internet, estado de red/Wi-Fi, multicast y cámara opcional para el lector QR. Rust/JNI mantiene identidad y confianza dentro del almacenamiento privado de la app, excluido de copias y transferencias de datos. No existe todavía cifrado con Android Keystore, servicio en primer plano, MediaProjection, CameraX, AudioRecord, SAF ni integración con portapapeles del sistema.

En el recorrido normal los eventos nativos determinan la conexión. Los controles de pantalla y servicios informan que falta su integración de plataforma. Los temporizadores, equipos y transferencias ilustrativos permanecen solamente en la fixture visual.

Los únicos XML son recursos Android de tema/icono y el manifiesto; todas las pantallas son Compose.

## Verificación

Con un emulador o dispositivo conectado:

```powershell
.\gradlew.bat :app:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=com.tyu.app.TyuJourneyTest
```

`TyuJourneyTest.kt` verifica la fixture visual, error/reintento, cancelación, olvido, servicios y recreación/orientación. `NativeConnectionTest.kt` verifica la conexión nativa contra Desktop y requiere una invitación vigente y aprobación real; sus instrucciones están en la guía de conexión. Solo el código de pruebas escribe capturas en el directorio de resultados del dispositivo.

Los informes se generan en `app/build/reports`. Las capturas de la revisión se entregan en `docs/screenshots`.
