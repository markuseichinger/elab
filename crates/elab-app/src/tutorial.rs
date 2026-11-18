//! Tutorial-Leiste: Erklärungen zu jedem Bauteil und Grundbegriff.
//! Ein Klick auf ein Bauteil (Palette oder Schaltplan) setzt das Thema.

use elab_app::model::{GateType, PartKind};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Topic {
    Welcome,
    Editor,
    Nets,
    Ground,
    Ohm,
    Kirchhoff,
    OperatingPoint,
    Colors,
    Scope,
    Resistor,
    Capacitor,
    Inductor,
    Lamp,
    DcVoltage,
    AcVoltage,
    SquareVoltage,
    CurrentSource,
    Diode,
    Led,
    Switch,
    Probe,
    // Digital
    Logic,
    Gate(GateType),
    LogicInput,
    Button,
    Clock,
    LogicHigh,
    LogicLed,
    DFlipFlop,
    JkFlipFlop,
    Counter,
    SevenSeg,
    Analyzer,
    TruthTable,
    // Phase 3
    Bjt,
    Mosfet,
    TransistorSwitch,
    RealLogic,
    Potentiometer,
    PushButton,
    Voltmeter,
    Ammeter,
    // Phase 4
    RealIc,
    Families,
    Schmitt,
    Timer555,
    OpAmp,
    AdcDac,
    RealTime,
    CustomIc,
    // Verbesserungen
    Limits,
    Tasks,
}

pub struct Doc {
    pub title: &'static str,
    pub what: &'static str,
    pub math: &'static str,
    pub typical: &'static str,
    pub watch: &'static str,
    pub related: &'static [Topic],
}

impl Topic {
    pub fn of(kind: &PartKind) -> Topic {
        match kind {
            PartKind::Resistor { .. } => Topic::Resistor,
            PartKind::Capacitor { .. } => Topic::Capacitor,
            PartKind::Inductor { .. } => Topic::Inductor,
            PartKind::Lamp { .. } => Topic::Lamp,
            PartKind::DcVoltage { .. } => Topic::DcVoltage,
            PartKind::AcVoltage { .. } => Topic::AcVoltage,
            PartKind::SquareVoltage { .. } => Topic::SquareVoltage,
            PartKind::CurrentSource { .. } => Topic::CurrentSource,
            PartKind::Diode { .. } => Topic::Diode,
            PartKind::Led { .. } => Topic::Led,
            PartKind::Switch { .. } => Topic::Switch,
            PartKind::Ground => Topic::Ground,
            PartKind::Probe => Topic::Probe,
            PartKind::Gate { gate } => Topic::Gate(*gate),
            PartKind::LogicInput { .. } => Topic::LogicInput,
            PartKind::Button => Topic::Button,
            PartKind::Clock { .. } => Topic::Clock,
            PartKind::LogicHigh => Topic::LogicHigh,
            PartKind::LogicLed { .. } => Topic::LogicLed,
            PartKind::DFlipFlop => Topic::DFlipFlop,
            PartKind::JkFlipFlop => Topic::JkFlipFlop,
            PartKind::Counter4 => Topic::Counter,
            PartKind::SevenSeg => Topic::SevenSeg,
            PartKind::Transistor { kind } if kind.is_bjt() => Topic::Bjt,
            PartKind::Transistor { .. } => Topic::Mosfet,
            PartKind::Potentiometer { .. } => Topic::Potentiometer,
            PartKind::PushButton => Topic::PushButton,
            PartKind::Voltmeter => Topic::Voltmeter,
            PartKind::Ammeter => Topic::Ammeter,
            PartKind::GateIc { .. } | PartKind::DffIc { .. } | PartKind::CounterIc { .. } => Topic::RealIc,
            PartKind::SchmittIc { .. } => Topic::Schmitt,
            PartKind::Timer555 => Topic::Timer555,
            PartKind::OpAmp { .. } => Topic::OpAmp,
            PartKind::Adc4 | PartKind::Dac4 => Topic::AdcDac,
            PartKind::Port | PartKind::Custom { .. } => Topic::CustomIc,
        }
    }

    /// Themen für das Inhaltsverzeichnis.
    pub const BASICS: [Topic; 11] = [
        Topic::Welcome,
        Topic::Editor,
        Topic::Nets,
        Topic::Ground,
        Topic::Ohm,
        Topic::Kirchhoff,
        Topic::OperatingPoint,
        Topic::Colors,
        Topic::Limits,
        Topic::Scope,
        Topic::Tasks,
    ];

    /// Themen zu Transistoren und Messgeräten.
    pub const TRANSISTORS: [Topic; 7] =
        [Topic::Bjt, Topic::Mosfet, Topic::TransistorSwitch, Topic::RealLogic, Topic::Potentiometer, Topic::Voltmeter, Topic::Ammeter];

    /// Themen zu echten Bausteinen (Mixed-Signal).
    pub const MIXED: [Topic; 8] =
        [Topic::RealIc, Topic::Families, Topic::Schmitt, Topic::Timer555, Topic::OpAmp, Topic::AdcDac, Topic::RealTime, Topic::CustomIc];

    /// Themen der Digitaltechnik für das Inhaltsverzeichnis.
    pub const DIGITAL: [Topic; 10] = [
        Topic::Logic,
        Topic::Gate(GateType::And),
        Topic::Gate(GateType::Or),
        Topic::Gate(GateType::Not),
        Topic::Gate(GateType::Xor),
        Topic::DFlipFlop,
        Topic::JkFlipFlop,
        Topic::Counter,
        Topic::Analyzer,
        Topic::TruthTable,
    ];
}

