# Freigabestatus

Maris ist noch in Entwicklung. Die Liste unten nutzt dieselben Daten wie die Veröffentlichungskontrolle. Umsetzung, automatische Tests und Gerätetests werden getrennt erfasst.

## Erforderliche Arbeiten {#scope}

Offene Punkte betreffen Funktionen, Installation, Oberfläche, Dokumentation und gemeldete Fehler. Neue Probleme müssen ebenfalls geprüft werden. Ein Punkt darf nicht einfach aus der Liste verschwinden, um früher zu veröffentlichen.

## Automatische Tests {#checks}

Die Tests prüfen Audioverarbeitung, gespeicherte Einstellungen, Tastatur und Maus, Rückgängig-Funktionen und Installation. Die 200 Runden kombinieren unterschiedliche Testdaten mit wechselnden zugehörigen Tests. Das sind weder 200 vollständige Prüfungen noch Hörtests. Codeänderungen brauchen neue Ergebnisse; übersprungene oder unterbrochene Tests zählen nicht als bestanden.

## Geräte und Installationspakete {#native}

macOS, Windows und Linux brauchen jeweils eigene Build- und Testergebnisse. Kopfhörer, Lautsprecher, Bluetooth, getrennte Geräte und Änderungen der Stromversorgung müssen auch praktisch geprüft werden. Öffentliche Pakete benötigen passende Testergebnisse, Geräteprüfungen, Lizenzprüfung und bestätigte Signaturen. Der normale Installer nimmt keine Entwicklungskandidaten an.

## Veröffentlichung {#publication}

Quellcode hochladen, Pakete bauen, die Website aktualisieren und die App veröffentlichen sind getrennte Vorgänge. Ein erfolgreicher Upload bedeutet nicht, dass alle Tests bestanden sind oder ein Download bereitsteht. Siehe [Build und Veröffentlichung](../development/releasing.md).
