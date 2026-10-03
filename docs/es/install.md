# Instalar Maris

La versión 0.1.0 distribuye archivos CLI/TUI precompilados sin afirmar que tienen firma del editor. El instalador valida el ejecutable nativo, el manifiesto de publicación y SHA-256; la firma de aplicaciones GUI pertenece a otro contrato de distribución. Los auriculares físicos, Bluetooth y la aceptación auditiva subjetiva siguen sin verificar. Si el proyecto no declara licencia, el archivo registra ese estado sin añadir una autorización de código abierto.

El instalador descarga el programa ya compilado para tu equipo. No necesitas el código fuente, Rust, Cargo, Xcode ni un compilador C++. Compilar por tu cuenta es una opción aparte para desarrolladores. Si la descarga falla, la instalación se detiene.

> Debe existir una versión estable publicada y aprobada. Maris sigue en desarrollo. Una versión inexistente, un fallo de red o un candidato detiene la instalación sin modificar la aplicación existente.

## Instalación en línea {#online}

macOS y Linux:

```sh
curl -fsSL https://maris.francis.run/install.sh | bash
```

Windows: usa PowerShell con permisos normales.

```powershell
irm https://maris.francis.run/install.ps1 | iex
```

Revisa el script y confía solo en la fuente del proyecto y el editor aprobado. No desactives las políticas de scripts, firma o cuarentena del sistema. El instalador no añade compiladores, controladores, servicios, inicio automático ni modelos.

## Versión, destino y simulación {#options}

```sh
bash install.sh --dry-run
bash install.sh --version v0.1.0 --yes
bash install.sh --prefix "$HOME/Audio Tools" --yes
```

```powershell
.\install.ps1 -DryRun
.\install.ps1 -Version v0.1.0 -Yes
.\install.ps1 -Prefix "$env:LOCALAPPDATA\Programs\Audio Tools" -Yes
```

Sustituye el ejemplo por una etiqueta publicada. Latest se resuelve una sola vez; las demás descargas quedan fijadas a esa versión. La simulación en línea no accede a la red ni escribe archivos. Sin la opción de confirmación automática se conserva la pregunta antes de instalar.

| Sistema | Arquitectura | Destino predeterminado | Herramientas existentes |
| --- | --- | --- | --- |
| macOS | x86_64 / ARM64; detecta Rosetta | `~/.local/lib/maris; ~/.local/bin/maris` | Bash, curl, tar, gzip, SHA-256 |
| Linux | x86_64 / ARM64 | `~/.local/lib/maris`; lanzador propio `~/.local/bin/maris` | Bash, curl, tar, gzip, SHA-256, bibliotecas de ejecución; psmisc para actualizar |
| Windows | x86_64 / ARM64 | `%LOCALAPPDATA%/Programs/Maris` | PowerShell 5.1+ y funciones HTTP/ZIP existentes de .NET |

Las tres rutas de audio del sistema están implementadas: CoreAudio process tap en macOS, WASAPI process loopback en Windows y un servicio local compatible con PulseAudio —incluido PipeWire Pulse— en Linux. CI, equipos limpios, dispositivos reales y pruebas prolongadas siguen siendo validaciones independientes por plataforma.

## Verificación antes de reemplazar {#verification}

Solo se aceptan la dirección fija del proyecto, un manifiesto estable completo y el objetivo nativo adecuado. Se rechazan candidatos, objetivos duplicados, versiones incorrectas, transferencias truncadas y tamaños o SHA-256 del archivo/binario que no coincidan. Hay límites de HTTPS, redirecciones, tiempo y tamaño.

Los archivos deben quedar dentro de la raíz del paquete. Se rechazan rutas que escapan, nombres duplicados o que colisionan por mayúsculas, enlaces, entradas especiales y cantidades o tamaños excesivos. Los datos del paquete, la versión del programa y el SHA-256 del ejecutable deben coincidir con la lista de descarga antes de reemplazar la aplicación instalada.

CLI/TUI valida el manifiesto publicado, plataforma, arquitectura, versión, tamaño, rutas y SHA-256. Rechaza candidatos, identidades GUI/CLI confundidas y archivos modificados. Los paquetes GUI mantienen por separado las comprobaciones de firma/Gatekeeper de macOS y Authenticode de Windows. Las sumas por sí solas no demuestran identidad del editor ni calidad auditiva.

## Actualizar, volver atrás y abrir {#recovery}

Ejecuta el mismo instalador para futuras versiones aprobadas. Cierra Maris primero; no se terminan procesos de audio. Un bloqueo, preparación en el mismo sistema de archivos y copia `.maris-backup.*` protegen el cambio. Los fallos manejables intentan restaurar la versión anterior. Preferencias, corrección, archivos ajenos y PATH se conservan.

Para volver atrás, elige una etiqueta estable anterior o usa un kit sin conexión completo y la copia con `--from` / `-From`. No elimines bloqueos activos. Tras pérdida de energía o cierre forzado, inspecciona primero las rutas conservadas.

Usa el comando impreso por el instalador; en macOS/Linux puedes añadir `~/.local/bin` a tu PATH. `maris` abre la TUI y `maris --help` muestra comandos. macOS puede pedir permiso de grabación para el terminal al iniciar la captura; instalar no concede permisos. Cerrar la TUI no detiene una sesión existente: usa `maris stop`.

## CI y publicación pública {#ci}

Las comprobaciones de push no publican la aplicación. La compilación manual separada produce seis objetivos nativos y kits candidate. Los paquetes de prueba de Actions necesitan permisos de acceso y caducan. Las descargas normales se ofrecen como paquetes revisados en GitHub Releases.

El manifiesto CLI identifica el contrato de distribución. Los seis archivos deben coincidir en ejecución CI nativa, fuente y versión. La instalación, las actualizaciones y 200 rondas aportan evidencia de software; la aceptación física es independiente. Las firmas son requisitos de la distribución GUI.

## Opciones sin conexión y de desarrollo {#development}

Los kits locales completos usan `--from` / `-From`. Solo `--build` / `-Build` explícito requiere Rust 1.94 y el entorno de compilación. `--allow-unsigned` / `-AllowUnsigned` es para desarrollo local confiable, no para saltarse la verificación en línea. Los auxiliares de instalación local no usan red.

## Resolver errores {#troubleshooting}

No publicado o error HTTP: no se instala ni compila. Objetivo o versión incorrectos: usa una etiqueta estable nativa válida. Error de hash o archivo: detén e investiga la procedencia, sin omitir controles. Instalación ocupada: revisa Maris o el bloqueo. No reduzcas la seguridad del sistema ante rechazos de firma o scripts.

Consulta el [estado de publicación](status.md) y el procedimiento técnico en inglés de [compilación y publicación](../development/releasing.md).
