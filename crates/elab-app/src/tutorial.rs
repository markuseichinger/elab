//! Tutorial-Leiste: Erklärungen zu jedem Bauteil und Grundbegriff.
//! Ein Klick auf ein Bauteil (Palette oder Schaltplan) setzt das Thema.

use elab_app::model::PartKind;

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
        }
    }

    /// Themen für das Inhaltsverzeichnis.
    pub const BASICS: [Topic; 9] = [
        Topic::Welcome,
        Topic::Editor,
        Topic::Nets,
        Topic::Ground,
        Topic::Ohm,
        Topic::Kirchhoff,
        Topic::OperatingPoint,
        Topic::Colors,
        Topic::Scope,
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
            what: "Bauteil platzieren: in der Palette anklicken, dann in den Schaltplan klicken. Mit R drehst du es vor dem Absetzen.\n\
                   Verdrahten: von einem Anschluss (kleiner Kreis am Bauteilende) zum Ziel ziehen. Oder Werkzeug „Draht“ (W): \
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
    }
}