pub fn doc(t: Topic) -> Doc {
    use Topic::*;
    match t {
        Welcome => Doc {
            title: "Willkommen im elab",
            what: "Hier baust du Schaltungen wie auf einem Steckbrett und siehst sofort, was darin passiert. \
                   Links wählst du Bauteile aus, in der Mitte verdrahtest du sie. Die Simulation rechnet nach jeder \
                   Änderung neu: Drähte färben sich nach ihrer Spannung, LEDs und Lampen leuchten, Messpunkte zeigen ihren Wert.\n\n\
                   Am schnellsten geht der Einstieg über „📚 Beispiele“ oben. Dort liegen fertige Schaltungen zum Ausprobieren.",
            math: "",
            typical: "Erste Schritte: Beispiel „Spannungsteiler“ öffnen, R2 anklicken, im Inspektor links den Wert ändern und \
                      beobachten, wie sich U_aus verschiebt.",
            watch: "Fahre mit der Maus über Bauteile und Drähte: Ein Infofenster zeigt Spannung, Strom und Leistung. \
                    Ein Klick auf ein Bauteil öffnet hier die passende Erklärung.",
            related: &[Editor, Nets, Ohm, OperatingPoint],
        },
        Editor => Doc {
            title: "Bedienung des Editors",
            what: "Bauteil platzieren: aus der Palette in den Schaltplan ziehen, oder anklicken und dann absetzen. R dreht, auch während des Ziehens. \
                   Setzt du ein Bauteil auf eine Leitung, wird es eingefügt: Die Leitung unter dem Bauteil verschwindet, sonst wäre es kurzgeschlossen. \
                   Quer liegende Bauteile drehen sich dabei automatisch passend (grüne Vorschau).\n\
                   Verdrahten: von einem Anschluss oder von irgendeiner Stelle einer Leitung aus ziehen, so verlängerst du Leitungen oder zweigst ab. Oder Werkzeug „Draht“ (W): \
                   jeder Klick setzt eine Ecke, Doppelklick oder Esc beendet. Die Leertaste wechselt die Knickrichtung.\n\
                   Auswählen und verschieben: anklicken bzw. ziehen. Angeschlossene Drähte gehen mit. Ein Rahmen auf freier Fläche \
                   wählt mehrere Teile aus.\n\
                   Ansicht: Zwei-Finger-Wischen oder Mausrad verschiebt, Pinch oder ⌘ + Mausrad zoomt, „⤢ Einpassen“ zeigt alles.",
            math: "Tastenkürzel\n  R          drehen\n  Entf / ⌫   löschen\n  W          Drahtwerkzeug\n  Esc        abbrechen\n  ⌘Z / ⇧⌘Z   rückgängig / wiederholen\n  ⌘A         alles auswählen\n  ⌘S / ⌘O    speichern / öffnen",
            typical: "Offene Anschlüsse sind rot umkreist. Ein gefüllter Punkt markiert eine Verbindung von drei oder mehr Leitungen. \
                      Sich kreuzende Drähte ohne Punkt sind nicht verbunden.",
            watch: "Doppelklick auf einen Schalter legt ihn um, die Simulation reagiert sofort.",
            related: &[Nets, Welcome],
        },
        Nets => Doc {
            title: "Knoten und Netze",
            what: "Alles, was über Drähte leitend zusammenhängt, hat dieselbe Spannung und bildet einen Knoten (ein „Netz“). \
                   Ein Draht ist im Modell ideal: Er hat keinen Widerstand, also fällt an ihm keine Spannung ab. \
                   Deshalb trägt ein Netz überall dieselbe Farbe.\n\n\
                   Verbunden ist, was einen Rasterpunkt teilt oder wo ein Drahtende auf einem anderen Draht endet (T-Abzweig, gefüllter Punkt).",
            math: "Ein Netz = ein Knoten = eine Unbekannte\nim Gleichungssystem des Simulators.",
            typical: "Eine Schaltung mit n Knoten (ohne Masse) hat n unbekannte Spannungen. Der Simulator stellt für jeden Knoten \
                      die Knotenregel auf und löst das System.",
            watch: "Fahre über einen Draht: Das Infofenster zeigt den Netznamen. Ein Messpunkt gibt seinem Netz einen Namen.",
            related: &[Kirchhoff, Ground, Probe],
        },
        Ground => Doc {
            title: "Masse (⏚)",
            what: "Spannung ist immer eine Differenz zwischen zwei Punkten. Damit man jedem Knoten eine einzelne Zahl geben kann, \
                   legt man einen Bezugspunkt fest: die Masse mit 0 V. Alle angezeigten Spannungen sind „gegen Masse“ gemessen.\n\n\
                   Jede Schaltung braucht mindestens ein Masse-Symbol. Mehrere Masse-Symbole sind automatisch miteinander verbunden.",
            math: "U_Knoten = φ_Knoten − φ_Masse,   φ_Masse = 0 V",
            typical: "Üblich ist die Masse am Minuspol der Versorgung. Sie kann aber an jedem Punkt liegen. Dann werden \
                      manche Spannungen negativ.",
            watch: "Setze die Masse im Spannungsteiler an den Pluspol der Batterie: Alle Spannungen werden negativ, die \
                    Differenzen bleiben gleich.",
            related: &[Nets, OperatingPoint],
        },
        Ohm => Doc {
            title: "Ohmsches Gesetz",
            what: "In einem Widerstand ist der Strom proportional zur anliegenden Spannung. Der Proportionalitätsfaktor \
                   ist der Widerstand R. Doppelte Spannung bedeutet also doppelten Strom.",
            math: "U = R · I        I = U / R\nP = U · I = I² · R = U² / R",
            typical: "1 kΩ an 5 V ergibt 5 mA und 25 mW. 100 Ω an 12 V ergibt 120 mA und 1,44 W, das ist zu viel für einen \
                      gewöhnlichen ¼-W-Widerstand.",
            watch: "Wähle einen Widerstand aus: Im Inspektor siehst du U, I und P und kannst U/I nachrechnen.",
            related: &[Resistor, Kirchhoff],
        },
        Kirchhoff => Doc {
            title: "Kirchhoffsche Regeln",
            what: "Knotenregel: Was in einen Knoten hineinfließt, fließt auch wieder hinaus. Ladung geht nicht verloren.\n\
                   Maschenregel: Entlang jedes geschlossenen Umlaufs addieren sich alle Spannungen zu null. Die Quellspannung \
                   teilt sich auf die Verbraucher auf.",
            math: "Knoten:  Σ I_hinein = Σ I_hinaus\nMasche:  Σ U = 0",
            typical: "Im Spannungsteiler fließt durch R1 und R2 derselbe Strom (Knotenregel). U_R1 + U_R2 = U_Batterie (Maschenregel).",
            watch: "Fahre im Beispiel „Glühlampen parallel“ über die Batterie und die Lampen: Der Batteriestrom ist die Summe der Lampenströme.",
            related: &[Ohm, Nets],
        },
        OperatingPoint => Doc {
            title: "Arbeitspunkt (Gleichstromanalyse)",
            what: "Die Live-Simulation berechnet den eingeschwungenen Gleichstromzustand: Kondensatoren sind dann aufgeladen und \
                   lassen keinen Strom mehr durch (offen), Spulen wirken wie ein Draht. Wechsel- und Rechteckquellen gehen mit \
                   ihrem Wert bei t = 0 ein.\n\n\
                   Für Dioden und LEDs ist das Gleichungssystem nichtlinear. Der Simulator nähert sich der Lösung mit dem \
                   Newton-Verfahren in mehreren Schritten.",
            math: "G · x = b   (modifizierte Knotenanalyse)\nNewton:  x_(k+1) = x_k − J⁻¹ · F(x_k)",
            typical: "Kleine Schaltungen brauchen unter einer Millisekunde Rechenzeit. Die Zahl der Newton-Schritte steht in der Statuszeile.",
            watch: "Zeitabhängiges Verhalten (Laden, Schwingen, Gleichrichten) zeigt das Oszilloskop.",
            related: &[Scope, Diode, Colors],
        },
        Colors => Doc {
            title: "Spannungsfarben",
            what: "Jeder Draht wird nach der Spannung seines Netzes eingefärbt: Grau ist 0 V (Masse), Orange ist positiv, Blau negativ. \
                   Je kräftiger die Farbe, desto näher liegt die Spannung an der höchsten Spannung der Schaltung.",
            math: "Farbskala: −U_max … 0 V … +U_max\n(U_max = größter Betrag aller Knotenspannungen)",
            typical: "In einer Reihenschaltung sieht man so auf einen Blick, wo die Spannung abfällt: Die Farbe ändert sich nur \
                      an Bauteilen, nie entlang eines Drahts.",
            watch: "Die Legende unten im Schaltplan zeigt die aktuelle Skala.",
            related: &[Nets, OperatingPoint],
        },
        Scope => Doc {
            title: "Oszilloskop (Zeitverlauf)",
            what: "Das Oszilloskop simuliert die Schaltung über einen Zeitraum und zeichnet die Spannung an jedem Messpunkt auf. \
                   Hier siehst du, was der Arbeitspunkt nicht zeigen kann: Kondensatoren laden sich auf, Schwingkreise schwingen, \
                   Gleichrichter glätten.\n\n\
                   Der Simulator wählt die Schrittweite selbst: Wo sich wenig ändert, macht er große Schritte, an Flanken und \
                   Knicken kleine.",
            math: "Trapezregel:  x_(n+1) = x_n + h/2 · (f(x_n) + f(x_(n+1)))\nSchrittweite über den lokalen Fehler gesteuert",
            typical: "Zeitbasis passend wählen: einige Perioden der Quelle oder etwa 5 · τ bei RC-Gliedern. Mit „Automatisch“ misst \
                      das Oszilloskop nach jeder Änderung neu.",
            watch: "Setze Messpunkte (📍) an die Stellen, die dich interessieren. Ihr Name erscheint in der Legende.",
            related: &[Probe, Capacitor, AcVoltage],
        },
        Resistor => Doc {
            title: "Widerstand",
            what: "Ein Widerstand begrenzt den Strom und setzt elektrische Energie in Wärme um. Er ist das einfachste Bauteil: \
                   Strom und Spannung sind streng proportional (ohmsch).",
            math: "U = R · I\nReihe:     R = R1 + R2\nParallel:  1/R = 1/R1 + 1/R2\nTeiler:    U_aus = U_ein · R2 / (R1 + R2)",
            typical: "Werte nach E-Reihen: 100 Ω, 220 Ω, 470 Ω, 1 kΩ, 4,7 kΩ, 10 kΩ, 100 kΩ. Eingabe auch als „4k7“ oder „2R2“.",
            watch: "Verdopple R2 im Spannungsteiler: U_aus steigt nicht auf das Doppelte, weil auch der Gesamtstrom sinkt.",
            related: &[Ohm, Kirchhoff, Lamp],
        },
        Capacitor => Doc {
            title: "Kondensator",
            what: "Ein Kondensator speichert Ladung zwischen zwei Platten. Strom fließt nur, während sich seine Spannung ändert. \
                   Im eingeschwungenen Gleichstromfall wirkt er deshalb wie eine Unterbrechung.\n\n\
                   Über einen Widerstand geladen, folgt die Spannung einer e-Funktion mit der Zeitkonstante τ = R · C.",
            math: "Q = C · U          I = C · dU/dt\nLaden:  U(t) = U₀ · (1 − e^(−t/τ)),  τ = R · C\nNach 1 τ: 63 %,  nach 5 τ: > 99 %",
            typical: "Keramik 10 pF … 1 µF (Entkopplung, Filter), Elko 1 µF … 10 mF (Glättung, Speicher).",
            watch: "Beispiel „RC-Tiefpass“: Halbiere C. Im Oszilloskop wird die Ladekurve doppelt so steil.",
            related: &[Scope, Inductor, Resistor],
        },
        Inductor => Doc {
            title: "Spule",
            what: "Eine Spule speichert Energie im Magnetfeld. Sie wehrt sich gegen Stromänderungen: Je schneller sich der Strom \
                   ändern soll, desto größer die Gegenspannung. Im Gleichstromfall wirkt sie wie ein Draht.\n\n\
                   Zusammen mit einem Kondensator entsteht ein Schwingkreis, in dem die Energie zwischen Magnet- und elektrischem Feld pendelt.",
            math: "U = L · dI/dt\nZeitkonstante mit R:  τ = L / R\nSchwingkreis:  f₀ = 1 / (2π · √(L · C))",
            typical: "Drosseln 1 µH … 100 mH. 1 mH mit 1 µF ergibt f₀ ≈ 5 kHz.",
            watch: "Beispiel „Reihenschwingkreis“: Vergrößere R. Die Schwingung klingt schneller ab, ab etwa R = 2·√(L/C) schwingt nichts mehr.",
            related: &[Capacitor, Scope],
        },
        Lamp => Doc {
            title: "Glühlampe",
            what: "Eine Glühlampe ist im Kern ein Widerstand, dessen Draht heiß genug wird, um zu leuchten. Im elab wird sie aus \
                   Nennspannung und Nennleistung als fester Widerstand berechnet. Die Helligkeit folgt der aufgenommenen Leistung.",
            math: "R = U_nenn² / P_nenn\n12 V / 5 W  →  R = 28,8 Ω,  I = 0,42 A",
            typical: "Ein echter Glühfaden hat kalt nur etwa ein Zehntel seines heißen Widerstands, daher der Einschaltstromstoß. \
                      Das Modell vernachlässigt diesen Effekt.",
            watch: "Schalte zwei Lampen in Reihe statt parallel: Jede bekommt nur die halbe Spannung und ein Viertel der Leistung.",
            related: &[Resistor, Kirchhoff],
        },
        DcVoltage => Doc {
            title: "Gleichspannungsquelle",
            what: "Eine ideale Spannungsquelle hält ihre Spannung konstant, egal welcher Strom fließt. Das Symbol ist die Batterie: \
                   Der lange Strich ist der Pluspol.\n\nIm Inspektor zeigt P die abgegebene Leistung.",
            math: "U = konstant\nP_abgegeben = U · I",
            typical: "1,5 V (Mignon), 3,3 V und 5 V (Logik), 9 V (Block), 12 V (Kfz), 24 V (Industrie).",
            watch: "Echte Batterien haben einen Innenwiderstand. Den bildest du nach, indem du einen kleinen Widerstand in Reihe schaltest.",
            related: &[Ground, Kirchhoff, CurrentSource],
        },
        AcVoltage => Doc {
            title: "Wechselspannungsquelle",
            what: "Liefert eine sinusförmige Spannung. Die Angabe ist die Amplitude (Scheitelwert), nicht der Effektivwert.",
            math: "u(t) = Û · sin(2π · f · t) + U_offset\nEffektivwert:  U_eff = Û / √2\nNetz:  230 V_eff  →  Û ≈ 325 V",
            typical: "Netzfrequenz 50 Hz, Audio 20 Hz … 20 kHz.",
            watch: "Der Arbeitspunkt verwendet den Wert bei t = 0 (meist 0 V). Den Verlauf zeigt das Oszilloskop.",
            related: &[Scope, Diode, SquareVoltage],
        },
        SquareVoltage => Doc {
            title: "Rechteckspannung",
            what: "Springt periodisch zwischen einem unteren und einem oberen Wert (Tastgrad 50 %). Damit testet man, wie eine \
                   Schaltung auf plötzliche Änderungen reagiert, z. B. die Ladekurve eines RC-Glieds.",
            math: "Periode T = 1 / f,  je T/2 oben und unten",
            typical: "Logiksignale 0 … 5 V oder 0 … 3,3 V, Taktgeber im kHz-Bereich.",
            watch: "Wähle die Frequenz so, dass eine halbe Periode deutlich länger ist als 5 · τ, dann lädt der Kondensator ganz durch.",
            related: &[Scope, Capacitor, AcVoltage],
        },
        CurrentSource => Doc {
            title: "Stromquelle",
            what: "Eine ideale Stromquelle treibt einen festen Strom durch die Schaltung. Die Spannung stellt sich so ein, \
                   wie es der Last entspricht. Der Pfeil zeigt die Stromrichtung, der Strom tritt am oberen Anschluss aus.",
            math: "I = konstant\nU = I · R_Last",
            typical: "In der Praxis z. B. LED-Treiber oder Konstantstromladung. Eine Stromquelle braucht immer einen geschlossenen Kreis.",
            watch: "Ändere den Lastwiderstand: Der Strom bleibt gleich, die Spannung an der Quelle ändert sich.",
            related: &[DcVoltage, Ohm],
        },
        Diode => Doc {
            title: "Diode",
            what: "Eine Diode lässt Strom praktisch nur in eine Richtung durch: von der Anode (Dreieck) zur Kathode (Strich). \
                   In Durchlassrichtung beginnt sie bei Silizium ab etwa 0,6 V zu leiten, der Strom steigt danach exponentiell. \
                   In Sperrrichtung fließt nur ein winziger Sperrstrom.",
            math: "Shockley:  I = I_S · (e^(U / (n · U_T)) − 1)\nU_T = k·T/q ≈ 25,9 mV bei 27 °C\n+60 mV · n  →  zehnfacher Strom",
            typical: "1N4148: Kleinsignal, schnell, bis 300 mA.  1N4007: Gleichrichter, bis 1 A / 1000 V.",
            watch: "Beispiel „Einweggleichrichter“: Am Ausgang liegt die Spitzenspannung minus etwa 0,8 V. Die Lücke zeigt die Flussspannung.",
            related: &[Led, OperatingPoint, AcVoltage],
        },
        Led => Doc {
            title: "Leuchtdiode (LED)",
            what: "Eine LED ist eine Diode, die beim Stromfluss Licht abgibt. Die Flussspannung hängt von der Farbe ab. \
                   Eine LED braucht immer einen Vorwiderstand: Wegen der steilen Kennlinie würde schon ein wenig zu viel Spannung \
                   einen sehr großen Strom bedeuten.",
            math: "R_vor = (U_B − U_F) / I_LED\n5 V, rot (1,8 V), 15 mA  →  R ≈ 213 Ω  →  220 Ω",
            typical: "U_F: rot 1,8 V, gelb 2,0 V, grün 2,2 V, blau/weiß 3,0 … 3,1 V. Standard-LEDs: 5 … 20 mA.",
            watch: "Entferne den Vorwiderstand (durch einen Draht ersetzen): Der Strom schießt in den Ampere-Bereich. Eine echte LED wäre sofort zerstört.",
            related: &[Diode, Ohm],
        },
        Switch => Doc {
            title: "Schalter",
            what: "Ein Schalter verbindet oder trennt zwei Punkte. Geschlossen hat er im Modell 1 mΩ, offen 10¹⁵ Ω (praktisch unendlich). \
                   Umschalten per Doppelklick oder im Inspektor.",
            math: "geschlossen: R = 1 mΩ\noffen:       R = 10¹⁵ Ω",
            typical: "Mit einem Schalter in der Zuleitung lässt sich gut zeigen, dass ohne geschlossenen Stromkreis kein Strom fließt.",
            watch: "Öffne den Schalter im LED-Beispiel: Der Strom ist null. Die LED-Spannung fällt auf 0 V, an den Schalterkontakten liegt die volle Spannung.",
            related: &[Kirchhoff, Led],
        },
        Probe => Doc {
            title: "Messpunkt",
            what: "Ein Messpunkt zeigt die Spannung seines Netzes gegen Masse direkt im Schaltplan an und liefert eine Kurve \
                   für das Oszilloskop. Sein Name wird zum Netznamen, z. B. „U_aus“.",
            math: "Anzeige: U = φ_Netz − φ_Masse",
            typical: "Setze Messpunkte an Ein- und Ausgang. Jeder bekommt eine eigene Farbe, dieselbe wie im Oszilloskop.",
            watch: "Ein Messpunkt darf mitten auf einem Draht sitzen. Er verbindet sich automatisch mit dem Draht.",
            related: &[Scope, Nets, Ground],
        },
        Logic => Doc {
            title: "Logikpegel 0, 1, X und Z",
            what: "In der Digitaltechnik zählt nur, ob eine Spannung als „niedrig“ (0) oder „hoch“ (1) gilt. Die Drähte \
                   zeigen das farbig: hellgrün ist 1, dunkelgrün ist 0.\n\n\
                   Dazu kennt der Simulator zwei Sonderzustände wie echte Logiksimulatoren: \
                   X (rot) heißt unbestimmt, etwa wenn zwei Ausgänge gegeneinander treiben (Buskonflikt) oder ein Eingang nirgends \
                   angeschlossen ist. Z (blaugrau) heißt hochohmig: Niemand treibt das Netz.\n\n\
                   Jedes Gatter braucht eine Laufzeit von 10 ns, bis sein Ausgang reagiert. Deshalb gibt es wie in echter \
                   Hardware kurze Störimpulse (Glitches), wenn Signale unterschiedlich lange Wege nehmen.",
            math: "0  Low   (CMOS 5 V: < 1,5 V)\n1  High  (CMOS 5 V: > 3,5 V)\nX  unbestimmt / Konflikt\nZ  hochohmig, kein Treiber",
            typical: "Ein offener CMOS-Eingang ist in der Praxis gefährlich: Er fängt Störungen ein und kann zufällig kippen. \
                      Darum zeigt der Simulator ihn als X.",
            watch: "Verbinde zwei Logikschalter mit demselben Draht und stelle sie unterschiedlich: Das Netz wird rot (Konflikt).",
            related: &[Gate(GateType::And), Analyzer, LogicInput],
        },
        Bjt => Doc {
            title: "Bipolartransistor (NPN, PNP)",
            what: "Ein kleiner Basisstrom steuert einen großen Kollektorstrom. Beim NPN-Transistor fließt der Hauptstrom von \
                   Kollektor (C) zu Emitter (E), gesteuert vom Strom in die Basis (B). Die Basis-Emitter-Strecke ist eine Diode: \
                   Erst ab etwa 0,6 V fließt Basisstrom.\n\n\
                   Drei Betriebsbereiche: gesperrt (kein Basisstrom, Schalter aus), aktiv (I_C = β · I_B, Verstärker) und \
                   gesättigt (so viel Basisstrom, dass der Transistor ganz durchschaltet, Schalter ein). Der PNP-Transistor \
                   arbeitet spiegelbildlich: Der Emitter liegt an der Plusseite, der Pfeil zeigt hinein.",
            math: "I_C = β · I_B          (aktiver Bereich)\nI_E = I_C + I_B\nU_BE ≈ 0,6 … 0,7 V     (Silizium)\nU_CE,sat ≈ 0,1 … 0,2 V (gesättigt)",
            typical: "BC547: β ≈ 200 … 450, I_C bis 100 mA. 2N2222: bis 600 mA. BC557 ist das PNP-Gegenstück zum BC547.",
            watch: "Fahre über den Transistor: Das Infofenster zeigt U_BE, U_CE, beide Ströme, I_C / I_B und den Betriebsbereich.",
            related: &[TransistorSwitch, Mosfet, Diode],
        },
        Mosfet => Doc {
            title: "MOSFET (N- und P-Kanal)",
            what: "Beim Feldeffekttransistor steuert eine Spannung, kein Strom: Das Gate ist isoliert, im Gleichstromfall fließt \
                   dort nichts. Ein N-Kanal-MOSFET leitet zwischen Drain (D) und Source (S), sobald U_GS über der Schwellspannung U_th liegt. \
                   Ein P-Kanal-MOSFET leitet, wenn das Gate um mehr als U_th unter der Source liegt.\n\n\
                   Das Substrat ist intern mit der Source verbunden (Pfeil). Weit durchgesteuert wirkt der MOSFET wie ein kleiner \
                   Widerstand (linearer Bereich, Schalter ein), knapp über der Schwelle wie eine gesteuerte Stromquelle (Sättigung).",
            math: "gesperrt:  U_GS < U_th        → I_D = 0\nlinear:    U_DS < U_GS − U_th → I_D = K·(2·(U_GS−U_th)·U_DS − U_DS²)\nSättigung: I_D = K · (U_GS − U_th)²",
            typical: "2N7000: U_th ≈ 2 V, bis 200 mA. BS250 ist das P-Kanal-Gegenstück. Die CMOS-Logik-Typen haben U_th ≈ 1 V und \
                      kleine Ströme, so wie die Transistoren in einem Logik-IC.",
            watch: "Beispiel „CMOS-Inverter“: Dreh das Poti langsam und beobachte, wie N- und P-Kanal abwechselnd sperren.",
            related: &[RealLogic, Bjt, TransistorSwitch],
        },
        TransistorSwitch => Doc {
            title: "Transistor als Schalter",
            what: "Mit einem Transistor schaltet ein kleines Signal (etwa ein Mikrocontroller-Pin) eine große Last. Der Basisstrom \
                   wird mit einem Vorwiderstand so gewählt, dass der Transistor sicher gesättigt ist: etwa zwei- bis fünfmal mehr, \
                   als I_C / β verlangen würde (Übersteuerung). Dann ist U_CE klein und der Transistor wird kaum warm.",
            math: "I_B = k · I_C / β       (k ≈ 3)\nR_B = (U_ein − 0,7 V) / I_B\nBeispiel: 20 mA, β = 200, 5 V\n  I_B = 0,3 mA → R_B ≈ 14 kΩ → 10 kΩ",
            typical: "Ein MOSFET braucht keinen Basisstrom. Wichtig ist nur, dass U_GS deutlich über U_th liegt: Ein 2N7000 schaltet \
                      bei 5 V gut, bei 3,3 V (ESP32) nur knapp.",
            watch: "Beispiel „Transistor als Schalter“: Taster drücken, beide Amperemeter vergleichen. Im gesättigten Zustand ist \
                    I_C deutlich kleiner als β · I_B, der Transistor ist wie gewollt übersteuert.",
            related: &[Bjt, Mosfet, PushButton],
        },
        RealLogic => Doc {
            title: "Logikpegel in echter Hardware",
            what: "In der Digital-Palette sind Pegel ideal 0 oder 1. In echter Hardware sind es Spannungen, und der Baustein entscheidet \
                   anhand von Schwellen: Bei 5-V-CMOS gilt alles unter etwa 1,5 V sicher als 0 und alles über 3,5 V sicher als 1. \
                   Dazwischen ist der Zustand undefiniert, beide Ausgangstransistoren leiten ein wenig, und es fließt Querstrom.\n\n\
                   Der CMOS-Inverter zeigt den Aufbau jedes Logikgatters: Ein P-Kanal-MOSFET zieht den Ausgang nach oben, ein N-Kanal-MOSFET \
                   nach unten. Im Ruhezustand sperrt immer einer von beiden, deshalb braucht CMOS fast keinen Strom.",
            math: "5-V-CMOS (74HC):\n  0 sicher:  U_E ≤ 1,5 V\n  1 sicher:  U_E ≥ 3,5 V\n  Ausgang:   ≈ 0 V bzw. ≈ 5 V\n  Schwelle:  ≈ U_DD / 2\n3,3-V-Logik (ESP32): 0 bis ≈ 0,8 V, 1 ab ≈ 2,5 V",
            typical: "Ein Ausgang, der nur 3,3 V liefert, ist für einen 5-V-CMOS-Eingang ein Grenzfall (unter 3,5 V). Dafür gibt es \
                      Pegelwandler oder 74HCT-Bausteine mit TTL-Schwellen.",
            watch: "Beispiel „CMOS-Inverter“: Stell das Poti langsam von 0 auf 100 % und lies die Voltmeter. Wo kippt der Ausgang, \
                    und wie viel Querstrom zeigt das Amperemeter genau dort? Die Bausteine im Reiter „ICs“ verhalten sich genauso: \
                    Sie brauchen eine Versorgung und entscheiden über Schaltschwellen.",
            related: &[Mosfet, Logic, RealIc, Families],
        },
        RealIc => Doc {
            title: "Logik-ICs mit Versorgung",
            what: "Die Bausteine im Reiter „ICs“ verhalten sich wie echte Chips auf dem Steckbrett. Sie haben VCC und GND und arbeiten \
                   nur, wenn dort eine passende Spannung anliegt. Ein Eingang ist hochohmig: Er misst nur die Spannung und \
                   entscheidet anhand von Schaltschwellen zwischen 0 und 1. Liegt die Spannung dazwischen, ist der Pegel undefiniert (X). \
                   Dann gibt der Ausgang etwa die halbe Versorgung aus.\n\n\
                   Ein Ausgang ist eine kleine Gegentaktstufe mit rund 50 Ω Innenwiderstand. Er schaltet nach VCC oder nach GND, \
                   und der Strom für eine LED fließt wirklich durch den VCC-Anschluss des ICs. \
                   Offene Eingänge sind bei CMOS verboten: Sie nehmen irgendeinen Pegel an. elab zeigt sie als X. \
                   Taster brauchen deshalb einen Pull-down- oder Pull-up-Widerstand.",
            math: "74HC an 5 V:\n  Eingang 0:  U_E < 1,5 V  (30 % von VCC)\n  Eingang 1:  U_E > 3,5 V  (70 % von VCC)\n  Ausgang:    ≈ 50 Ω nach VCC bzw. GND\n  Versorgung: 2 … 6 V, über 7 V zerstört\n  Laufzeit:   ≈ 10 ns",
            typical: "Typnummern: 74HC00 NAND, 02 NOR, 04 NICHT, 08 UND, 32 ODER, 86 XOR, 14 Schmitt-Trigger, 74 D-Flipflop, \
                      161 Zähler. Pull-down 10 kΩ, LED am Ausgang mit 330 Ω … 1 kΩ.",
            watch: "Nimm im Beispiel „NAND-Baustein“ den Pull-down-Widerstand weg: Der Eingang wird X. Hänge den IC an 9 V: \
                    Er geht kaputt. Die Pegel, die der IC an seinen Anschlüssen sieht, sind farbig markiert.",
            related: &[Families, RealLogic, Schmitt, RealTime],
        },
        Families => Doc {
            title: "Logikfamilien: HC, HCT, 3,3 V",
            what: "Logikfamilien unterscheiden sich in Versorgung und Schaltschwellen. 74HC-Bausteine schalten relativ zu ihrer \
                   Versorgung (30 % / 70 %). 74HCT-Bausteine haben feste TTL-Schwellen (0,8 V / 2,0 V) und laufen nur an 5 V.\n\n\
                   Das ist wichtig, wenn verschiedene Spannungen zusammentreffen. Ein ESP32 oder Raspberry Pi gibt als 1 nur 3,3 V aus. \
                   Ein 74HC an 5 V braucht aber mindestens 3,5 V, die 3,3 V liegen also im verbotenen Bereich. Ein 74HCT erkennt sie \
                   sicher als 1 und setzt sie auf 5 V um: ein einfacher Pegelwandler. In die andere Richtung, von 5 V nach 3,3 V, \
                   braucht man einen Spannungsteiler oder einen Baustein, der 5 V am Eingang verträgt.",
            math: "Eingangsschwellen bei 5 V:\n  74HC:   0 < 1,5 V,  1 > 3,5 V\n  74HCT:  0 < 0,8 V,  1 > 2,0 V\n3,3-V-Ausgang: 1 ≈ 3,3 V\n  → für 74HC: undefiniert\n  → für 74HCT: sicher 1",
            typical: "Die Familie stellst du im Inspektor eines Logik-ICs um. HCT-Bausteine sind die übliche Wahl als Pegelwandler \
                      von 3,3-V-Mikrocontrollern zu 5-V-Logik, etwa für LED-Streifen (WS2812).",
            watch: "Beispiel „3,3 V trifft 5 V“: Beide ICs bekommen dasselbe 3,3-V-Signal. Der HC zeigt X und gibt eine halbe \
                    Spannung aus, der HCT schaltet sauber.",
            related: &[RealIc, RealLogic, Schmitt],
        },
        Schmitt => Doc {
            title: "Schmitt-Trigger (74HC14)",
            what: "Ein Eingang mit zwei Schaltschwellen: Steigt die Spannung, schaltet er erst bei U_T+ um, fällt sie, erst bei U_T−. \
                   Dazwischen behält er seinen Zustand (Hysterese). So werden langsame oder verrauschte Signale zu sauberen Flanken, \
                   und es gibt keinen undefinierten Bereich.\n\n\
                   Mit einem Widerstand vom Ausgang zurück auf einen Kondensator am Eingang entsteht ein Oszillator: Der Kondensator lädt \
                   bis U_T+, der Ausgang kippt, der Kondensator entlädt bis U_T−, und so weiter.",
            math: "74HC14 an 5 V:\n  U_T+ ≈ 2,8 V,  U_T− ≈ 1,8 V\nRC-Oszillator:\n  T = R·C · (ln((U−U_T−)/(U−U_T+)) + ln(U_T+/U_T−))\n    ≈ 0,8 · R·C",
            typical: "Taster entprellen, Sensorsignale aufbereiten, einfache Taktgeber. Mit 47 kΩ und 10 µF blinkt eine LED etwa zweimal pro Sekunde.",
            watch: "Beispiel „Schmitt-Trigger-Oszillator“: Setz einen Messpunkt an den Kondensator. Seine Spannung pendelt genau zwischen \
                    den beiden Schwellen.",
            related: &[RealIc, Capacitor, Timer555],
        },
        Timer555 => Doc {
            title: "Timer 555",
            what: "Der 555 ist einer der bekanntesten ICs. Innen teilen drei 5-kΩ-Widerstände die Versorgung in Drittel. Zwei Komparatoren \
                   vergleichen TRIG mit ⅓ und THR mit ⅔ der Versorgung und setzen ein Flipflop: TRIG unter ⅓ → Ausgang 1, \
                   THR über ⅔ → Ausgang 0. Ist der Ausgang 0, schaltet ein Transistor DIS nach Masse.\n\n\
                   Als Blinker (astabil) lädt der Kondensator über R_A und R_B, bis THR ⅔ erreicht. Dann entlädt DIS ihn über R_B bis ⅓, \
                   und alles beginnt von vorn. /RES auf 0 hält den Timer an.",
            math: "Astabil:\n  t_high = 0,693 · (R_A + R_B) · C\n  t_low  = 0,693 · R_B · C\n  f = 1,44 / ((R_A + 2·R_B) · C)\nVersorgung 4,5 … 16 V\nAusgang 1 ≈ VCC − 1,7 V",
            typical: "Blinker, Tongeneratoren, Verzögerungen. R_A ≥ 1 kΩ, sonst fließt beim Entladen zu viel Strom durch DIS.",
            watch: "Beispiel „555-Blinker“: Ändere C oder R_B und vergleiche die Blinkfrequenz mit der Formel. Im Oszilloskop sieht man \
                    die Sägezahnspannung am Kondensator zwischen 3 V und 6 V (bei 9 V).",
            related: &[Capacitor, Schmitt, RealTime],
        },
        OpAmp => Doc {
            title: "Operationsverstärker",
            what: "Ein OpAmp verstärkt die Differenz zwischen + und − enorm (100 000-fach). Allein schaltet er deshalb wie ein Komparator: \
                   Ausgang hoch, wenn + größer ist, sonst tief. Mit Gegenkopplung (Ausgang über einen Widerstand zurück auf −) regelt er \
                   seinen Ausgang so, dass + und − gleich groß sind. Dann bestimmen nur die Widerstände die Verstärkung.\n\n\
                   Der Ausgang kann nur innerhalb der Versorgung schwingen. Ein LM358 kommt nicht ganz an V+ heran (etwa 1,5 V darunter), \
                   ein Rail-to-Rail-OpAmp wie der MCP6001 schon.",
            math: "Nichtinvertierend:  U_aus = U_ein · (1 + R2/R1)\nInvertierend:       U_aus = −U_ein · R2/R1\nSpannungsfolger:    U_aus = U_ein\nGrenze:  V− … V+ (LM358: bis V+ − 1,5 V)",
            typical: "Sensorverstärker, Filter, Pufferstufen, Komparatoren. Einfache Versorgung 5 V oder 12 V, symmetrisch ±12 V.",
            watch: "Beispiel „OpAmp-Verstärker“: Dreh das Poti hoch. Ab einem Punkt steigt die Ausgangsspannung nicht mehr, der OpAmp ist \
                    übersteuert. Mit dem MCP6001 (im Inspektor) kommt er näher an die Versorgung.",
            related: &[Voltmeter, Potentiometer, AdcDac],
        },
        AdcDac => Doc {
            title: "A/D- und D/A-Umsetzer",
            what: "Ein A/D-Umsetzer (ADC) macht aus einer Spannung eine Zahl: Er teilt den Bereich 0 … U_REF in 2ⁿ Stufen und gibt die \
                   Nummer der Stufe binär aus. Ein D/A-Umsetzer (DAC) macht es umgekehrt. Dieser DAC ist ein R-2R-Widerstandsnetz: Jedes Bit \
                   schaltet einen Zweig nach REF oder GND, der Ausgang liefert U_REF · Code / 16 mit 10 kΩ Innenwiderstand.\n\n\
                   Mit 4 Bit gibt es nur 16 Stufen. Zwischen zwei Stufen geht Information verloren: der Quantisierungsfehler.",
            math: "Code = ⌊16 · U_ein / U_REF⌋  (0 … 15)\nU_aus = U_REF · Code / 16\nStufe (LSB) = U_REF / 16 = 0,3125 V bei 5 V",
            typical: "Mikrocontroller wie der ESP32 haben 12-Bit-ADCs (4096 Stufen) und 8-Bit-DACs. Der unbelastete DAC-Ausgang braucht \
                      oft einen Spannungsfolger (OpAmp), damit eine Last die Spannung nicht einbrechen lässt.",
            watch: "Beispiel „ADC und DAC“: Dreh das Poti langsam. Die LEDs zählen binär, das Voltmeter am DAC springt in Stufen.",
            related: &[OpAmp, Counter, RealIc],
        },
        RealTime => Doc {
            title: "Echtzeitsimulation",
            what: "Sobald ein IC, ein Kondensator, eine Spule oder eine Wechsel- bzw. Rechteckquelle in der Schaltung ist, rechnet elab den \
                   zeitlichen Verlauf fortlaufend in Echtzeit, statt nur einen Arbeitspunkt zu bestimmen. Beim Start ist alles wie beim \
                   Einschalten: Kondensatoren sind leer, Flipflops stehen auf 0. Die Farben und der Stromfluss zeigen immer den \
                   aktuellen Augenblick.\n\n\
                   Analogteil und Logik laufen dabei gemeinsam: Jeder Eingang prüft seine Spannung, und wenn sie eine Schaltschwelle \
                   kreuzt, sucht die Simulation den genauen Zeitpunkt. Dann schaltet die Logik mit ihrer Laufzeit, und der neue \
                   Ausgangspegel wirkt sofort wieder auf die Schaltung.",
            math: "",
            typical: "Mit dem Tempo-Regler in der Statuszeile läuft die Zeit langsamer (Zeitlupe) oder schneller. ↺ schaltet neu ein. \
                      Das Oszilloskop zeichnet die ersten Augenblicke nach dem Einschalten auf.",
            watch: "Schnelle Vorgänge (kHz) sind in Echtzeit nur ein Flimmern. Stell das Tempo auf 0,01× und schau zu, wie die Spannung am \
                    Kondensator eines Oszillators steigt und fällt.",
            related: &[RealIc, Timer555, Scope],
        },
        CustomIc => Doc {
            title: "Eigene ICs",
            what: "Jede Schaltung lässt sich als eigener Baustein speichern und dann beliebig oft einsetzen, wie ein IC aus dem Katalog. \
                   So baut man Großes aus Kleinem: aus Gattern einen Halbaddierer, aus zwei Halbaddierern einen Volladdierer, \
                   daraus ein Rechenwerk.\n\n\
                   Die Anschlüsse legst du mit dem Bauteil „IC-Anschluss“ fest. Sein Name wird zum Pin-Namen. Anschlüsse auf der \
                   linken Hälfte der Schaltung landen links am IC, die anderen rechts. VCC und GND kommen nach oben und unten. \
                   Speichern: im Reiter ICs auf „Schaltplan als IC speichern“. Das IC erscheint dann unter „Eigene ICs“.",
            math: "Halbaddierer:  S = A ⊕ B,  C = A · B\nVolladdierer:  zwei Halbaddierer + ODER\n  S = A ⊕ B ⊕ C_ein\n  C_aus = A·B + C_ein·(A ⊕ B)",
            typical: "Jedes eingesetzte IC trägt eine Kopie seiner Innenschaltung, eine Datei funktioniert deshalb auch ohne die \
                      Bibliothek. Die Bibliothek liegt in ~/.elab/ics. Im Inspektor öffnet „Innenschaltung öffnen“ den Inhalt zum Ansehen \
                      oder Ändern. Danach unter demselben Namen neu speichern.",
            watch: "Beispiel „Volladdierer aus Halbaddierer-ICs“: Schalte A, B und C_ein und vergleiche mit der Wahrheitstabelle. \
                    Fahre über ein Halbaddierer-IC: Die Anschlüsse zeigen ihre Pegel.",
            related: &[RealIc, Logic, TruthTable],
        },
        Limits => Doc {
            title: "Grenzwerte: Bauteile brennen durch",
            what: "Echte Bauteile vertragen nur begrenzt Strom und Leistung. Wird es zu viel, werden sie heiß und gehen kaputt. \
                   elab rechnet das mit: Ein Bauteil, das sich der Grenze nähert, färbt sich orange und pulsiert rot. Bleibt es zu \
                   lange darüber, brennt es durch. Es ist dann rot durchgestrichen und leitet nicht mehr.\n\n\
                   Kurze Spitzen schaden nicht: Wie ein echtes Bauteil braucht es eine Weile (hier etwa 0,1 s), bis es sich erwärmt. \
                   Im Inspektor setzt „Ersetzen“ ein neues Bauteil ein, ⌘Z macht das Durchbrennen rückgängig.",
            math: "Leistung:  P = U · I = I² · R = U² / R\nLED:  I_max ≈ 30 mA\nWiderstand (0207):  P_max = ¼ W\nGlühlampe:  P_max = 2 · P_nenn\n\
                   BC547:  I_C ≤ 100 mA, P ≤ 0,5 W",
            typical: "Grenzen in elab: LED 30 mA, 1N4148 300 mA, 1N4007 1,5 A, Widerstände 0,25 W, Glühlampen doppelte Nennleistung. \
                      Transistoren: BC547/BC557 100 mA und 0,5 W, 2N2222 600 mA, 2N7000 200 mA, BS250 180 mA. \
                      Fahre über ein Bauteil, um seine Belastung in Prozent zu sehen. Der Knopf „🔥 Grenzwerte“ oben schaltet das ab.",
            watch: "Nimm „LED mit Vorwiderstand“ und setze den Widerstand auf 47 Ω: Die LED brennt durch. Mit 22 Ω erwischt es auch den \
                    Widerstand, weil an ihm mehr als ¼ W abfällt. Rechne vorher: Welcher Widerstand ist mindestens nötig?",
            related: &[Ohm, Led, Resistor],
        },
        Tasks => Doc {
            title: "Aufgaben mit Prüfung",
            what: "Unter Beispiele → „🎯 Aufgaben“ findest du kleine Bauaufträge: Ein leeres Blatt, ein Ziel, und elab prüft \
                   laufend, ob deine Schaltung es erfüllt. Anders als bei den geführten Übungen gibt es keine Schritt-für-Schritt-\
                   Anleitung, nur Hinweise, wenn du nicht weiterkommst.\n\n\
                   Geprüft wird das Verhalten, nicht der Aufbau: Jede Schaltung, die das Gewünschte tut, zählt.",
            math: "Spannungsteiler:  U_aus = U · R₂ / (R₁ + R₂)\nVorwiderstand:  R = (U − U_LED) / I\n555 astabil:  f ≈ 1,44 / ((R_A + 2 R_B) · C)",
            typical: "Die Karte unten im Bild zeigt das Ziel und, was schon erfüllt ist. Mit „Tipp“ gibt es einen Hinweis, \
                      „Abbrechen“ beendet die Aufgabe.",
            watch: "Beginne mit „Spannungsteiler für 3,3 V“: Welche zwei Widerstände aus der E-Reihe treffen 3,3 V aus 9 V am besten?",
            related: &[Ohm, Limits],
        },
        Potentiometer => Doc {
            title: "Potentiometer",
            what: "Ein einstellbarer Spannungsteiler: Der Schleifer S greift den Widerstand zwischen A und B an einer wählbaren \
                   Stelle ab. Die Stellung gibt an, welcher Anteil des Widerstands zwischen S und B liegt. Mit B an Masse und A an der \
                   Versorgung liefert 70 % also 70 % der Spannung.",
            math: "R_SB = R · s,  R_AS = R · (1 − s)\nunbelastet:  U_SB = U_AB · s",
            typical: "Lautstärkeregler, Sollwertgeber, Einstellen von Schaltschwellen. Üblich sind 1 kΩ … 1 MΩ. Verstellen: Schieberegler \
                      im Inspektor oder bei ausgewähltem Poti die Pfeiltasten (⇧ für feine Schritte).",
            watch: "Belaste den Schleifer mit einem kleinen Widerstand gegen Masse: Die Spannung bricht ein, weil ein Teil des Stroms \
                    durch die Last statt durch R_SB fließt.",
            related: &[Resistor, Voltmeter, RealLogic],
        },
        PushButton => Doc {
            title: "Taster",
            what: "Ein Schließer, der nur leitet, solange er gedrückt ist. Halte die Maustaste auf dem Taster gedrückt. Anders als der \
                   Schalter rastet er nicht ein.",
            math: "gedrückt:    R = 1 mΩ\nlosgelassen: offen",
            typical: "Klingeltaster, Tastaturen, Reset-Knöpfe. Am Logikeingang braucht ein Taster einen Pull-down- oder Pull-up-Widerstand, \
                      sonst liegt der Eingang im losgelassenen Zustand offen.",
            watch: "Beispiel „NAND-Gatter aus Transistoren“: Die Taster legen die Eingänge auf 5 V, die 100-kΩ-Widerstände ziehen sie \
                    sonst auf 0 V.",
            related: &[Switch, TransistorSwitch],
        },
        Voltmeter => Doc {
            title: "Voltmeter",
            what: "Misst die Spannung zwischen seinem Plus- und Minusanschluss. Ein Voltmeter wird parallel zu dem Bauteil angeschlossen, \
                   dessen Spannung du wissen willst. Das ideale Voltmeter hat einen unendlichen Innenwiderstand, es belastet die \
                   Schaltung also nicht.",
            math: "U = φ(+) − φ(−)\nparallel anschließen",
            typical: "Echte Digitalmultimeter haben etwa 10 MΩ Innenwiderstand. An hochohmigen Schaltungen verfälscht das die Messung.",
            watch: "Vertausche Plus und Minus: Das Vorzeichen der Anzeige kehrt sich um.",
            related: &[Ammeter, Probe, Kirchhoff],
        },
        Ammeter => Doc {
            title: "Amperemeter",
            what: "Misst den Strom, der durch das Gerät fließt. Dafür muss es in Reihe in die Leitung: Setzt du es auf eine Leitung, \
                   wird es automatisch eingefügt. Das ideale Amperemeter hat 0 Ω Innenwiderstand.\n\n\
                   Ein Amperemeter parallel zu einer Spannungsquelle ist ein Kurzschluss: Die Anzeige meldet ÜBERLAST. Ein echtes \
                   Gerät würde dabei seine Sicherung auslösen.",
            math: "I positiv, wenn er von + nach − durch das Gerät fließt\nin Reihe anschließen",
            typical: "Multimeter haben getrennte Buchsen für mA und 10 A. Messbereich vor dem Messen passend wählen.",
            watch: "Miss den Strom vor und hinter einem Bauteil: Er ist gleich (Knotenregel).",
            related: &[Voltmeter, Kirchhoff, Ohm],
        },
        Gate(g) => gate_doc(g),
        LogicInput => Doc {
            title: "Logikschalter",
            what: "Ein Schalter, der fest 0 oder 1 liefert. Ein Klick auf den Schalter im Schaltplan legt ihn um. \
                   Logikschalter werden automatisch A, B, C … benannt, so heißen sie auch in der Wahrheitstabelle.",
            math: "Ausgang = 0 oder 1 (rastend)",
            typical: "Zum Durchprobieren von Gattern und Schaltnetzen, für feste Steuersignale (Reset, Freigabe).",
            watch: "Öffne die Wahrheitstabelle unten: Die Zeile der aktuellen Schalterstellung ist markiert. Ein Klick auf \
                    eine Zeile stellt die Schalter entsprechend ein.",
            related: &[Button, TruthTable, Logic],
        },
        Button => Doc {
            title: "Taster",
            what: "Liefert 1, solange du die Maustaste darauf gedrückt hältst, sonst 0. Ideal für Takt- und Setz-/Rücksetzsignale von Hand.",
            math: "Ausgang = 1, solange gedrückt",
            typical: "Echte Taster prellen: Beim Drücken schließt und öffnet der Kontakt mehrmals in wenigen Millisekunden. \
                      Der simulierte Taster prellt nicht.",
            watch: "Beispiel „RS-Flipflop“: Ein kurzer Druck auf S setzt den Speicher, der Zustand bleibt nach dem Loslassen erhalten.",
            related: &[LogicInput, DFlipFlop],
        },
        Clock => Doc {
            title: "Taktgeber",
            what: "Erzeugt ein Rechtecksignal mit einstellbarer Frequenz. Der Takt läuft in Echtzeit, mit dem Zeitlupenregler \
                   im Logikanalysator verlangsamst oder beschleunigst du ihn.",
            math: "Periode T = 1 / f, je T/2 auf 1 und auf 0",
            typical: "Zum Zuschauen 1 … 4 Hz. In echten Schaltungen liefert ein Quarz MHz bis GHz.",
            watch: "Takte einen Zähler: Bei jeder steigenden Flanke (0 → 1) zählt er eins weiter.",
            related: &[Counter, DFlipFlop, Analyzer],
        },
        LogicHigh => Doc {
            title: "Fester Pegel 1",
            what: "Legt ein Netz fest auf 1, wie ein Anschluss an die Versorgungsspannung (VCC). Für eine feste 0 \
                   nimmst du das Masse-Symbol.",
            math: "Ausgang = 1 (VCC)\nMasse = 0 (GND)",
            typical: "Unbenutzte Eingänge auf einen festen Pegel legen; beim JK-Flipflop J = K = 1 für den Kippbetrieb.",
            watch: "Ein UND-Gatter mit einem Eingang auf 1 gibt den anderen Eingang unverändert weiter.",
            related: &[Logic, JkFlipFlop],
        },
        LogicLed => Doc {
            title: "Logik-Anzeige",
            what: "Leuchtet bei 1, bleibt bei 0 dunkel. Bei X oder Z zeigt sie ein Fragezeichen. \
                   Logik-Anzeigen sind die Ausgänge der Wahrheitstabelle.",
            math: "leuchtet ⇔ Eingang = 1",
            typical: "Eine echte LED am Logikausgang braucht einen Vorwiderstand. Die Logik-Anzeige ist idealisiert.",
            watch: "Benenne die Anzeige im Inspektor, z. B. „Summe“. Der Name erscheint in Tabelle und Analysator.",
            related: &[TruthTable, Analyzer],
        },
        DFlipFlop => Doc {
            title: "D-Flipflop",
            what: "Der Grundbaustein jedes digitalen Speichers. Bei jeder steigenden Taktflanke (C: 0 → 1) übernimmt er den Wert \
                   an D und hält ihn bis zur nächsten Flanke, egal was D dazwischen macht. /Q (gesprochen „Q quer“, im Symbol mit Überstrich) ist immer das Gegenteil von Q.\n\n\
                   Verbindet man /Q mit D, kippt das Flipflop bei jeder Flanke: Die Frequenz halbiert sich (Frequenzteiler).",
            math: "Steigende Flanke an C:  Q ← D\nsonst:                  Q bleibt\n/Q = NICHT Q",
            typical: "Register, Schieberegister, Synchronisation von Signalen. Das Dreieck am C-Eingang kennzeichnet die Flankensteuerung.",
            watch: "Beispiel „Frequenzteiler“: Im Logikanalysator hat Q1 die halbe, Q2 ein Viertel der Taktfrequenz.",
            related: &[JkFlipFlop, Clock, Counter],
        },
        JkFlipFlop => Doc {
            title: "JK-Flipflop",
            what: "Ein vielseitiges Flipflop mit zwei Steuereingängen. Bei der steigenden Taktflanke gilt: J setzt, K setzt zurück, \
                   beide zusammen lassen es kippen, keiner von beiden hält den Zustand.",
            math: "J K │ Q nach der Flanke\n0 0 │ Q (halten)\n0 1 │ 0 (rücksetzen)\n1 0 │ 1 (setzen)\n1 1 │ /Q (kippen)",
            typical: "Mit J = K = 1 (Pegel 1) wird es zum Kippflipflop (T-Flipflop), der Baustein klassischer Zähler.",
            watch: "Lege J und K auf Logikschalter, takte mit einem Taster und probiere alle vier Fälle durch.",
            related: &[DFlipFlop, LogicHigh, Counter],
        },
        Counter => Doc {
            title: "4-Bit-Binärzähler",
            what: "Zählt bei jeder steigenden Taktflanke um eins weiter, von 0 bis 15, danach wieder 0 (Überlauf). Die Ausgänge \
                   Q0…Q3 zeigen den Stand als Binärzahl. R = 1 setzt ihn sofort auf 0 (asynchroner Reset).",
            math: "Stand = Q0·1 + Q1·2 + Q2·4 + Q3·8\nQ0 hat die halbe Taktfrequenz, Q1 ein Viertel …",
            typical: "Entspricht einem 74HC161/163. Zähler teilen Frequenzen, zählen Ereignisse und erzeugen Adressen.",
            watch: "Im Logikanalysator siehst du: Jedes Bit hat die halbe Frequenz des vorigen.",
            related: &[Clock, SevenSeg, DFlipFlop],
        },
        SevenSeg => Doc {
            title: "7-Segment-Anzeige",
            what: "Zeigt eine 4-Bit-Binärzahl als Ziffer 0…9 bzw. A…F (hexadezimal) an. Der Decoder ist eingebaut: Die Eingänge \
                   haben die Wertigkeiten 1, 2, 4 und 8. Ist ein Eingang offen oder unbestimmt, erscheint ein Strich.",
            math: "Wert = E1·1 + E2·2 + E4·4 + E8·8\nSegmente a…g",
            typical: "In echten Schaltungen sitzt dazwischen ein Decoder-IC wie der 74HC4511 (nur 0…9).",
            watch: "Verbinde die Ausgänge eines Zählers mit der Anzeige und sieh zu, wie sie hochzählt.",
            related: &[Counter, Logic],
        },
        Analyzer => Doc {
            title: "Logikanalysator",
            what: "Zeichnet die Pegel aller Schalter, Taster, Takte, Logik-Anzeigen und digitalen Messpunkte über die Zeit auf. \
                   Die rechte Kante ist „jetzt“. So siehst du Abläufe in Schaltungen mit Gedächtnis: Flanken, Teilerverhältnisse, Zählfolgen.",
            math: "Zeitachse: letzte Sekunden bis jetzt\nhoch = 1, tief = 0, rot = X, Mitte = Z",
            typical: "Mit dem Zeitlupenregler läuft die Simulation langsamer oder schneller als echt, ⏸ hält sie an.",
            watch: "Vergleiche Takt und Q eines D-Flipflops: Q ändert sich nur bei steigenden Taktflanken.",
            related: &[Clock, DFlipFlop, TruthTable],
        },
        TruthTable => Doc {
            title: "Wahrheitstabelle",
            what: "Listet für jede Kombination der Eingänge (Logikschalter, Taster) die Ausgänge (Logik-Anzeigen, Messpunkte). \
                   Das funktioniert für Schaltnetze, also Schaltungen ohne Gedächtnis: Der Ausgang hängt nur von den aktuellen Eingängen ab.\n\n\
                   Die Zeile der aktuellen Schalterstellung ist hervorgehoben. Ein Klick auf eine Zeile stellt die Schalter ein.",
            math: "n Eingänge → 2ⁿ Zeilen\n2 Eingänge: 00, 01, 10, 11",
            typical: "Aus der Tabelle liest man die Schaltfunktion ab, z. B. Summe = A XOR B, Übertrag = A UND B beim Halbaddierer.",
            watch: "Baue eine Funktion aus NAND-Gattern nach und vergleiche die Tabelle mit dem Original.",
            related: &[LogicInput, LogicLed, Analyzer],
        },
    }
}

