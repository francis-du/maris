# Maris installieren

Der Installer lädt das fertige Programmpaket für deinen Computer. Du brauchst weder den Quellcode noch Rust, Cargo, Xcode oder einen C++-Compiler. Selbst kompilieren ist eine separate Entwickleroption. Wenn der Download fehlschlägt, stoppt die Installation.

> Eine veröffentlichte und abgenommene stabile Version muss vorhanden sein. Maris befindet sich noch in Entwicklung. Fehlende Versionen, Netzfehler und Kandidaten stoppen die Installation, ohne die bestehende Anwendung zu verändern.

## Online installieren {#online}

macOS und Linux:

```sh
curl -fsSL https://francis-du.github.io/maris/install.sh | bash
```

Windows: PowerShell als normaler Benutzer verwenden.

```powershell
irm https://francis-du.github.io/maris/install.ps1 | iex
```

Prüfe das Skript und vertraue nur der Projektquelle und dem abgenommenen Herausgeber. Skript-, Signatur- und Quarantäneschutz des Betriebssystems nicht umgehen. Es werden keine Compiler, Treiber, Dienste, Autostarts oder Modelle installiert.

## Version, Ziel und Vorschau {#options}

```sh
bash install.sh --dry-run
bash install.sh --version v1.2.3 --yes
bash install.sh --prefix "$HOME/Audio Tools" --yes
```

```powershell
.\install.ps1 -DryRun
.\install.ps1 -Version v1.2.3 -Yes
.\install.ps1 -Prefix "$env:LOCALAPPDATA\Programs\Audio Tools" -Yes
```

Ersetze das Beispiel durch einen veröffentlichten Tag. Latest wird einmal aufgelöst, alle weiteren Downloads bleiben an diese Version gebunden. Die Online-Vorschau greift weder aufs Netz zu noch schreibt sie Dateien. Ohne Bestätigungsoption bleibt die interaktive Zustimmung erhalten.

| System | Architektur | Standardziel | Vorhandene Laufzeitwerkzeuge |
| --- | --- | --- | --- |
| macOS | x86_64 / ARM64, Rosetta-Erkennung | `~/Applications/Maris.app` | Bash, curl, tar, gzip, SHA-256, Apple-Signaturwerkzeuge |
| Linux | x86_64 / ARM64 | `~/.local/lib/maris`, eigener Starter `~/.local/bin/maris` | Bash, curl, tar, gzip, SHA-256, Laufzeitbibliotheken; psmisc bei Updates |
| Windows | x86_64 / ARM64 | `%LOCALAPPDATA%/Programs/Maris` | PowerShell 5.1+ und vorhandene .NET-HTTP-/ZIP-Funktionen |

Alle drei System-Audiowege sind implementiert: macOS nutzt den CoreAudio Process Tap, Windows WASAPI Process Loopback und Linux einen lokalen PulseAudio-kompatiblen Dienst einschließlich PipeWire Pulse. CI, saubere Systeme, reale Geräte und Langzeittests bleiben plattformspezifische Abnahmen.

## Prüfung vor dem Austausch {#verification}

Akzeptiert werden nur die feste Projektadresse, ein vollständiges stabiles Manifest und das passende native Ziel. Kandidaten, doppelte Ziele, Versionsfehler, unvollständige Transfers sowie falsche Längen oder Archiv-/Binär-SHA-256 werden abgelehnt. HTTPS, Weiterleitungen, Dauer und Datenmenge sind begrenzt.

Archivpfade müssen unter dem vorgesehenen Paketverzeichnis bleiben. Traversal, doppelte oder groß-/kleinschreibungsbedingt kollidierende Namen, Links, Spezialdateien und übermäßige Größen/Dateianzahlen werden verworfen. Paketangaben, Programmversion und SHA-256 des Programms müssen mit der Downloadliste übereinstimmen, bevor die installierte App ersetzt wird.

macOS behält Signatur- und Gatekeeper-Prüfung, Windows Authenticode, Linux den manifestgebundenen Binärhash. Eine Prüfsumme bestätigt Bytes, nicht unabhängige Herausgeberidentität oder Hörqualität.

## Aktualisierung, Rückkehr und Start {#recovery}

Derselbe Installer lädt spätere freigegebene Versionen. Beende Maris bewusst vorher; laufende Audioprozesse werden nicht beendet. Sperre, Staging auf demselben Dateisystem und eine `.maris-backup.*`-Kopie sichern den Austausch. Bei behandelbaren Fehlern wird Wiederherstellung versucht. Einstellungen, Korrektur, fremde Dateien und PATH bleiben erhalten.

Zur Rückkehr einen früheren stabilen Tag wählen oder das Backup mit einem vollständigen Offline-Kit über `--from` / `-From` installieren. Keine aktive Sperre löschen. Nach Stromausfall oder erzwungenem Abbruch zuerst die verbliebenen Pfade prüfen.

Die Installation startet kein Audio und ändert weder Systemlautstärke noch Standardausgabe. Auf macOS Maris.app bewusst öffnen und autorisieren; sonst zuerst `--help` und explizite Geräte-/Offlinefunktionen ansehen. Eine noch laufende alte Programmversion wird nicht automatisch ausgetauscht.

## CI und öffentliche Pakete {#ci}

Push-Prüfungen veröffentlichen keine Anwendung. Die separate manuelle CI erstellt sechs native Ziele und als candidate markierte Kits. Testpakete aus Actions benötigen Zugriffsrechte und laufen ab. Normale Downloads stehen als geprüfte Installationspakete unter GitHub Releases bereit.

Das Manifest heißt `maris-release.tsv`; Unix-Kits verwenden `.tar.gz`, Windows `.zip`. Stabile Kits brauchen die native Signatur-/Abnahmeprüfung auch nach dem Entpacken. Alle sechs Ziele müssen dieselbe Version und denselben Quellstand haben. 200 Runden ersetzen keine offene Anforderung.

## Offline und Entwicklung {#development}

Vollständige lokale Kits verwenden `--from` / `-From`. Nur explizites `--build` / `-Build` benötigt Rust 1.94 und die jeweilige Build-Umgebung. `--allow-unsigned` / `-AllowUnsigned` gilt ausschließlich für bewusst vertraute lokale Entwicklung, nicht zum Umgehen der Onlineprüfung. Offline-Helfer arbeiten ohne Netzwerk.

## Fehler behandeln {#troubleshooting}

Nicht veröffentlicht oder HTTP-Fehler: keine Installation und kein Kompilieren. Falsche Version/Architektur: korrekten nativen Tag wählen. Hash- oder Archivfehler: stoppen und Herkunft untersuchen, nicht umgehen. Belegte Installation: Maris oder Sperre prüfen. Signatur-/Skriptrichtlinienfehler nicht durch Abschalten des OS-Schutzes beheben.

Siehe [Freigabestatus](status.md) sowie die englischen [Build- und Veröffentlichungsschritte](../development/releasing.md).
