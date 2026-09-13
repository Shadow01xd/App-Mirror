# TYU

<!-- impeccable:product-schema 1 -->

## Platform

android

## Product Purpose

Interfaz nativa Kotlin / Jetpack Compose que representa el enlace entre teléfono y PC. Mantener el recorrido actual es un requisito explícito del rediseño.

## Capabilities and Constraints

La conexión normal utiliza TYU Core compartido mediante Rust/JNI: búsqueda LAN y conexión al tocar el equipo, o QR escaneado; ambas formas solicitan aprobación en Desktop. No se exige QR al conectar por búsqueda y no hay campo para pegar enlaces. Se mantiene QUIC/TLS, identidad y confianza persistentes, reconexión y desconexión. Se conserva Compose. Cámara se solicita únicamente para escanear el QR; se añaden permisos de red y multicast. Los adaptadores de pantalla, cámara como servicio, micrófono, SAF y portapapeles del sistema aún no existen y no muestran estados activos simulados en el recorrido normal.

Flujo existente: bienvenida, permisos visuales, búsqueda o QR, conexión/error/reintento, dispositivo conectado, modos, servicios, actividad, detalles y configuración. Monitor representa PC → teléfono; Espejo, teléfono → PC; Bypass, teléfono ↔ PC sin compartir pantalla. Los controles y estados se conservan al volver y al recrear la actividad.

## Brand Commitments

TYU. Conservar negro, superficies oscuras y amarillo #F6CC45. El usuario autoriza reemplazar el resto del diseño y añadir animaciones pequeñas para una experiencia premium. Construcción directa en Compose y comprobación en Android, elegidas explícitamente.

## Users

Inferido del recorrido existente: personas que usan teléfono y PC juntos y eligen un modo de pantalla o un servicio. No se ha definido un segmento comercial más específico.

## Evidence on Hand

README.md, app/backend/, ../core/, model/, feature/, NativeConnectionTest.kt y TyuJourneyTest.kt. Los ejemplos de mock/ permanecen solo en previews y en el recorrido visual explícito de depuración (`tyu.fixture=true`). Consultar ../docs/BACKEND_STATUS.md para la validación real.

## Accessibility & Inclusion

Conservar navegación atrás de Android, controles de al menos 48 dp, contenido desplazable, ampliación de fuente y orientación horizontal. Animaciones compatibles con el ajuste de duración del sistema.
