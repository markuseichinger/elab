//! Testskripte: spielt Maus- und Tastaturereignisse in die laufende App ein.
//!
//! Gedacht für automatisierte Oberflächentests und Screenshots:
//!
//! ```text
//! elab --script test.txt
//! ```
//!
//! Befehle (eine Zeile je Befehl, `#` für Kommentare; Koordinaten in Rasterpunkten):
//! `example N`, `tool select|wire|<Bauteil>`, `move X Y`, `click X Y`, `shiftclick X Y`,
//! `dblclick X Y`, `clickpx X Y`, `type TEXT`, `drag X1 Y1 X2 Y2`, `paldrag BAUTEIL X Y`, `key NAME`, `cmd NAME`, `cmdshift NAME`,
//! `select NAME`, `scope on|off`, `tutorial on|off`, `timebase SEKUNDEN`, `fit`,
//! `advance SEKUNDEN`, `hold X Y`, `release`, `panel scope|analyzer|truth`, `palette analog|digital`,
//! `pause on|off`, `wait N`, `shot DATEI.ppm`, `dump DATEI.json`, `quit`.

use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub enum Cmd {
    /// Beispiel nach Nummer oder Titelanfang
    Example(String),
    /// Geführte Übung: start, stop, skip
    Lesson(String),
    Tool(String),
    Move(f32, f32),
    Click(f32, f32, Modifiers),
    /// Klick an Bildschirmkoordinaten (für Bedienelemente außerhalb der Zeichenfläche)
    ClickPx(f32, f32),
    /// Ziehen von Bildschirmkoordinaten auf einen Rasterpunkt (z. B. aus der Palette)
    DragPxGrid(f32, f32, f32, f32),
    /// Bauteil per Name aus der Palette auf einen Rasterpunkt ziehen
    PalDrag(String, f32, f32),
    /// Text eintippen und mit Enter bestätigen
    Type(String),
    DblClick(f32, f32),
    Drag(f32, f32, f32, f32),
    Key(Key, Modifiers),
    Select(String),
    /// Hauptwert eines Bauteils setzen (Widerstand, Kapazität, Spannung …), wie im Inspektor
    Value(String, f64),
    Scope(bool),
    Tutorial(bool),
    Timebase(f64),
    Fit,
    Wait(u32),
    Shot(String),
    Dump(String),
    /// Digitalsimulation um so viele Sekunden voranbringen
    Advance(f64),
    /// Taster an Rasterposition gedrückt halten / loslassen
    Hold(f32, f32),
    Release,
    /// Reiter der unteren Leiste: scope, analyzer, truth
    Panel(String),
    /// Digitalsimulation anhalten
    Pause(bool),
    /// Aktuellen Schaltplan als eigenes IC speichern
    SaveIc(String),
    Quit,
}

