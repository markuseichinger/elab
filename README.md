# elab – Elektronik-Lernlabor

Schaltungen bauen, simulieren und beim Arbeiten zusehen: ein Lernprogramm für
Elektronik in Rust, als Gegenstück zu [nnlab](https://github.com/markuseichinger/nnlab).

![elab mit Einweggleichrichter, Tutorial und Oszilloskop](docs/screenshot.png)

![Digitaltechnik: Zähler mit 7-Segment-Anzeige und Logikanalysator](docs/screenshot_digital.png)

> **Stand: Phase 2** – Analog- und Digitaltechnik: Schaltplan-Editor mit Live-Simulation,
> Oszilloskop, Logikanalysator, Wahrheitstabelle und Tutorial.

## Starten

```bash
cargo run --release                 # startet mit dem Beispiel „Spannungsteiler“
cargo run --release -- datei.elab   # öffnet eine gespeicherte Schaltung
cargo test --workspace              # alle Tests (Simulator, Netzliste, Modell)
```

## Was die App kann

- **Schaltplan-Editor** auf einem Raster: Bauteile per Drag & Drop aus der Palette setzen,
  drehen (auch während des Ziehens), verschieben, löschen. Drähte vom Anschluss oder von
  jeder Stelle einer Leitung aus ziehen (verlängern, abzweigen) oder Ecke für Ecke verlegen.
  Angeschlossene Drähte gehen beim Verschieben mit und werden um Bauteile herum verlegt.
- **Einfügen in Leitungen:** Ein Bauteil, das auf eine Leitung gesetzt wird, wird eingefügt –
  die Leitung unter dem Bauteil verschwindet, statt es kurzzuschließen. Quer liegende Bauteile
  drehen sich automatisch passend (grüne Vorschau). Überbrückte Bauteile werden gemeldet.
- **Live-Simulation:** Nach jeder Änderung wird der Arbeitspunkt neu berechnet.
  Drähte färben sich nach ihrer Spannung (grau 0 V, orange positiv, blau negativ),
  LEDs und Glühlampen leuchten entsprechend ihrem Strom bzw. ihrer Leistung.
- **Infofenster** beim Überfahren: Spannung, Strom, Leistung, Netzname, Leitzustand der Diode.
- **Inspektor:** Werte mit SI-Präfix eingeben (`4k7`, `100n`, `2,2 mA`, `1M`) und
  live sehen, wie U, I und P reagieren, inkl. Ohm-Probe und Warnungen (¼-W-Grenze, LED-Überstrom).
- **Oszilloskop:** Zeitverlauf an allen Messpunkten, Zeitbasis 100 µs … 1 s,
  automatische Neumessung nach jeder Änderung.
- **Tutorial-Leiste** wie in nnlab: Erklärung, Formeln, übliche Werte und Experimente
  zu jedem Bauteil und zu den Grundlagen (Knoten, Masse, Ohm, Kirchhoff, Arbeitspunkt …),
  mit Live-Werten des ausgewählten Bauteils.
- **Verständliche Fehler:** fehlende Masse, kurzgeschlossene Quellen, doppelte Namen,
  nicht verbundene Bauteile (werden markiert und übersprungen statt die Simulation abzubrechen).
- **Digitaltechnik** mit ereignisgesteuerter Simulation in Echtzeit: vierwertige Logik
  (0, 1, X, Z), Gatterlaufzeiten, Glitches, Buskonflikte, Dauerschwingungen werden erkannt.
  Drähte zeigen ihren Pegel farbig, Schalter per Klick, Taster per Gedrückthalten.
- **Logikanalysator** mit Zeitlupe und Pause, **Wahrheitstabelle** für Schaltnetze
  (aktuelle Zeile markiert, Klick auf eine Zeile stellt die Schalter ein).
- **Undo/Redo** für jede Änderung, **Speichern/Laden** als `.elab` (JSON),
  **elf Beispielschaltungen** (sechs analog, fünf digital).

### Bauteile

**Analog:** Gleich-, Wechsel- und Rechteckspannung, Stromquelle, Widerstand, Kondensator,
Spule, Glühlampe, Diode (1N4148, 1N4007, Lehrbuch), LED (rot, gelb, grün, blau, weiß),
Schalter, Masse, Messpunkt.

**Digital:** UND, ODER, NICHT, NAND, NOR, XOR, XNOR, Logikschalter, Taster, Taktgeber,
fester Pegel 1, Logik-Anzeige, 7-Segment-Anzeige (hex), D-Flipflop, JK-Flipflop,
4-Bit-Zähler. Masse wirkt an Digitalnetzen als Pegel 0.

Schaltzeichen nach IEC 60617 (Gatter als Rechtecksymbole mit &, ≥1, =1).

### Bedienung

| Aktion | Eingabe |
|---|---|
| Bauteil platzieren | aus der Palette ziehen · oder anklicken und absetzen (`⇧`-Klick: mehrere) |
| In Leitung einfügen | Bauteil auf die Leitung setzen oder schieben (grüne Vorschau) |
| Draht | vom Anschluss oder von einer Leitung aus ziehen · oder `W`, Klick je Ecke, Doppelklick/`Esc` beendet, Leertaste wechselt den Knick |
| Drehen / Löschen | `R` (auch beim Ziehen) / `Entf` oder `⌫` · `Esc` bricht Verschieben ab |
| Schalter umlegen | Doppelklick · Logikschalter: Klick · Taster: gedrückt halten |
| Mehrfachauswahl | Rahmen ziehen, `⇧`-Klick, `⌘A` |
| Ansicht | Zwei-Finger-Wischen verschiebt, Pinch oder `⌘`+Mausrad zoomt, „⤢ Einpassen“ |
| Rückgängig / Wiederholen | `⌘Z` / `⇧⌘Z` |
| Neu / Öffnen / Speichern | `⌘N` / `⌘O` / `⌘S` |

## Aufbau

```
elab/
├── crates/elab-sim/      Simulationskern ohne UI (Phase 0)
│   ├── src/analysis.rs   MNA, Newton-Raphson, DC-Arbeitspunkt, Transientenanalyse
│   ├── src/circuit.rs    Schaltungsmodell und Fehlermeldungen
│   ├── src/diode.rs      Shockley-Modell mit pn-Spannungsbegrenzung
│   ├── src/digital.rs    Ereignisgesteuerte Logiksimulation (0/1/X/Z, Laufzeiten)
│   ├── src/waveform.rs   DC, Sinus, Puls/Rechteck, stückweise linear
│   └── tests/            Referenztests gegen analytische Lösungen
└── crates/elab-app/      Oberfläche (egui/eframe 0.37, wie nnlab)
    ├── src/model.rs      Schaltplan: Bauteile, Drähte, Raster, Undo
    ├── src/netlist.rs    Schaltplan → Netze → Simulator-Schaltung
    ├── src/sim.rs        Arbeitspunkt, Oszilloskop, Wahrheitstabelle, SI-Eingabe
    ├── tests/components.rs  jedes Bauteil eingefügt und gegen Handrechnung geprüft
    ├── src/examples.rs   Beispielschaltungen
    ├── src/editor.rs     Zeichenfläche, Werkzeuge, Live-Darstellung
    ├── src/panels.rs     Logikanalysator, Wahrheitstabelle, Echtzeit-Digitalbetrieb
    ├── src/symbols.rs    Schaltzeichen
    ├── src/tutorial.rs   Erklärtexte
    ├── src/script.rs     Testskripte für automatisierte Oberflächentests
    └── src/main.rs       Fenster, Panels, Inspektor, Oszilloskop, Dateien
```

## Oberflächentests

Die App lässt sich per Skript fernsteuern. Echte Maus- und Tastaturereignisse werden
eingespielt, Screenshots und der Dokumentzustand werden gespeichert:

```bash
cargo run --release -- --script crates/elab-app/ui-tests/schaltung_bauen.txt
cargo run --release -- --script crates/elab-app/ui-tests/einfuegen.txt
cargo run --release -- --script crates/elab-app/ui-tests/digital_bauen.txt
```

- `schaltung_bauen`: LED-Schaltung von Grund auf, Verdrahten, Verschieben, Drehen, Undo
- `einfuegen`: Bauteile per Klick und Drag & Drop in Leitungen einfügen, Leitung verlängern
- `digital_bauen`: Gatterschaltung bauen, schalten, Buskonflikt, Analog/Digital-Mischung

Befehle: siehe `src/script.rs`.

## Fahrplan

| Phase | Inhalt |
|---|---|
| **0** ✅ | Analogkern + Referenztests |
| **1** ✅ | Schaltplan-Editor, Live-Arbeitspunkt, Tutorial, Oszilloskop, Speichern/Laden |
| **2** ✅ | Digital-Engine (0/1/X/Z, ereignisgesteuert), Gatter, Flipflops, Zähler, Logikanalysator, Wahrheitstabelle |
| 3 | Transistoren (BJT, MOSFET), Multimeter, Poti und Taster |
| 4 | Mixed-Signal-Brücke, OpAmp, 555, ADC/DAC, eigene ICs |
| 5 | Stromfluss-Animation, Echtzeit-Simulation, Kennlinien, Lektionen mit Aufgabenprüfung |
| 6 | Politur, macOS-Bundle |