fn gate_doc(g: GateType) -> Doc {
    use Topic::*;
    let related: &'static [Topic] = &[Logic, TruthTable, Gate(GateType::Nand)];
    match g {
        GateType::And => Doc {
            title: "UND-Gatter (AND)",
            what: "Der Ausgang ist nur 1, wenn alle Eingänge 1 sind. Wie zwei Schalter in Reihe: Nur wenn beide geschlossen sind, fließt Strom.",
            math: "A B │ Y
0 0 │ 0
0 1 │ 0
1 0 │ 0
1 1 │ 1
Y = A ∧ B",
            typical: "Freigabe eines Signals: Y folgt A, solange B = 1 ist. IEC-Symbol: „&“.",
            watch: "Eine 0 an einem Eingang erzwingt Y = 0, egal was am anderen anliegt, sogar bei X.",
            related,
        },
        GateType::Or => Doc {
            title: "ODER-Gatter (OR)",
            what: "Der Ausgang ist 1, sobald mindestens ein Eingang 1 ist. Wie zwei Schalter parallel.",
            math: "A B │ Y
0 0 │ 0
0 1 │ 1
1 0 │ 1
1 1 │ 1
Y = A ∨ B",
            typical: "Mehrere Auslöser für dieselbe Aktion (z. B. Alarm von mehreren Sensoren). IEC-Symbol: „≥1“.",
            watch: "Vergleiche mit XOR: Der Unterschied liegt nur in der letzten Zeile.",
            related,
        },
        GateType::Not => Doc {
            title: "NICHT-Gatter (Inverter)",
            what: "Kehrt den Pegel um: aus 0 wird 1, aus 1 wird 0. Der Kreis am Ausgang steht im IEC-Symbol für die Negation.",
            math: "A │ Y
0 │ 1
1 │ 0
Y = ¬A",
            typical: "Drei Inverter im Ring schwingen von selbst (Ringoszillator), die Frequenz bestimmt die Gatterlaufzeit.",
            watch: "Schalte ein NICHT-Gatter auf sich selbst zurück (Ausgang an Eingang): Der Simulator meldet eine Dauerschwingung.",
            related,
        },
        GateType::Nand => Doc {
            title: "NAND-Gatter",
            what: "UND mit negiertem Ausgang: Der Ausgang ist nur dann 0, wenn alle Eingänge 1 sind. NAND ist universell:                    Aus NAND-Gattern allein lässt sich jede andere Logikfunktion bauen.",
            math: "A B │ Y
0 0 │ 1
0 1 │ 1
1 0 │ 1
1 1 │ 0
Y = ¬(A ∧ B)",
            typical: "Der 74HC00 enthält vier NAND-Gatter. Ein NICHT entsteht, indem man beide Eingänge verbindet.",
            watch: "Baue ein ODER aus drei NAND-Gattern: zwei als Inverter vor einem NAND (De Morgan).",
            related,
        },
        GateType::Nor => Doc {
            title: "NOR-Gatter",
            what: "ODER mit negiertem Ausgang: nur 1, wenn alle Eingänge 0 sind. Wie NAND ist NOR universell.                    Zwei rückgekoppelte NOR-Gatter bilden einen Speicher (RS-Flipflop).",
            math: "A B │ Y
0 0 │ 1
0 1 │ 0
1 0 │ 0
1 1 │ 0
Y = ¬(A ∨ B)",
            typical: "Die Apollo-Bordcomputer bestanden fast nur aus NOR-Gattern.",
            watch: "Beispiel „RS-Flipflop“: Die Rückkopplung macht aus zwei Gattern einen 1-Bit-Speicher.",
            related,
        },
        GateType::Xor => Doc {
            title: "XOR-Gatter (Exklusiv-ODER)",
            what: "Der Ausgang ist 1, wenn die Eingänge verschieden sind. Damit addiert XOR zwei Bits ohne Übertrag.",
            math: "A B │ Y
0 0 │ 0
0 1 │ 1
1 0 │ 1
1 1 │ 0
Y = A ⊕ B",
            typical: "Halbaddierer (Summe = A XOR B), Paritätsprüfung, gesteuerter Inverter (B = 1 kehrt A um). IEC-Symbol: „=1“.",
            watch: "Beispiel „Halbaddierer“: Summe und Übertrag in der Wahrheitstabelle ergeben zusammen A + B.",
            related,
        },
        GateType::Xnor => Doc {
            title: "XNOR-Gatter (Äquivalenz)",
            what: "XOR mit negiertem Ausgang: 1, wenn beide Eingänge gleich sind. Damit vergleicht man Bits.",
            math: "A B │ Y
0 0 │ 1
0 1 │ 0
1 0 │ 0
1 1 │ 1
Y = ¬(A ⊕ B)",
            typical: "Komparatoren: Zwei Zahlen sind gleich, wenn alle Bitpaare per XNOR 1 liefern.",
            watch: "Schalte zwei Logikschalter auf ein XNOR: Die Anzeige leuchtet bei Gleichstand.",
            related,
        },
    }
}