pub fn parse(src: &str) -> Result<Vec<Cmd>, String> {
    let mut out = Vec::new();
    for (ln, line) in src.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        let f = |i: usize| -> Result<f32, String> { t.get(i).and_then(|s| s.parse().ok()).ok_or(format!("Zeile {}: Zahl erwartet", ln + 1)) };
        let key = |i: usize| -> Result<Key, String> {
            let name = t.get(i).ok_or(format!("Zeile {}: Taste fehlt", ln + 1))?;
            Key::from_name(name).ok_or(format!("Zeile {}: unbekannte Taste {name}", ln + 1))
        };
        let on = |i: usize| t.get(i).is_some_and(|s| *s == "on");
        out.push(match t[0] {
            "example" => Cmd::Example(t[1..].join(" ")),
            "lesson" => Cmd::Lesson(if t.len() > 1 { t[1..].join(" ") } else { "start".into() }),
            "tool" => Cmd::Tool(t.get(1).unwrap_or(&"select").to_string()),
            "move" => Cmd::Move(f(1)?, f(2)?),
            "click" => Cmd::Click(f(1)?, f(2)?, Modifiers::NONE),
            "shiftclick" => Cmd::Click(f(1)?, f(2)?, Modifiers::SHIFT),
            "dblclick" => Cmd::DblClick(f(1)?, f(2)?),
            "clickpx" => Cmd::ClickPx(f(1)?, f(2)?),
            "dragpx" => Cmd::DragPxGrid(f(1)?, f(2)?, f(3)?, f(4)?),
            "paldrag" => {
                // Name darf Leerzeichen enthalten: die letzten beiden Wörter sind die Koordinaten
                let n = t.len();
                if n < 4 {
                    return Err(format!("Zeile {}: paldrag NAME X Y", ln + 1));
                }
                Cmd::PalDrag(t[1..n - 2].join(" "), f(n - 2)?, f(n - 1)?)
            }
            "type" => Cmd::Type(line.split_once(' ').map_or("", |x| x.1).to_string()),
            "drag" => Cmd::Drag(f(1)?, f(2)?, f(3)?, f(4)?),
            "key" => Cmd::Key(key(1)?, Modifiers::NONE),
            "cmd" => Cmd::Key(key(1)?, Modifiers::COMMAND),
            "cmdshift" => Cmd::Key(key(1)?, Modifiers::COMMAND | Modifiers::SHIFT),
            "select" => Cmd::Select(t.get(1).unwrap_or(&"").to_string()),
            "value" => Cmd::Value(
                t.get(1).unwrap_or(&"").to_string(),
                elab_app::sim::parse_si(t.get(2).unwrap_or(&"")).ok_or(format!("Zeile {}: Wert fehlt", ln + 1))?,
            ),
            "scope" => Cmd::Scope(on(1)),
            "tutorial" => Cmd::Tutorial(on(1)),
            "timebase" => Cmd::Timebase(f(1)? as f64),
            "fit" => Cmd::Fit,
            "wait" => Cmd::Wait(f(1)? as u32),
            "shot" => Cmd::Shot(t.get(1).unwrap_or(&"shot.ppm").to_string()),
            "dump" => Cmd::Dump(t.get(1).unwrap_or(&"-").to_string()),
            "advance" => Cmd::Advance(f(1)? as f64),
            "hold" => Cmd::Hold(f(1)?, f(2)?),
            "release" => Cmd::Release,
            "panel" => Cmd::Panel(t.get(1).unwrap_or(&"scope").to_string()),
            // Früher Palettenreiter; paldrag klappt die Gruppe jetzt selbst auf
            "palette" => Cmd::Wait(1),
            "pause" => Cmd::Pause(on(1)),
            "saveic" => Cmd::SaveIc(t.get(1).ok_or(format!("Zeile {}: Name fehlt", ln + 1))?.to_string()),
            "quit" => Cmd::Quit,
            other => return Err(format!("Zeile {}: unbekannter Befehl {other}", ln + 1)),
        });
    }
    Ok(out)
}

/// Abspielzustand.
pub struct Runner {
    pub cmds: VecDeque<Cmd>,
    /// Ereignisse je Frame
    pub frames: VecDeque<Vec<Event>>,
    pub wait: u32,
    pub shot_pending: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Wiederholungen des aktuellen Befehls (z. B. Palette aufklappen)
    pub retries: u32,
}

impl Runner {
    pub fn new(cmds: Vec<Cmd>) -> Self {
        Self { cmds: cmds.into(), frames: VecDeque::new(), wait: 3, shot_pending: None, retries: 0 }
    }

    pub fn busy(&self) -> bool {
        !self.frames.is_empty() || self.wait > 0 || self.shot_pending.as_ref().is_some_and(|d| !d.load(std::sync::atomic::Ordering::SeqCst))
    }

    fn btn(p: Pos2, pressed: bool, m: Modifiers) -> Event {
        Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: m }
    }

    pub fn click(&mut self, p: Pos2, m: Modifiers) {
        self.frames.push_back(vec![Event::PointerMoved(p)]);
        self.frames.push_back(vec![Self::btn(p, true, m)]);
        self.frames.push_back(vec![Self::btn(p, false, m)]);
        self.frames.push_back(vec![]);
    }

    pub fn dblclick(&mut self, p: Pos2) {
        self.frames.push_back(vec![Event::PointerMoved(p)]);
        self.frames.push_back(vec![Self::btn(p, true, Modifiers::NONE), Self::btn(p, false, Modifiers::NONE)]);
        self.frames.push_back(vec![Self::btn(p, true, Modifiers::NONE), Self::btn(p, false, Modifiers::NONE)]);
        self.frames.push_back(vec![]);
    }

    pub fn drag(&mut self, a: Pos2, b: Pos2) {
        self.frames.push_back(vec![Event::PointerMoved(a)]);
        self.frames.push_back(vec![Self::btn(a, true, Modifiers::NONE)]);
        for k in 1..=10 {
            self.frames.push_back(vec![Event::PointerMoved(a + (b - a) * (k as f32 / 10.0))]);
        }
        self.frames.push_back(vec![Self::btn(b, false, Modifiers::NONE)]);
        self.frames.push_back(vec![]);
    }

    pub fn type_text(&mut self, text: &str) {
        self.frames.push_back(vec![Event::Text(text.to_string())]);
        self.key(Key::Enter, Modifiers::NONE);
    }

    pub fn key(&mut self, key: Key, m: Modifiers) {
        let ev = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: m };
        self.frames.push_back(vec![ev(true)]);
        self.frames.push_back(vec![ev(false)]);
        self.frames.push_back(vec![]);
    }
}
