# TYU

<!-- impeccable:product-schema 1 -->

## Platform

android

## Product Purpose

Interfaz nativa Kotlin / Jetpack Compose que representa el enlace entre teléfono y PC. Mantener el recorrido actual es un requisito explícito del rediseño.

## Capabilities and Constraints

El frontend actual utiliza exclusivamente estado local de demostración. No se añaden permisos, conexiones, backend, almacenamiento real ni captura de hardware.

Flujo existente: bienvenida, permisos visuales, búsqueda o QR, conexión/error/reintento, dispositivo conectado, modos, servicios, actividad, detalles y configuración. Monitor representa PC → teléfono; Espejo, teléfono → PC; Bypass, teléfono ↔ PC sin compartir pantalla. Los controles y estados se conservan al volver y al recrear la actividad.

## Brand Commitments

TYU. Conservar negro, superficies oscuras y amarillo #F6CC45. El usuario autoriza reemplazar el resto del diseño y añadir animaciones pequeñas para una experiencia premium. Construcción directa en Compose y comprobación en Android, elegidas explícitamente.

## Users

Inferido del recorrido existente: personas que usan teléfono y PC juntos y eligen un modo de pantalla o un servicio. No se ha definido un segmento comercial más específico.

## Evidence on Hand

README.md, mock/, model/, feature/, navegación local y pruebas TyuJourneyTest.kt. Las cifras, dispositivos y archivos son ejemplos existentes, no capacidades operativas verificadas.

## Accessibility & Inclusion

Conservar navegación atrás de Android, controles de al menos 48 dp, contenido desplazable, ampliación de fuente y orientación horizontal. Animaciones compatibles con el ajuste de duración del sistema.
