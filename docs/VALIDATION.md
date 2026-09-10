# Verificación TYU

Fecha: 10 de septiembre de 2026.

## Artefacto

- APK: [app-debug.apk](../app/build/outputs/apk/debug/app-debug.apk)
- Paquete: `com.tyu.app`, versión `0.1.0`.
- APK de desarrollo firmada: 16,881,685 bytes (16.10 MiB).
- SHA-256: `C0731261A4297BBE7D99C314C2BAAD20C612E878FD5B6C4AA7075DADD771B828`.
- Android mínimo declarado: API 26. La ejecución se verificó en Android 16 / API 36; no se probó físicamente en todas las versiones anteriores.

## Resultados

| Comprobación | Resultado |
| --- | --- |
| `:app:assembleDebug` | Correcto |
| `:app:assembleDebugAndroidTest` | Correcto |
| `:app:lintDebug` | 0 errores, 7 avisos |
| Instalación de la APK mediante ADB | Correcta |
| Cuatro pruebas instrumentadas, APK final | 4 aprobadas, 0 fallidas; 43.722 s |
| Recorrido completo compacto, fuente al 130 % | 1 aprobado, 0 fallidos; 26.279 s |
| Archivos Kotlin de producción/previews | 86, ninguno vacío |
| Capturas Android | 26 normales/horizontales + 21 compactas |

Los siete avisos de lint corresponden a cinco sugerencias de actualización de herramientas/dependencias, la detección por nombre de la fuente opcional y la recomendación de reglas de extracción de backups. No se desactivó lint ni se añadió un baseline para ocultar errores.

Las pruebas se ejecutaron inicialmente con `connectedDebugAndroidTest` y se repitieron sobre la APK final mediante el runner instrumentado. El runner directo permite recuperar las capturas antes de que el flujo Gradle desinstale la aplicación de prueba.

## Cobertura

- Welcome, permisos visuales, estado vacío, búsqueda y dispositivos encontrados.
- Error al conectar PC X y reintento exitoso.
- QR visual, cancelar y continuar.
- Monitor: opciones seleccionadas, activar, detener y controles en horizontal.
- Espejo: preparación, activo y detener.
- Bypass: cámara y micrófono independientes, almacenamiento y transferencias.
- Deshabilitar categorías al desactivar almacenamiento.
- Deshabilitar envío sin selección; elegir ejemplos; cola 100 % / 53 % / esperando; completar.
- Actividad coherente con cámara y micrófono activos.
- Detalles de equipo, IP y olvido con confirmación/cancelación.
- Configuración, recibidos vacíos y retorno desde permisos.
- Conservación de conexión y orientación seleccionada al recrear la actividad.

## Revisión visual

Se revisaron capturas renderizadas en un AVD Pixel 6, Android 16, 1080 × 2400 y 2400 × 1080; además, se recorrió la aplicación en 720 × 1280 a 360 dpi (320 dp de ancho), con escala de fuente 1.3.

Se ajustó el encabezado conectado para mostrar nombre/estado en horizontal y se separaron los controles del monitor activo para evitar superposición en alturas reducidas. Las pantallas extensas utilizan desplazamiento; en las capturas compactas puede verse solo la parte inicial del contenido. La prueba recorre también el contenido inferior.

Ejemplos:

- [Welcome](screenshots/01-welcome.png)
- [Permisos](screenshots/02-permissions.png)
- [Conectado](screenshots/06-connected.png)
- [Monitor](screenshots/07-monitor.png)
- [Espejo](screenshots/09-mirror.png)
- [Bypass](screenshots/11-bypass.png)
- [Transferencia](screenshots/18-transfer-progress.png)
- [Conectado horizontal](screenshots/26-connected-landscape.png)
- [Conectado compacto, texto grande](screenshots/compact/06-connected.png)

## Alcance de la APK

La inspección de permisos con `aapt dump permissions` muestra únicamente el permiso interno de firma `com.tyu.app.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION`, generado por AndroidX. No hay permisos de Internet, red, Bluetooth, cámara, micrófono ni almacenamiento.

La revisión de los fuentes de producción no encontró APIs de red, captura, MediaProjection, solicitudes de permisos ni servicios Android. El emulador creado para la verificación se llama `TYU_UI_Preview`; sus ajustes temporales de tamaño/densidad/fuente se restauraron al terminar.
