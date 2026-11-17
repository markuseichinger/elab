# elab – Elektronik-Lernlabor

Schaltungen bauen, simulieren und beim Arbeiten zusehen: ein Lernprogramm für
Elektronik in Rust, als Gegenstück zu [nnlab](https://github.com/markuseichinger/nnlab).

> **Stand: Phase 0** – eigenständiger Simulationskern mit Referenztests.
> Editor und Oberfläche (egui) folgen in Phase 1.

## Aufbau

```
elab/
└── crates/
    └── elab-sim/        Simulationskern, ohne UI-Abhängigkeiten
        ├── src/
        │   ├── circuit.rs    Schaltungsmodell, Bauteile, Fehlermeldungen
        │   ├── analysis.rs   MNA, Newton-Raphson, DC-Arbeitspunkt, Transientenanalyse
        │   ├── diode.rs      Shockley-Modell mit pn-Spannungsbegrenzung
        │   ├── waveform.rs   DC, Sinus, Puls/Rechteck, stückweise linear
        │   └── linalg.rs     Dichte LU-Zerlegung mit Pivotisierung
        ├── tests/reference.rs   Tests gegen analytische Lösungen
        └── examples/demo.rs     Übersicht und Mini-Benchmark, CSV-Export
```

## Simulationsverfahren

- **Modifizierte Knotenanalyse:** Unbekannte sind die Knotenspannungen und die
  Zweigströme von Spannungsquellen und Spulen.
- **Newton-Raphson** für Dioden mit SPICE-artiger Spannungsbegrenzung (`pnjlim`).
  Wenn der direkte Versuch scheitert, greifen automatisch **gmin-Stepping**
  und **Source-Stepping**.
- **Transientenanalyse** mit Trapezregel. Direkt nach Knickstellen der Quellen
  folgt ein Schritt Euler-rückwärts gegen Klingeln. Die **Schrittweite wird adaptiv**
  über den lokalen Abbruchfehler gesteuert (Prädiktor-Korrektor nach Milne),
  und Flanken von Puls-/Rechteckquellen werden exakt getroffen.
- **Verständliche Fehlermeldungen** auf Deutsch, z. B. für schwebende Knoten,
  parallele Spannungsquellen oder ungültige Werte.

## Bauteile (Phase 0)

Widerstand, Kondensator, Spule, Schalter, Spannungs- und Stromquelle mit
DC/Sinus/Puls/PWL, Diode (Modelle: ideal, 1N4148, 1N4007, rote LED).

## Ausprobieren

```bash
cargo test                                         # alle Referenztests
cargo run --release --example demo                 # Übersicht + Laufzeiten
cargo run --release --example demo -- plots/       # zusätzlich CSV je Schaltung
```

```rust
use elab_sim::{Circuit, Simulator, TranOptions, Waveform};

let mut c = Circuit::new();
c.vsource("V1", "in", "gnd", Waveform::step(5.0, 0.0))?;
c.resistor("R1", "in", "out", 1e3)?;
c.capacitor("C1", "out", "gnd", 1e-6)?;

let mut sim = Simulator::new(&c)?;
let res = sim.transient(&TranOptions::new(5e-3, 2e-5))?;
println!("U(τ) = {:.3} V", res.voltage_at("out", 1e-3)?);   // ≈ 3,161 V
```

## Referenztests

| Test | Prüft gegen |
|---|---|
| Spannungsteiler, Überlagerung, Wheatstone-Brücke | Knotenanalyse von Hand |
| Diode vorwärts/rückwärts, Diodenkette, LED, Diode an 1 Ω | Shockley-Gleichung (Bisektion) |
| RC-Ladekurve, RL-Einschaltvorgang | e-Funktion, max. Abweichung < 0,1 % |
| RC mit grobem h_max = τ | Schrittweitensteuerung allein hält die Genauigkeit |
| LC-Schwingkreis | Periodendauer 2π√(LC), Amplitude nach 10 Perioden |
| RLC-Sprungantwort | geschlossene Lösung, Überschwingen |
| Rechteck auf RC | Flanken exakt getroffen, Endwerte |
| Einweg- und Brückengleichrichter (230 V) | Spitzenwert, Brummspannung, kein Rückstrom |
| Fehlerfälle | parallele Spannungsquellen, schwebende Knoten, ungültige Werte |

## Fahrplan

| Phase | Inhalt |
|---|---|
| **0** ✅ | Analogkern + Referenztests |
| 1 | Schaltplan-Editor (egui): Raster, Platzieren, Drähte, Undo/Redo, Speichern |
| 2 | Digital-Engine (0/1/X/Z, ereignisgesteuert), Gatter, Flipflops, 74xx, Logikanalysator |
| 3 | Transistoren (BJT, MOSFET), Oszilloskop, Multimeter |
| 4 | Mixed-Signal-Brücke, OpAmp, 555, ADC/DAC, eigene ICs |
| 5 | Stromfluss-Animation, Bauteil-Inspektor, Kennlinien, Lektionen mit Aufgabenprüfung |
| 6 | Politur, macOS-Bundle |
