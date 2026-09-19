# TYU Microphone

## Estado comprobado

El código propio sustituye completamente el adaptador VB-CABLE. Android captura PCM S16LE mono de 48 kHz; TYU Core lo transporta por la sesión QUIC autenticada de Bypass y Desktop lo entrega por IOCTL al controlador WaveRT. El nombre de entrada previsto es **TYU Microphone**. Mirror, Monitor y la configuración de cámara no cambian.

El paquete **no está firmado ni instalado**. Compilarlo no demuestra que Windows cree el endpoint ni que una aplicación reciba audio. Esas pruebas, Driver Verifier y la validación de hardware siguen pendientes. No se puede declarar el micrófono funcional en este equipo todavía.

Validación local: 55 pruebas Rust aprobadas; formato y Clippy del workspace correctos; Desktop compilado y `--smoke` aprobado. APK debug compilado y Android lint con 0 errores y 7 advertencias. Compilación nativa con análisis, pruebas PCM e INF/catálogo correctos. `TyuMicProbe` devuelve 2 (controlador ausente) y SignTool devuelve 1 (`No signature found`): no son pruebas de audio aprobadas. Los hashes de los archivos protegidos de Mirror/Monitor coinciden con su estado anterior.

## Compilación

Requiere Windows x64, herramientas C++ de Visual Studio, SDK 10.0.26100.0 y WDK 26100. El script puede descargar el paquete WDK oficial de Microsoft fijado a 10.0.26100.6584 dentro de `target`; no instala herramientas globales.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/build-tyu-microphone.ps1 -RestoreWdk -Analyze
cargo build -p tyu-desktop
```

Salida: `target/tyu-microphone/{TyuMicrophone.sys,TyuMicrophone.inf,TyuMicrophone.cat,TyuMicInstall.exe,TyuMicProbe.exe}`. Compilar de nuevo regenera el catálogo; no recompilar un paquete ya firmado para instalarlo.

La compilación ejecuta pruebas de muestras con signo, tamaño inválido, vuelta del búfer, saturación y vaciado. Usa `/W4 /WX`, análisis MSVC opcional, InfVerif y generación de catálogo con Inf2Cat. Dos supresiones locales C28197 documentan la retención COM de los miniports por PortCls; el análisis no sigue ese conteo de referencias.

## Firma e instalación

Los controladores nuevos de kernel requieren firma de Microsoft para la distribución normal en Windows moderno. El catálogo local generado por Inf2Cat no es una firma. Hace falta una cuenta de desarrollador de hardware y completar el proceso aplicable en Hardware Dev Center; ese acceso no está disponible en esta sesión. Referencia: [requisitos oficiales de firma de kernel](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/kernel-mode-code-signing-requirements--windows-vista-and-later-).

Primero se debe probar el controlador en un entorno Windows de laboratorio con depuración de kernel y recuperar/corregir cualquier fallo antes de distribuirlo. No se ha habilitado test signing, desactivado Secure Boot ni instalado certificados en el PC del usuario.

Una vez disponible un paquete firmado y validado, desde PowerShell con administrador:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/install-tyu-microphone.ps1 -Package C:\ruta\paquete-firmado
```

El script verifica SYS e INF contra el catálogo usando la política de firma de kernel. Si no pasa, termina antes de registrar el dispositivo. El instalador nativo sólo crea/actualiza `Root\TYUMicrophone`; `-Uninstall` retira ese dispositivo. Un reinicio solicitado por Windows se informa y no se ejecuta automáticamente.

## Aceptación pendiente

1. Instalar el paquete validado y confirmar que Administrador de dispositivos no tiene errores y que Configuración de sonido ofrece TYU Microphone.
2. Con el micrófono Bypass apagado, ejecutar `target/tyu-microphone/TyuMicProbe.exe`: abre el controlador, introduce una señal conocida y comprueba muestras a través de un cliente WASAPI real. Después exige silencio tras RESET. Sin controlador devuelve error; nunca simula un resultado positivo. La señal de prueba sólo existe en este ejecutable.
3. Conectar un teléfono físico por WLAN, conceder RECORD_AUDIO y activar Micrófono en Bypass. Seleccionar TYU Microphone en una grabadora y verificar voz, continuidad y latencia.
4. Comprobar cierre/reapertura de la grabadora, desconexión/reconexión, suspensión/reanudación y encendido/apagado del servicio. Tras detenerlo debe haber silencio, sin repetir audio previo.
5. Ejecutar Driver Verifier y las pruebas de audio/energía/instalación aplicables en laboratorio. La compilación y el análisis estático no sustituyen esas pruebas.

## Contrato y límites

`protocol.h` define versión 1, PCM 48 kHz/mono/16 bits e IOCTL INFO/WRITE/RESET. El adaptador Rust valida la versión, formato y estado antes de escribir. Un único productor abre el dispositivo; la ACL permite SYSTEM, administradores y usuarios interactivos locales. No hay acceso de red al dispositivo de control.

El búfer de audio tiene capacidad de 100 ms; descarta audio anterior al saturarse para limitar el retraso y produce silencio al agotarse. El temporizador alimenta el búfer de captura y sus notificaciones. Cierre, RESET y parada vacían las muestras pendientes. WASAPI compartido debe permitir a Windows servir el endpoint a varias aplicaciones; el miniport admite un stream de captura del motor de audio. Estas propiedades de ejecución todavía necesitan la validación anterior.
