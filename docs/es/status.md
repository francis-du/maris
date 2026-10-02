# Estado de publicación

Maris sigue en desarrollo. La lista de abajo usa los mismos datos que las comprobaciones de publicación. La implementación, las pruebas automáticas y las pruebas con dispositivos se registran por separado.

## Trabajo necesario {#scope}

Los puntos pendientes incluyen funciones, instalación, interfaz, documentación y errores comunicados. Los problemas nuevos también deben comprobarse; borrar un punto no permite publicar antes.

## Pruebas automáticas {#checks}

Se comprueban el procesamiento de audio, los ajustes guardados, el teclado y el ratón, deshacer e instalación. Las 200 rondas combinan datos de prueba diferentes con pruebas relacionadas. No son 200 revisiones completas ni sustituyen las pruebas de escucha. Cambiar el código exige resultados nuevos. Las pruebas omitidas o interrumpidas no cuentan como aprobadas.

## Dispositivos y paquetes {#native}

macOS, Windows y Linux necesitan sus propias compilaciones y pruebas. Auriculares, altavoces, Bluetooth, desconexiones y cambios de alimentación también requieren pruebas físicas. Los paquetes públicos necesitan resultados de la misma versión, pruebas de dispositivos, revisión de licencias y verificación de firmas. El instalador normal no acepta candidatos de desarrollo.

## Publicación {#publication}

Subir el código, compilar paquetes, actualizar la web y publicar la aplicación son operaciones distintas. Una subida correcta no significa que todas las pruebas hayan pasado ni que exista una descarga pública. Consulta [compilación y publicación](../development/releasing.md).
