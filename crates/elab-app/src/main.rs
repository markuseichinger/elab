//! elab – Elektronik-Lernlabor: Schaltungen bauen und live simulieren.

mod editor;
mod lesson_ui;
mod log;
mod mixed;
mod panels;
mod script;
mod symbols;
mod theme;
mod tutorial;

use editor::{Editor, LiveData, Tool};
use egui::{Color32, Key, Modifiers, RichText, Sense, Stroke, StrokeKind, Vec2};
use elab_app::examples;
use elab_app::model::{DiodeType, Document, History, LedColor, PartKind, Shape};
use elab_app::netlist::{self, Netlist};
use elab_app::sim::{self, parse_si, OpView, ScopeData};
use elab_sim::fmt_si;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use theme::*;
use tutorial::Topic;

/// Reiter der Bauteilpalette.
fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let script = arg("--script").map(|p| {
        let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("Skript {p}: {e}"));
        script::parse(&src).unwrap_or_else(|e| panic!("{e}"))
    });
    let size = arg("--size").and_then(|s| s.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?]))).unwrap_or([1560.0, 980.0]);
    let file = args.iter().skip(1).find(|a| a.ends_with(".elab")).map(PathBuf::from);
    install_crash_handler(script.is_none());
    log::init();
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("elab – Elektronik-Lernlabor")
            .with_inner_size(size)
            .with_min_inner_size([1100.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native("elab", opts, Box::new(move |cc| Ok(Box::new(App::new(cc, script, file)))))
}

/// Ordner für automatische Sicherung und Absturzberichte: ~/.elab
fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join(".elab")
}

fn autosave_path() -> PathBuf {
    data_dir().join("letzte-sitzung.elab")
}

fn ic_dir() -> PathBuf {
    data_dir().join("ics")
}

/// Lädt die eigenen ICs aus ~/.elab/ics.
fn load_ic_lib() -> Vec<Document> {
    let mut v: Vec<Document> = std::fs::read_dir(ic_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "elab"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|s| Document::from_json(&s).ok())
        .filter(|d| !d.title.is_empty())
        .collect();
    v.sort_by_key(|a| a.title.to_lowercase());
    v
}

/// Dateiname für ein IC: nur Buchstaben, Ziffern, - und _.
fn ic_file(name: &str) -> PathBuf {
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    ic_dir().join(format!("{safe}.elab"))
}

/// Schreibt bei einem Absturz einen Bericht nach ~/.elab/absturz.txt und zeigt eine Meldung,
/// statt dass das Fenster kommentarlos verschwindet.
fn install_crash_handler(dialog: bool) {
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let report = format!("elab {} ist abgestürzt.\n\n{info}\n\nAblauf:\n{bt}", env!("CARGO_PKG_VERSION"));
        let _ = std::fs::create_dir_all(data_dir());
        let path = data_dir().join("absturz.txt");
        let _ = std::fs::write(&path, &report);
        log::write(&format!("ABSTURZ: {info}\n{bt}"));
        if let Some(doc) = log::last_doc() {
            let _ = std::fs::write(log::dir().join("absturz.elab"), doc);
        }
        eprintln!("{report}");
        if dialog {
            let _ = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("elab – unerwarteter Fehler")
                .set_description(format!(
                    "Das Programm muss beendet werden.\n\n{info}\n\nBericht: {}\nDeine Schaltung wurde automatisch gesichert und lässt sich über „Beispiele → Letzte Sitzung“ wiederherstellen.",
                    path.display()
                ))
                .show();
        }
    }));
}

/// Vorsätze, die für eine Einheit sinnvoll sind: (Zehnerpotenz, Zeichen).
fn unit_prefixes(unit: &str) -> Vec<(i32, &'static str)> {
    let all = [(-12, "p"), (-9, "n"), (-6, "µ"), (-3, "m"), (0, ""), (3, "k"), (6, "M")];
    let (lo, hi) = match unit {
        "Ω" => (0, 6),
        "F" => (-12, -3),
        "H" => (-6, 0),
        "V" => (-3, 3),
        "A" => (-6, 0),
        "W" => (-3, 3),
        "Hz" => (-3, 6),
        "s" => (-9, 0),
        _ => (-12, 6),
    };
    all.into_iter().filter(|p| p.0 >= lo && p.0 <= hi).collect()
}

/// Zahl ohne überflüssige Nullen, mit Komma: 4,7 · 100 · 2,2
fn fmt_mantissa(m: f64) -> String {
    let s = format!("{:.4}", m);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.replace('.', ",")
}

/// Gängige Werte zum Auswählen (Normreihe E6/E12 in den üblichen Dekaden).
fn common_values(unit: &str) -> &'static [f64] {
    match unit {
        "Ω" => &[10.0, 22.0, 47.0, 100.0, 220.0, 330.0, 470.0, 680.0, 1e3, 2.2e3, 3.3e3, 4.7e3, 6.8e3, 10e3, 22e3, 47e3, 100e3, 220e3, 470e3, 1e6],
        "F" => &[100e-12, 1e-9, 10e-9, 22e-9, 47e-9, 100e-9, 220e-9, 470e-9, 1e-6, 2.2e-6, 4.7e-6, 10e-6, 22e-6, 47e-6, 100e-6, 470e-6, 1e-3],
        "H" => &[10e-6, 100e-6, 1e-3, 10e-3, 100e-3, 1.0],
        "V" => &[1.5, 3.0, 3.3, 5.0, 9.0, 12.0, 24.0],
        "Hz" => &[1.0, 2.0, 10.0, 50.0, 100.0, 1e3, 10e3],
        _ => &[],
    }
}

/// Schonfrist in s nach einer Änderung, bevor ein überlastetes Bauteil durchbrennt
const BURN_GRACE: f64 = 2.5;

const TIMEBASES: [(f64, &str); 9] = [
    (100e-6, "100 µs"),
    (300e-6, "300 µs"),
    (1e-3, "1 ms"),
    (3e-3, "3 ms"),
    (10e-3, "10 ms"),
    (30e-3, "30 ms"),
    (100e-3, "100 ms"),
    (300e-3, "300 ms"),
    (1.0, "1 s"),
];

struct App {
    doc: Document,
    hist: History,
    ed: Editor,
    sim_on: bool,
    sim_key: Option<(Document, bool, Vec<u32>)>,
    nl: Option<Netlist>,
    op: Result<OpView, String>,
    show_scope: bool,
    /// Stromfluss als wandernde Punkte zeigen
    show_flow: bool,
    flow: Vec<elab_app::flow::FlowSeg>,
    /// Bauteile können durchbrennen
    limits_on: bool,
    thermal: elab_app::limits::Thermal,
    /// Echtzeit beim letzten Erwärmungsschritt (Wechsel setzt die Erwärmung zurück)
    thermal_rt: bool,
    /// Nach Rückgängig/Wiederholen brennt nichts durch, bis die Schaltung geändert wird
    /// (sonst ließe sich nicht hinter das Durchbrennen zurückgehen).
    burn_hold: Option<Document>,
    /// Letzter Stand und Zeitpunkt einer Änderung (Schonfrist vor dem Durchbrennen)
    edit_doc: Option<Document>,
    edit_t: f64,
    grace_warned: bool,
    scope_t: f64,
    /// Echtzeit-Oszilloskop: auf steigende Flanke triggern, Anzeige angehalten
    scope_trigger: bool,
    scope_hold: bool,
    scope_key: Option<(Document, u64)>,
    scope: Option<Result<ScopeData, String>>,
    topic: Topic,
    show_tutorial: bool,
    file: Option<PathBuf>,
    saved: Document,
    title_sent: String,
    bufs: HashMap<String, String>,
    buf_sel: Option<u32>,
    flash: Option<(String, Color32, f64)>,
    script: Option<script::Runner>,
    // Digital
    dsim: Option<elab_sim::digital::DigitalSim>,
    dsim_key: Option<Document>,
    pressed: std::collections::HashSet<u32>,
    dig_speed: f64,
    dig_paused: bool,
    an_window: f64,
    truth: Option<Result<sim::TruthTable, String>>,
    bottom_tab: panels::BottomTab,
    /// Interne Zwischenablage und Zahl der Einfügungen (jede weiter versetzt)
    clip: Option<elab_app::model::Clip>,
    paste_n: i32,
    /// Palette als Liste mit Beschreibung statt als Kacheln
    pal_list: bool,
    /// Suchtext der Bauteilpalette
    pal_filter: String,
    /// Testskript: Palettenkachel, die aufgeklappt und ins Bild gerollt werden soll
    pal_focus: Option<String>,
    // Gemischte Schaltungen (Echtzeit)
    msim: Option<elab_sim::MixedSim>,
    msim_key: Option<Document>,
    /// Zählt Neuaufbauten der Netzliste; `msim_gen` = zuletzt übernommener Stand
    nl_gen: u64,
    msim_gen: u64,
    /// Logikpegel je Netz (rein digitale Netze einer gemischten Schaltung)
    mixed_logic: Vec<elab_sim::digital::Logic>,
    /// Pegel an den Anschlüssen von ICs und gekoppelten Logikbausteinen
    pin_logic: HashMap<u32, Vec<Option<elab_sim::digital::Logic>>>,
    /// Simulation kommt nicht hinterher
    msim_lag: bool,
    /// Zuletzt „langsamer als Echtzeit“ (Zeitpunkt), Warnungen mit letztem Auftreten
    lag_seen: f64,
    warn_seen: HashMap<String, f64>,
    /// Bibliothek eigener ICs (Titel = Name)
    ic_lib: Vec<Document>,
    /// Dialog „Als IC speichern“: eingegebener Name
    save_ic: Option<String>,
    /// Laufende geführte Übung
    lesson: Option<lesson_ui::LessonRun>,
    /// Übungen, auf die schon hingewiesen wurde
    lesson_hinted: std::collections::HashSet<String>,
    /// Vom Testskript gedrückter Taster
    script_pressed: Option<u32>,
    /// Palettenkacheln des letzten Bildes (für Testskripte)
    tile_rects: Vec<(String, egui::Rect)>,
    tile_rects_prev: Vec<(String, egui::Rect)>,
    /// Zuletzt automatisch gesicherter Stand und Zeitpunkt
    autosaved: Option<Document>,
    autosave_due: f64,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, script: Option<Vec<script::Cmd>>, file: Option<PathBuf>) -> Self {
        // Bedienleisten hell oder dunkel wie das System, beide mit kontrastreichem Text
        cc.egui_ctx.set_visuals_of(egui::Theme::Dark, theme::dark_visuals());
        cc.egui_ctx.set_visuals_of(egui::Theme::Light, theme::light_visuals());
        match std::env::var("ELAB_THEME").as_deref() {
            Ok("light") => cc.egui_ctx.set_theme(egui::Theme::Light),
            Ok("dark") => cc.egui_ctx.set_theme(egui::Theme::Dark),
            _ => {}
        }
        let ex = examples::all().remove(0);
        let mut app = Self {
            saved: ex.doc.clone(),
            doc: ex.doc,
            hist: History::default(),
            ed: Editor::default(),
            sim_on: true,
            sim_key: None,
            nl: None,
            op: Err(String::new()),
            show_scope: true,
            show_flow: true,
            flow: Vec::new(),
            limits_on: true,
            thermal: Default::default(),
            thermal_rt: false,
            burn_hold: None,
            edit_doc: None,
            edit_t: 0.0,
            grace_warned: false,
            scope_t: ex.scope_t,
            scope_trigger: true,
            scope_hold: false,
            scope_key: None,
            scope: None,
            topic: Topic::Welcome,
            show_tutorial: true,
            file: None,
            title_sent: String::new(),
            bufs: HashMap::new(),
            buf_sel: None,
            flash: None,
            script: script.map(script::Runner::new),
            dsim: None,
            dsim_key: None,
            pressed: Default::default(),
            dig_speed: 1.0,
            dig_paused: false,
            an_window: 10.0,
            truth: None,
            bottom_tab: panels::BottomTab::Scope,
            clip: None,
            paste_n: 0,
            pal_list: false,
            pal_filter: String::new(),
            pal_focus: None,
            msim: None,
            msim_key: None,
            nl_gen: 0,
            msim_gen: 0,
            mixed_logic: Vec::new(),
            pin_logic: HashMap::new(),
            msim_lag: false,
            lag_seen: -10.0,
            warn_seen: HashMap::new(),
            ic_lib: load_ic_lib(),
            save_ic: None,
            lesson: None,
            lesson_hinted: Default::default(),
            script_pressed: None,
            tile_rects: Vec::new(),
            tile_rects_prev: Vec::new(),
            autosaved: None,
            autosave_due: 0.0,
        };
        if let Some(f) = file {
            app.open_path(f);
        }
        app
    }

    // ------------------------------------------------------------------
    // Dokument-Verwaltung
    // ------------------------------------------------------------------

    fn replace_doc(&mut self, doc: Document) {
        self.hist.checkpoint(&self.doc);
        self.doc = doc;
        self.ed.clear_selection();
        self.ed.tool = Tool::Select;
        self.ed.fit_request = true;
    }

    fn load_example(&mut self, idx: usize) {
        if let Some(ex) = examples::all().into_iter().nth(idx) {
            let sequential = ex
                .doc
                .parts
                .iter()
                .any(|p| matches!(p.kind, PartKind::Clock { .. } | PartKind::DFlipFlop | PartKind::JkFlipFlop | PartKind::Counter4));
            self.replace_doc(ex.doc);
            self.saved = self.doc.clone();
            self.file = None;
            self.scope_t = ex.scope_t;
            self.bottom_tab = if ex.digital {
                if sequential {
                    panels::BottomTab::Analyzer
                } else {
                    panels::BottomTab::Truth
                }
            } else {
                panels::BottomTab::Scope
            };
            self.show_scope = true;
            self.topic = if ex.digital {
                if sequential {
                    Topic::Analyzer
                } else {
                    Topic::TruthTable
                }
            } else {
                Topic::Welcome
            };
            self.flash(format!("Beispiel „{}“: {}", ex.title, ex.summary), BLUE);
        }
    }

    /// Erwärmung nachführen und überlastete Bauteile durchbrennen lassen.
    fn check_limits(&mut self, ctx: &egui::Context) {
        let rt = self.msim.is_some();
        if !self.limits_on || rt != self.thermal_rt {
            self.thermal.reset();
            self.thermal_rt = rt;
        }
        let Ok(view) = &self.op else {
            self.thermal.reset();
            return;
        };
        if !self.limits_on || !self.sim_on {
            return;
        }
        let t = match &self.msim {
            Some(ms) => ms.time(),
            None => ctx.input(|i| i.time),
        };
        let burned = self.thermal.update(&self.doc, view, Some(t));
        if self.thermal.map().values().any(|&h| h > 0.3) {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        if self.burn_hold.as_ref().is_some_and(|d| *d != self.doc) {
            self.burn_hold = None;
        }
        if burned.is_empty() || self.burn_hold.is_some() {
            return;
        }
        // Schonfrist nach jeder Änderung: Wer erst die Leitung zieht und dann den Widerstand
        // einsetzt, soll nicht schon dazwischen alles zerstören. Stattdessen eine Warnung.
        let now = ctx.input(|i| i.time);
        if self.edit_doc.as_ref() != Some(&self.doc) {
            self.edit_doc = Some(self.doc.clone());
            self.edit_t = now;
            self.grace_warned = false;
        }
        let left = BURN_GRACE - (now - self.edit_t);
        if left > 0.0 {
            if !self.grace_warned {
                self.grace_warned = true;
                let names: Vec<String> = burned.iter().filter_map(|(id, why)| self.doc.part(*id).map(|p| format!("{} ({why})", p.name))).collect();
                self.flash(format!("🔥 Überlastet: {}. Ändere die Schaltung, sonst brennt es gleich durch!", names.join(", ")), RED);
                self.topic = Topic::Limits;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        self.hist.checkpoint(&self.doc);
        let mut msg = Vec::new();
        for (id, why) in &burned {
            if let Some(p) = self.doc.part_mut(*id) {
                p.broken = true;
                msg.push(format!("{} ({why})", p.name));
            }
        }
        self.thermal.reset();
        let verb = if burned.len() == 1 { "ist" } else { "sind" };
        self.flash(format!("💥 {} {verb} durchgebrannt. Ersetzen im Inspektor, ⌘Z macht es rückgängig.", msg.join(", ")), RED);
        self.topic = Topic::Limits;
    }

    fn flash(&mut self, msg: String, col: Color32) {
        self.flash = Some((msg, col, 0.0));
    }

    fn new_doc(&mut self) {
        self.replace_doc(Document::default());
        self.saved = self.doc.clone();
        self.file = None;
        self.topic = Topic::Editor;
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("elab-Schaltung", &["elab"]).pick_file() {
            self.open_path(p);
        }
    }

    fn open_path(&mut self, p: PathBuf) {
        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|s| Document::from_json(&s)) {
            Ok(doc) => {
                self.replace_doc(doc);
                self.saved = self.doc.clone();
                self.flash(format!("Geöffnet: {}", p.display()), GREEN);
                self.file = Some(p);
            }
            Err(e) => self.flash(format!("Öffnen fehlgeschlagen: {e}"), RED),
        }
    }

    fn save(&mut self, save_as: bool) {
        let path = match (&self.file, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => {
                let name = if self.doc.title.is_empty() { "Schaltung".to_string() } else { self.doc.title.clone() };
                rfd::FileDialog::new().add_filter("elab-Schaltung", &["elab"]).set_file_name(format!("{name}.elab")).save_file()
            }
        };
        let Some(mut path) = path else { return };
        if path.extension().is_none() {
            path.set_extension("elab");
        }
        match std::fs::write(&path, self.doc.to_json()) {
            Ok(()) => {
                self.saved = self.doc.clone();
                self.flash(format!("Gespeichert: {}", path.display()), GREEN);
                self.file = Some(path);
            }
            Err(e) => self.flash(format!("Speichern fehlgeschlagen: {e}"), RED),
        }
    }

    /// Sichert den Schaltplan gut eine Sekunde nach der letzten Änderung nach ~/.elab.
    fn autosave(&mut self, ctx: &egui::Context) {
        if self.script.is_some() || self.autosaved.as_ref() == Some(&self.doc) {
            return;
        }
        let now = ctx.input(|i| i.time);
        if self.autosave_due == 0.0 {
            self.autosave_due = now + 1.5;
        }
        if now >= self.autosave_due {
            let _ = std::fs::create_dir_all(data_dir());
            let _ = std::fs::write(autosave_path(), self.doc.to_json());
            self.autosaved = Some(self.doc.clone());
            self.autosave_due = 0.0;
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
    }

    fn undo(&mut self) {
        if self.hist.undo(&mut self.doc) {
            self.ed.prune(&self.doc);
            self.burn_hold = Some(self.doc.clone());
        }
    }

    fn redo(&mut self) {
        if self.hist.redo(&mut self.doc) {
            self.ed.prune(&self.doc);
            self.burn_hold = Some(self.doc.clone());
        }
    }

    fn global_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (redo, redo2, undo, save, open, new) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z),
                i.consume_key(Modifiers::COMMAND, Key::Y),
                i.consume_key(Modifiers::COMMAND, Key::Z),
                i.consume_key(Modifiers::COMMAND, Key::S),
                i.consume_key(Modifiers::COMMAND, Key::O),
                i.consume_key(Modifiers::COMMAND, Key::N),
            )
        });
        if redo || redo2 {
            self.redo();
        }
        if undo {
            self.undo();
        }
        if save {
            self.save(false);
        }
        if open {
            self.open_dialog();
        }
        if new {
            self.new_doc();
        }
        // Kopieren, Ausschneiden, Einfügen (⌘/Strg + C, X, V). egui meldet sie als eigene
        // Ereignisse; Strg+C auf dem Mac kommt als Taste.
        let (mut copy, mut cut, mut paste) = (false, false, None::<String>);
        ctx.input_mut(|i| {
            for e in &i.events {
                match e {
                    egui::Event::Copy => copy = true,
                    egui::Event::Cut => cut = true,
                    egui::Event::Paste(t) => paste = Some(t.clone()),
                    _ => {}
                }
            }
            // Tastenkombinationen nur, wenn kein eigenes Ereignis kam (sonst doppelt)
            for m in [Modifiers::CTRL, Modifiers::COMMAND] {
                let (c, x, v) = (i.consume_key(m, Key::C), i.consume_key(m, Key::X), i.consume_key(m, Key::V));
                copy |= c && !cut;
                cut |= x && !copy;
                if v && paste.is_none() {
                    paste = Some(String::new());
                }
            }
        });
        if copy || cut {
            self.copy_selection(ctx, cut);
        }
        if let Some(text) = paste {
            self.paste_clip(&text);
        }
    }

    fn copy_selection(&mut self, ctx: &egui::Context, cut: bool) {
        let clip = self.doc.copy(&self.ed.sel_parts, &self.ed.sel_wires);
        if clip.is_empty() {
            return;
        }
        ctx.copy_text(clip.to_text());
        let n = clip.parts.len();
        self.clip = Some(clip);
        self.paste_n = 0;
        if cut {
            self.hist.checkpoint(&self.doc);
            self.doc.remove(&self.ed.sel_parts, &self.ed.sel_wires);
            self.ed.clear_selection();
            self.flash(format!("{n} Bauteile ausgeschnitten. Einfügen mit ⌘V."), BLUE);
        } else {
            self.flash(format!("{n} Bauteile kopiert. Einfügen mit ⌘V."), BLUE);
        }
    }

    /// Fügt die Zwischenablage ein: zuerst elabs eigene, sonst ein Ausschnitt aus dem
    /// Systemspeicher (z. B. aus einem anderen elab-Fenster).
    fn paste_clip(&mut self, text: &str) {
        let clip = elab_app::model::Clip::from_text(text).or_else(|| self.clip.clone());
        let Some(clip) = clip.filter(|c| !c.is_empty()) else { return };
        self.paste_n += 1;
        // Unter den Mauszeiger, sonst schräg versetzt neben das Original
        let off = match (self.ed.pointer, clip.center()) {
            (Some(m), Some(c)) => m - c,
            // Maus nicht über dem Schaltplan: rechts neben die vorhandene Schaltung
            _ => {
                let right = self.doc.bbox().map_or(0, |b| b.2.ceil() as i32);
                let left = clip.parts.iter().map(|p| p.bbox().0.floor() as i32).chain(clip.wires.iter().map(|w| w.0.x.min(w.1.x))).min().unwrap_or(0);
                self.ed.fit_request = true;
                elab_app::model::P::new(right - left + 4, 0)
            }
        };
        self.hist.checkpoint(&self.doc);
        let (parts, wires) = self.doc.paste(&clip, off);
        self.ed.clear_selection();
        self.ed.sel_parts = parts;
        self.ed.sel_wires = wires;
        self.ed.tool = Tool::Select;
    }

    // ------------------------------------------------------------------
    // Simulation
    // ------------------------------------------------------------------

    fn update_sim(&mut self) {
        let mut pressed: Vec<u32> = self.pressed.iter().copied().collect();
        pressed.sort();
        let key_ok = self.sim_key.as_ref().is_some_and(|(d, on, p)| *d == self.doc && *on == self.sim_on && *p == pressed);
        if !key_ok {
            log::set_doc(serde_json::to_string(&self.doc).unwrap_or_default());
            let nl = netlist::build_with(&self.doc, &self.pressed);
            self.nl_gen += 1;
            self.op = if !self.sim_on {
                Err(String::new())
            } else if sim::realtime(&nl) {
                // läuft in Echtzeit (update_mixed)
                match &nl.error {
                    Some(e) => Err(e.clone()),
                    None => self.op.clone().or(Err(String::new())),
                }
            } else {
                sim::operating_point(&nl)
            };
            self.nl = Some(nl);
            self.sim_key = Some((self.doc.clone(), self.sim_on, pressed));
        }
        let t_bits = self.scope_t.to_bits();
        let stale = self.scope_key.as_ref().is_none_or(|(d, t)| *d != self.doc || *t != t_bits);
        // Ohne Echtzeit ist der Verlauf konstant und schnell berechnet. In Echtzeit zeigt das
        // Oszilloskop die laufende Simulation (scope_ui).
        let realtime = self.nl.as_ref().is_some_and(sim::realtime);
        if self.show_scope && stale && !realtime {
            if let Some(nl) = &self.nl {
                self.scope = Some(sim::record(&self.doc, nl, self.scope_t));
            }
            self.scope_key = Some((self.doc.clone(), t_bits));
        }
    }

    // ------------------------------------------------------------------
    // Kopfleiste
    // ------------------------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("elab").size(20.0).strong().color(ACCENT));
            ui.add_space(10.0);
            let big = |t: &str| egui::Button::new(RichText::new(t).size(15.0));
            if ui.add(big("🗋 Neu")).on_hover_text("Leerer Schaltplan (⌘N). Rückgängig holt den alten zurück.").clicked() {
                self.new_doc();
            }
            if ui.add(big("📂 Öffnen")).on_hover_text("Schaltung laden (⌘O)").clicked() {
                self.open_dialog();
            }
            if ui.add(big("💾 Speichern")).on_hover_text("Speichern (⌘S), Rechtsklick: speichern unter").clicked() {
                self.save(false);
            }
            ui.menu_button(RichText::new("📚 Beispiele").size(15.0), |ui| {
                if autosave_path().exists()
                    && ui.button(RichText::new("↺ Letzte Sitzung").size(14.0)).on_hover_text("Automatisch gesicherter Stand").clicked()
                {
                    match std::fs::read_to_string(autosave_path()).map_err(|e| e.to_string()).and_then(|s| Document::from_json(&s)) {
                        Ok(doc) => {
                            self.replace_doc(doc);
                            self.flash("Letzte Sitzung wiederhergestellt.".into(), GREEN);
                        }
                        Err(e) => self.flash(format!("Wiederherstellen fehlgeschlagen: {e}"), RED),
                    }
                    ui.close();
                }
                ui.separator();
                let all = examples::all();
                for (cat, desc) in examples::CATEGORIES {
                    let n = all.iter().filter(|e| e.category == cat).count();
                    ui.menu_button(RichText::new(format!("{cat}  ({n})")).size(14.0), |ui| {
                        ui.label(RichText::new(desc).small().color(muted()));
                        ui.separator();
                        for (k, ex) in all.iter().enumerate().filter(|(_, e)| e.category == cat) {
                            if ui.button(RichText::new(ex.title).size(14.0)).on_hover_text(ex.summary).clicked() {
                                self.load_example(k);
                                ui.close();
                            }
                        }
                    });
                }
                ui.separator();
                ui.menu_button(RichText::new("🧩 Geführte Übungen").size(14.0), |ui| {
                    ui.label(RichText::new("Schritt für Schritt ein Bauteil richtig beschalten").small().color(muted()));
                    ui.separator();
                    for l in elab_app::lessons::all() {
                        if ui.button(RichText::new(&l.title).size(14.0)).on_hover_text(&l.intro).clicked() {
                            self.start_lesson(&l.start_kind);
                            ui.close();
                        }
                    }
                });
                ui.menu_button(RichText::new("🎯 Aufgaben").size(14.0), |ui| {
                    ui.label(RichText::new("Selbst bauen, elab prüft das Ergebnis").small().color(muted()));
                    ui.separator();
                    for t in elab_app::lessons::tasks() {
                        if ui.button(RichText::new(&t.title).size(14.0)).on_hover_text(&t.intro).clicked() {
                            self.start_task(t);
                            ui.close();
                        }
                    }
                });
            });
            ui.separator();
            if ui.add_enabled(self.hist.can_undo(), big("⟲")).on_hover_text("Rückgängig (⌘Z)").clicked() {
                self.undo();
            }
            if ui.add_enabled(self.hist.can_redo(), big("⟳")).on_hover_text("Wiederholen (⇧⌘Z)").clicked() {
                self.redo();
            }
            ui.separator();
            let r = ui
                .add(egui::Button::new(RichText::new("⚡ Live-Simulation").size(15.0)).selected(self.sim_on))
                .on_hover_text("Arbeitspunkt nach jeder Änderung neu berechnen und Spannungen einfärben");
            if r.clicked() {
                self.sim_on = !self.sim_on;
                self.topic = Topic::OperatingPoint;
            }
            let r = ui
                .add(egui::Button::new(RichText::new("〰 Stromfluss").size(15.0)).selected(self.show_flow))
                .on_hover_text("Strom als wandernde Punkte zeigen (technische Stromrichtung, von + nach −). Je schneller, desto mehr Strom.");
            if r.clicked() {
                self.show_flow = !self.show_flow;
                self.topic = Topic::Kirchhoff;
            }
            let r = ui
                .add(egui::Button::new(RichText::new("🔥 Grenzwerte").size(15.0)).selected(self.limits_on))
                .on_hover_text("Bauteile brennen bei zu viel Strom oder Leistung durch, wie in echt");
            if r.clicked() {
                self.limits_on = !self.limits_on;
                self.topic = Topic::Limits;
            }
            let r = ui
                .add(egui::Button::new(RichText::new("📈 Messgeräte").size(15.0)).selected(self.show_scope))
                .on_hover_text("Oszilloskop, Logikanalysator und Wahrheitstabelle");
            if r.clicked() {
                self.show_scope = !self.show_scope;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.show_tutorial { "🎓 Tutorial an" } else { "🎓 Tutorial" };
                if ui.add(egui::Button::new(RichText::new(label).size(15.0)).selected(self.show_tutorial)).clicked() {
                    self.show_tutorial = !self.show_tutorial;
                }
                ui.separator();
                if ui.add(big("⤢ Einpassen")).on_hover_text("Ganze Schaltung anzeigen").clicked() {
                    self.ed.fit_request = true;
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            match &self.op {
                Err(e) if e.is_empty() => {
                    ui.label(RichText::new("Simulation aus").color(muted()).strong());
                }
                Err(e) => {
                    ui.label(RichText::new(format!("⚠ {e}")).color(RED).strong());
                }
                Ok(_) if self.nl.as_ref().is_some_and(|nl| nl.simulated.is_empty() && nl.excluded.is_empty() && nl.digital.is_some()) => {
                    ui.label(RichText::new("✔ Digitalschaltung").color(GREEN).strong());
                }
                Ok(_) if self.msim.is_some() => self.mixed_status(ui),
                Ok(op) => {
                    let nl = self.nl.as_ref().unwrap();
                    let n_nets = nl.nets.iter().filter(|n| n.reachable && !n.is_gnd).count();
                    ui.label(RichText::new("✔ Arbeitspunkt berechnet").color(GREEN).strong());
                    ui.label(
                        RichText::new(format!(
                            "{} Bauteile · {} Knoten · {} Newton-Schritte · {}",
                            nl.simulated.len(),
                            n_nets,
                            op.newton,
                            fmt_si(op.millis * 1e-3, "s")
                        ))
                        .color(muted()),
                    );
                    if !nl.bridged.is_empty() {
                        let names: Vec<String> = nl.bridged.iter().filter_map(|id| self.doc.part(*id)).map(|p| p.name.clone()).collect();
                        ui.label(RichText::new(format!("· überbrückt (stromlos): {}", names.join(", "))).color(ACCENT));
                    }
                    if !nl.excluded.is_empty() {
                        let names: Vec<String> = nl.excluded.iter().filter_map(|id| self.doc.part(*id)).map(|p| p.name.clone()).collect();
                        ui.label(RichText::new(format!("· nicht verbunden: {}", names.join(", "))).color(ACCENT));
                    }
                }
            }
            if let (Some(ds), Some(nl)) = (&self.dsim, &self.nl) {
                let d = nl.digital.as_ref().unwrap();
                ui.separator();
                ui.label(RichText::new(format!("Digital: {} Bausteine · t = {}", d.comps.len(), fmt_si(ds.time as f64 * 1e-9, "s"))).color(muted()));
                let conflicts = ds.conflicts();
                if !conflicts.is_empty() {
                    let names: Vec<&str> = conflicts.iter().map(|&n| nl.nets[n].name.as_str()).collect();
                    ui.label(RichText::new(format!("⚠ Buskonflikt an {}", names.join(", "))).color(RED));
                }
                if ds.overloaded {
                    ui.label(RichText::new("⚠ Dauerschwingung").color(RED));
                }
            }
            if let Some((msg, col, t)) = &mut self.flash {
                let now = ui.input(|i| i.time);
                if *t == 0.0 {
                    *t = now + 6.0;
                }
                if now < *t {
                    ui.separator();
                    ui.label(RichText::new(msg.as_str()).color(*col));
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
                } else {
                    self.flash = None;
                }
            }
        });
        ui.add_space(2.0);
    }

    // ------------------------------------------------------------------
    // Palette
    // ------------------------------------------------------------------

    fn palette(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Bauteile").size(17.0).strong().color(ACCENT));
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(RichText::new("⬉ Auswahl").size(14.0)).selected(self.ed.tool == Tool::Select))
                .on_hover_text("Auswählen, verschieben, vom Anschluss aus verdrahten (Esc)")
                .clicked()
            {
                self.ed.tool = Tool::Select;
            }
            if ui
                .add(egui::Button::new(RichText::new("╱ Draht").size(14.0)).selected(self.ed.tool == Tool::Wire))
                .on_hover_text("Drähte Ecke für Ecke verlegen (W)")
                .clicked()
            {
                self.ed.tool = Tool::Wire;
                self.topic = Topic::Editor;
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.pal_filter).hint_text("🔍 Bauteil suchen").desired_width(170.0));
            if !self.pal_filter.is_empty() && ui.small_button("✕").clicked() {
                self.pal_filter.clear();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.selectable_label(self.pal_list, "☰").on_hover_text("Liste mit Beschreibung").clicked() {
                    self.pal_list = true;
                }
                if ui.selectable_label(!self.pal_list, "▦").on_hover_text("Kacheln").clicked() {
                    self.pal_list = false;
                }
            });
        });
        let mut groups = PartKind::palette();
        let mut own = vec![PartKind::Port];
        own.extend(self.ic_lib.iter().map(|d| PartKind::Custom { name: d.title.clone(), def: Box::new(d.clone()) }));
        groups.push(("Eigene ICs", own));
        let filter = self.pal_filter.trim().to_lowercase();
        let matches = |k: &PartKind| filter.is_empty() || tile_key(k).to_lowercase().contains(&filter) || k.prefix().to_lowercase() == filter;
        let mut remove_ic: Option<String> = None;
        let mut save_ic = false;
        self.tile_rects_prev = std::mem::take(&mut self.tile_rects);
        for (cat, kinds) in groups {
            let kinds: Vec<PartKind> = kinds.into_iter().filter(|k| matches(k)).collect();
            if kinds.is_empty() && !(cat == "Eigene ICs" && filter.is_empty()) {
                continue;
            }
            let focus = self.pal_focus.as_ref().is_some_and(|f| kinds.iter().any(|k| tile_key(k) == *f));
            let mut header =
                egui::CollapsingHeader::new(RichText::new(format!("{cat}  ({})", kinds.len())).size(14.0).strong()).id_salt(cat).default_open(true);
            if !filter.is_empty() || focus {
                header = header.open(Some(true));
            }
            let list = self.pal_list;
            header.show(ui, |ui| {
                let layout = |ui: &mut egui::Ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                    for kind in kinds {
                        let selected = matches!(&self.ed.tool, Tool::Place(k) if *k == kind);
                        let r = if list { list_row(ui, &kind, selected) } else { tile(ui, &kind, selected) }
                            .on_hover_text(format!("{}: in den Schaltplan ziehen oder anklicken und absetzen", kind.label()));
                        let visible = ui.clip_rect().contains_rect(r.rect);
                        if self.pal_focus.as_deref() == Some(tile_key(&kind).as_str()) && !visible {
                            r.scroll_to_me(Some(egui::Align::Center));
                        }
                        if visible {
                            self.tile_rects.push((tile_key(&kind), r.rect));
                        }
                        if r.drag_started() {
                            self.topic = Topic::of(&kind);
                            self.ed.place_rot = 0;
                            self.ed.tool = Tool::Select;
                            self.ed.palette_drag = Some(kind.clone());
                        }
                        if r.dragged() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                        }
                        if let PartKind::Custom { name, .. } = &kind {
                            let name = name.clone();
                            r.context_menu(|ui| {
                                if ui.button("Aus der Bibliothek entfernen").clicked() {
                                    remove_ic = Some(name.clone());
                                    ui.close();
                                }
                            });
                        }
                        if r.clicked() {
                            self.topic = Topic::of(&kind);
                            self.ed.tool = Tool::Place(kind);
                            self.ed.place_rot = 0;
                        }
                    }
                };
                if list {
                    ui.vertical(layout);
                } else {
                    ui.horizontal_wrapped(layout);
                }
                if cat == "Eigene ICs" {
                    save_ic = ui
                        .button("💾 Schaltplan als IC speichern …")
                        .on_hover_text("Die aktuelle Schaltung wird ein Baustein. Ihre IC-Anschlüsse werden die Pins.")
                        .clicked();
                }
            });
        }
        if save_ic {
            self.save_ic = Some(if self.doc.title.is_empty() { String::new() } else { self.doc.title.clone() });
            self.topic = Topic::CustomIc;
        }
        if let Some(name) = remove_ic {
            let _ = std::fs::remove_file(ic_file(&name));
            self.ic_lib.retain(|d| d.title != name);
            self.flash(format!("„{name}“ aus der Bibliothek entfernt. Eingesetzte Exemplare bleiben erhalten."), ACCENT);
        }
    }

    /// Speichert den aktuellen Schaltplan als eigenes IC in der Bibliothek.
    fn save_custom_ic(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Bitte einen Namen eingeben.".into());
        }
        let ports = elab_app::model::custom_ports(&self.doc);
        if ports.is_empty() {
            return Err("Die Schaltung hat keine IC-Anschlüsse. Setze mindestens ein Bauteil „IC-Anschluss“.".into());
        }
        let mut def = self.doc.clone();
        def.title = name.to_string();
        std::fs::create_dir_all(ic_dir()).map_err(|e| e.to_string())?;
        std::fs::write(ic_file(name), def.to_json()).map_err(|e| e.to_string())?;
        self.ic_lib.retain(|d| d.title != name);
        self.ic_lib.push(def);
        self.ic_lib.sort_by_key(|a| a.title.to_lowercase());
        self.flash(format!("IC „{name}“ gespeichert ({} Anschlüsse). Es liegt jetzt unter „Eigene ICs“.", ports.len()), GREEN);
        Ok(())
    }

    /// Dialog zum Speichern als IC.
    fn save_ic_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut name) = self.save_ic.take() else { return };
        let ports = elab_app::model::custom_ports(&self.doc);
        let mut open = true;
        let mut cancel = false;
        let mut done = false;
        egui::Window::new("Schaltplan als IC speichern").collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(200.0).hint_text("z. B. Halbaddierer"));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    done = true;
                }
            });
            if ports.is_empty() {
                ui.label(
                    RichText::new("Noch keine IC-Anschlüsse: Setze das Bauteil „IC-Anschluss“ an jede Leitung, die nach außen führen soll.")
                        .color(RED),
                );
            } else {
                let side = |s: elab_app::model::Side| ports.iter().filter(|p| p.2 == s).map(|p| p.1.clone()).collect::<Vec<_>>().join(", ");
                use elab_app::model::Side;
                for (s, label) in [(Side::Left, "links"), (Side::Right, "rechts"), (Side::Top, "oben"), (Side::Bottom, "unten")] {
                    let t = side(s);
                    if !t.is_empty() {
                        ui.label(RichText::new(format!("Anschlüsse {label}: {t}")).color(muted()));
                    }
                }
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(!ports.is_empty() && !name.trim().is_empty(), egui::Button::new("💾 Speichern")).clicked() {
                    done = true;
                }
                if ui.button("Abbrechen").clicked() {
                    cancel = true;
                }
            });
        });
        if cancel {
            open = false;
        }
        if done {
            match self.save_custom_ic(&name) {
                Ok(()) => return,
                Err(e) => self.flash(e, RED),
            }
        }
        if open {
            self.save_ic = Some(name);
        }
    }

    // ------------------------------------------------------------------
    // Inspektor
    // ------------------------------------------------------------------

    /// Textfeld mit SI-Eingabe. Gibt den neuen Wert zurück, wenn er bestätigt wurde.
    /// Eingabe eines Werts: Zahl, Vorsatz aus der Liste (µ, m, k …) und gängige Werte zur
    /// Auswahl. In das Zahlenfeld darf man auch „4k7“ oder „100n“ tippen.
    fn si_field(&mut self, ui: &mut egui::Ui, label: &str, unit: &str, value: f64, positive: bool, key: &str) -> Option<f64> {
        let prefixes = unit_prefixes(unit);
        let k = format!("{key}:{}", value.to_bits());
        let ke = format!("{k}:e");
        // Vorsatz passend zum Wert (aus der Liste der Einheit)
        let auto_exp = {
            let e = if value == 0.0 { 0 } else { ((value.abs().log10() + 1e-9).floor() as i32).div_euclid(3) * 3 };
            let (lo, hi) = (prefixes.first().map_or(0, |p| p.0), prefixes.last().map_or(0, |p| p.0));
            e.clamp(lo, hi)
        };
        let exp: i32 = self.bufs.get(&ke).and_then(|s| s.parse().ok()).unwrap_or(auto_exp);
        let buf = self.bufs.entry(k.clone()).or_insert_with(|| fmt_mantissa(value / 10f64.powi(exp))).clone();
        let mut buf_new = buf.clone();
        let mut exp_new = exp;
        let mut result = None;
        ui.horizontal(|ui| {
            ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new(label).color(muted())));
            let resp = ui.add(egui::TextEdit::singleline(&mut buf_new).desired_width(64.0));
            let cur = prefixes.iter().find(|p| p.0 == exp).map_or_else(|| unit.to_string(), |p| format!("{}{unit}", p.1));
            egui::ComboBox::from_id_salt(format!("pre:{key}")).width(58.0).selected_text(cur).show_ui(ui, |ui| {
                for &(e, p) in &prefixes {
                    ui.selectable_value(&mut exp_new, e, format!("{p}{unit}"));
                }
            });
            // Zahl mit eigenem Vorsatz („4k7“) gilt absolut, sonst Zahl × gewählter Vorsatz
            let typed_prefix = buf_new.chars().any(|c| c.is_alphabetic() || c == 'µ');
            let parsed = parse_si(&buf_new)
                .map(|m| if typed_prefix { m } else { m * 10f64.powi(exp_new) })
                .filter(|v| v.is_finite() && (!positive || *v > 0.0));
            let presets = common_values(unit);
            if !presets.is_empty() {
                egui::ComboBox::from_id_salt(format!("std:{key}"))
                    .width(24.0)
                    .selected_text("")
                    .show_ui(ui, |ui| {
                        ui.label(RichText::new("Gängige Werte").small().color(muted()));
                        for &v in presets {
                            if ui.selectable_label(v == value, fmt_si(v, unit)).clicked() {
                                result = Some(v);
                            }
                        }
                    })
                    .response
                    .on_hover_text("Gängige Werte");
            }
            match parsed {
                None => {
                    ui.label(RichText::new("⚠").color(RED)).on_hover_text("Zahl nicht lesbar. Beispiele: 4,7 oder 100 – oder direkt 4k7, 100n, 2,2m");
                }
                Some(v) if (resp.lost_focus() || exp_new != exp) && v != value && result.is_none() => result = Some(v),
                _ => {}
            }
        });
        if result.is_none() {
            self.bufs.insert(k, buf_new);
            self.bufs.insert(ke, exp_new.to_string());
        }
        result
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        let sel = self.ed.single_selected();
        if sel != self.buf_sel {
            self.bufs.clear();
            self.buf_sel = sel;
        }
        ui.label(RichText::new("Inspektor").size(17.0).strong().color(ACCENT));
        let Some(id) = sel else {
            let (np, nw) = (self.ed.sel_parts.len(), self.ed.sel_wires.len());
            if np + nw > 0 {
                ui.label(format!("{np} Bauteile und {nw} Drähte ausgewählt."));
                ui.horizontal(|ui| {
                    if np > 0 && ui.button("⟳ Drehen (R)").clicked() {
                        self.hist.checkpoint(&self.doc);
                        let ids = self.ed.sel_parts.clone();
                        self.doc.transform_parts(&ids, &Default::default(), Default::default(), |p| p.rot = (p.rot + 1) % 4);
                    }
                    if ui.button("🗑 Löschen (Entf)").clicked() {
                        self.hist.checkpoint(&self.doc);
                        self.doc.remove(&self.ed.sel_parts, &self.ed.sel_wires);
                        self.ed.clear_selection();
                    }
                });
            } else {
                ui.label(RichText::new("Klicke ein Bauteil an, um seine Werte zu sehen und zu ändern.").color(muted()));
            }
            return;
        };
        let Some(part) = self.doc.part(id).cloned() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(part.kind.label()).size(15.0).strong());
            if ui.small_button("?").on_hover_text("Erklärung im Tutorial").clicked() {
                self.topic = Topic::of(&part.kind);
                self.show_tutorial = true;
            }
        });
        ui.add_space(2.0);
        if part.broken {
            egui::Frame::new().fill(RED.gamma_multiply(0.15)).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                ui.label(RichText::new("💥 Durchgebrannt").strong().color(RED));
                ui.label("Das Bauteil leitet nicht mehr. Beseitige erst die Ursache, sonst brennt das neue gleich wieder durch.");
                if ui.button("🔧 Ersetzen").clicked() {
                    self.hist.checkpoint(&self.doc);
                    self.doc.part_mut(id).unwrap().broken = false;
                    self.thermal.reset();
                }
            });
            ui.add_space(4.0);
        } else if !part.kind.limits().is_empty() && self.limits_on {
            let h = self.thermal.heat(id);
            let col = if h > 0.8 {
                RED
            } else if h > 0.5 {
                Color32::from_rgb(235, 200, 70)
            } else {
                muted()
            };
            ui.horizontal(|ui| {
                ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Belastung").color(muted())));
                ui.add(egui::ProgressBar::new(h.clamp(0.0, 1.0) as f32).desired_width(120.0).fill(col).text(format!("{:.0} %", h * 100.0)))
                    .on_hover_text("Anteil am Grenzwert für Strom bzw. Leistung. Über 100 % brennt das Bauteil durch.");
            });
        }
        // Name
        if !matches!(part.kind, PartKind::Ground) {
            let k = format!("name:{id}:{}", part.name);
            let buf = self.bufs.entry(k).or_insert_with(|| part.name.clone());
            let mut new_name = None;
            ui.horizontal(|ui| {
                ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Name").color(muted())));
                let r = ui.add(egui::TextEdit::singleline(buf).desired_width(120.0));
                let trimmed = buf.trim().to_string();
                let taken = self.doc.parts.iter().any(|p| p.id != id && p.name == trimmed);
                if taken {
                    ui.label(RichText::new("⚠").color(RED)).on_hover_text("Name bereits vergeben");
                } else if r.lost_focus() && !trimmed.is_empty() && trimmed != part.name {
                    new_name = Some(trimmed);
                }
            });
            if let Some(n) = new_name {
                self.hist.checkpoint(&self.doc);
                self.doc.part_mut(id).unwrap().name = n;
            }
        }
        // Parameter
        let mut new_kind: Option<PartKind> = None;
        match part.kind.clone() {
            PartKind::Resistor { r } => {
                if let Some(v) = self.si_field(ui, "Widerstand", "Ω", r, true, "r") {
                    new_kind = Some(PartKind::Resistor { r: v });
                }
            }
            PartKind::Capacitor { c } => {
                if let Some(v) = self.si_field(ui, "Kapazität", "F", c, true, "c") {
                    new_kind = Some(PartKind::Capacitor { c: v });
                }
            }
            PartKind::Inductor { l } => {
                if let Some(v) = self.si_field(ui, "Induktivität", "H", l, true, "l") {
                    new_kind = Some(PartKind::Inductor { l: v });
                }
            }
            PartKind::Lamp { v_nom, p_nom } => {
                if let Some(v) = self.si_field(ui, "Nennspannung", "V", v_nom, true, "vn") {
                    new_kind = Some(PartKind::Lamp { v_nom: v, p_nom });
                }
                if let Some(v) = self.si_field(ui, "Nennleistung", "W", p_nom, true, "pn") {
                    new_kind = Some(PartKind::Lamp { v_nom, p_nom: v });
                }
                ui.label(RichText::new(format!("Ersatzwiderstand: {}", fmt_si(v_nom * v_nom / p_nom, "Ω"))).color(muted()));
            }
            PartKind::DcVoltage { v } => {
                if let Some(x) = self.si_field(ui, "Spannung", "V", v, false, "v") {
                    new_kind = Some(PartKind::DcVoltage { v: x });
                }
            }
            PartKind::AcVoltage { amp, freq, offset } => {
                if let Some(x) = self.si_field(ui, "Amplitude", "V", amp, false, "amp") {
                    new_kind = Some(PartKind::AcVoltage { amp: x.abs(), freq, offset });
                }
                if let Some(x) = self.si_field(ui, "Frequenz", "Hz", freq, true, "f") {
                    new_kind = Some(PartKind::AcVoltage { amp, freq: x, offset });
                }
                if let Some(x) = self.si_field(ui, "Offset", "V", offset, false, "off") {
                    new_kind = Some(PartKind::AcVoltage { amp, freq, offset: x });
                }
                ui.label(RichText::new(format!("Effektivwert: {}", fmt_si(amp / 2f64.sqrt(), "V"))).color(muted()));
            }
            PartKind::SquareVoltage { low, high, freq } => {
                if let Some(x) = self.si_field(ui, "Unterer Wert", "V", low, false, "lo") {
                    new_kind = Some(PartKind::SquareVoltage { low: x, high, freq });
                }
                if let Some(x) = self.si_field(ui, "Oberer Wert", "V", high, false, "hi") {
                    new_kind = Some(PartKind::SquareVoltage { low, high: x, freq });
                }
                if let Some(x) = self.si_field(ui, "Frequenz", "Hz", freq, true, "f") {
                    new_kind = Some(PartKind::SquareVoltage { low, high, freq: x });
                }
            }
            PartKind::CurrentSource { i } => {
                if let Some(x) = self.si_field(ui, "Strom", "A", i, false, "i") {
                    new_kind = Some(PartKind::CurrentSource { i: x });
                }
            }
            PartKind::Diode { kind } => {
                let mut k = kind;
                ui.horizontal(|ui| {
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Typ").color(muted())));
                    egui::ComboBox::from_id_salt(("dt", id)).selected_text(k.label()).show_ui(ui, |ui| {
                        for t in DiodeType::ALL {
                            ui.selectable_value(&mut k, t, t.label());
                        }
                    });
                });
                if k != kind {
                    new_kind = Some(PartKind::Diode { kind: k });
                }
            }
            PartKind::Led { color } => {
                let mut c = color;
                ui.horizontal(|ui| {
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Farbe").color(muted())));
                    egui::ComboBox::from_id_salt(("lc", id)).selected_text(c.label()).show_ui(ui, |ui| {
                        for t in LedColor::ALL {
                            ui.selectable_value(&mut c, t, format!("{} ({})", t.label(), fmt_si(t.vf(), "V")));
                        }
                    });
                });
                if c != color {
                    new_kind = Some(PartKind::Led { color: c });
                }
            }
            PartKind::Switch { closed } => {
                let mut c = closed;
                ui.horizontal(|ui| {
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Zustand").color(muted())));
                    ui.checkbox(&mut c, "geschlossen");
                });
                if c != closed {
                    new_kind = Some(PartKind::Switch { closed: c });
                }
            }
            PartKind::Gate { gate } => {
                let mut g = gate;
                ui.horizontal(|ui| {
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Funktion").color(muted())));
                    egui::ComboBox::from_id_salt(("gt", id)).selected_text(g.label()).show_ui(ui, |ui| {
                        for t in elab_app::model::GateType::ALL.into_iter().filter(|t| t.inputs() == gate.inputs()) {
                            ui.selectable_value(&mut g, t, t.label());
                        }
                    });
                });
                if g != gate {
                    new_kind = Some(PartKind::Gate { gate: g });
                }
            }
            PartKind::LogicInput { on } => {
                let mut c = on;
                ui.horizontal(|ui| {
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Pegel").color(muted())));
                    ui.checkbox(&mut c, "1 (ein)");
                });
                if c != on {
                    // Umschalten ist Bedienung, keine Bearbeitung: nicht in die Undo-Historie
                    self.doc.part_mut(id).unwrap().kind = PartKind::LogicInput { on: c };
                }
            }
            PartKind::Clock { freq } => {
                if let Some(x) = self.si_field(ui, "Frequenz", "Hz", freq, true, "cf") {
                    new_kind = Some(PartKind::Clock { freq: x });
                }
                ui.label(RichText::new(format!("Periode: {}", fmt_si(1.0 / freq, "s"))).color(muted()));
            }
            PartKind::LogicLed { color } => {
                let mut c = color;
                ui.horizontal(|ui| {
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Farbe").color(muted())));
                    egui::ComboBox::from_id_salt(("llc", id)).selected_text(c.label()).show_ui(ui, |ui| {
                        for t in LedColor::ALL {
                            ui.selectable_value(&mut c, t, t.label());
                        }
                    });
                });
                if c != color {
                    new_kind = Some(PartKind::LogicLed { color: c });
                }
            }
            PartKind::Transistor { kind } => {
                let mut k = kind;
                ui.horizontal(|ui| {
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Typ").color(muted())));
                    egui::ComboBox::from_id_salt(("tt", id)).selected_text(k.label()).show_ui(ui, |ui| {
                        for t in elab_app::model::TransistorType::ALL.into_iter().filter(|t| t.family() == kind.family()) {
                            ui.selectable_value(&mut k, t, t.label());
                        }
                    });
                });
                if k != kind {
                    new_kind = Some(PartKind::Transistor { kind: k });
                }
                let info = match (k.bjt(), k.mos()) {
                    (Some(m), _) => format!("β = {:.0}, I_S = {}", m.bf, fmt_si(m.is, "A")),
                    (_, Some(m)) => format!("U_th = {}, K = {}", fmt_si(if m.nmos { m.vth } else { -m.vth }, "V"), fmt_si(m.k, "A/V²")),
                    _ => String::new(),
                };
                ui.label(RichText::new(format!("Modell: {info}")).color(muted()));
            }
            PartKind::Potentiometer { r, pos } => {
                if let Some(v) = self.si_field(ui, "Widerstand", "Ω", r, true, "pr") {
                    new_kind = Some(PartKind::Potentiometer { r: v, pos });
                }
                let mut p = pos * 100.0;
                let resp = ui
                    .horizontal(|ui| {
                        ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Stellung").color(muted())));
                        ui.add(egui::Slider::new(&mut p, 0.0..=100.0).suffix(" %").fixed_decimals(0))
                    })
                    .inner;
                if resp.drag_started() {
                    self.hist.checkpoint(&self.doc);
                }
                if (p / 100.0 - pos).abs() > 1e-9 {
                    if !resp.dragged() {
                        self.hist.checkpoint(&self.doc);
                    }
                    // Live verstellen, ohne für jeden Zwischenschritt einen Undo-Schritt anzulegen
                    self.doc.part_mut(id).unwrap().kind = PartKind::Potentiometer { r, pos: p / 100.0 };
                }
                ui.label(RichText::new("Anteil zwischen Schleifer S und B. Pfeiltasten verstellen ebenfalls.").small().color(muted()));
            }
            PartKind::GateIc { gate, family } => {
                let mut g = gate;
                ui.horizontal(|ui| {
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Funktion").color(muted())));
                    egui::ComboBox::from_id_salt(("gti", id)).selected_text(g.label()).show_ui(ui, |ui| {
                        for t in elab_app::model::GateType::ALL
                            .into_iter()
                            .filter(|t| t.inputs() == gate.inputs() && *t != elab_app::model::GateType::Xnor)
                        {
                            ui.selectable_value(&mut g, t, t.label());
                        }
                    });
                });
                if g != gate {
                    new_kind = Some(PartKind::GateIc { gate: g, family });
                }
                if let Some(k) = self.family_combo(ui, id, &part.kind) {
                    new_kind = Some(k);
                }
            }
            PartKind::SchmittIc { .. } | PartKind::DffIc { .. } | PartKind::CounterIc { .. } => {
                if let Some(k) = self.family_combo(ui, id, &part.kind) {
                    new_kind = Some(k);
                }
            }
            PartKind::OpAmp { kind } => {
                let mut k = kind;
                ui.horizontal(|ui| {
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Typ").color(muted())));
                    egui::ComboBox::from_id_salt(("oa", id)).selected_text(k.label()).show_ui(ui, |ui| {
                        for t in elab_app::model::OpAmpType::ALL {
                            ui.selectable_value(&mut k, t, t.label());
                        }
                    });
                });
                if k != kind {
                    new_kind = Some(PartKind::OpAmp { kind: k });
                }
                ui.label(RichText::new(k.describe()).small().color(muted()));
            }
            PartKind::Timer555 => {
                ui.label(RichText::new("Bipolarer Timer, 4,5 … 16 V. Ausgang 1 ≈ VCC − 1,7 V.").small().color(muted()));
            }
            PartKind::Adc4 => {
                ui.label(RichText::new("Code = ⌊16 · U_IN / U_REF⌋, Ausgänge wie 74HC.").small().color(muted()));
            }
            PartKind::Dac4 => {
                ui.label(RichText::new("U_OUT = U_REF · Code / 16, Innenwiderstand 10 kΩ (R-2R).").small().color(muted()));
            }
            PartKind::Port => {
                ui.label(
                    RichText::new("Der Name wird zum Pin-Namen, wenn du die Schaltung als IC speicherst. VCC und GND landen oben und unten.")
                        .small()
                        .color(muted()),
                );
            }
            PartKind::Custom { name, def } => {
                let ports: Vec<String> = elab_app::model::custom_ports(&def).into_iter().map(|p| p.1).collect();
                let n = def.parts.iter().filter(|p| !matches!(p.kind, PartKind::Port)).count();
                ui.label(RichText::new(format!("„{name}“: {n} Bauteile innen, Anschlüsse {}", ports.join(", "))).color(muted()));
                if ui
                    .button("🔍 Innenschaltung öffnen")
                    .on_hover_text("Ersetzt den Schaltplan durch den Inhalt des ICs. ⟲ holt den alten zurück.")
                    .clicked()
                {
                    let mut d = (*def).clone();
                    d.title = name.clone();
                    self.replace_doc(d);
                    self.flash(format!("Innenschaltung von „{name}“ geöffnet. Rückgängig (⌘Z) führt zurück."), BLUE);
                }
            }
            PartKind::Ground
            | PartKind::Probe
            | PartKind::PushButton
            | PartKind::Voltmeter
            | PartKind::Ammeter
            | PartKind::Button
            | PartKind::LogicHigh
            | PartKind::DFlipFlop
            | PartKind::JkFlipFlop
            | PartKind::Counter4
            | PartKind::SevenSeg => {}
        }
        // Pegel an den Anschlüssen von Digitalbausteinen
        if part.kind.is_digital() {
            if let (Some(ds), Some(nl)) = (&self.dsim, &self.nl) {
                ui.add_space(4.0);
                egui::Frame::group(ui.style()).inner_margin(8.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Pegel an den Anschlüssen").strong().color(BLUE));
                    let names = part.kind.pin_names();
                    ui.horizontal_wrapped(|ui| {
                        for (k, &n) in nl.pin_nets.get(&id).into_iter().flatten().enumerate() {
                            let v = ds.net(n);
                            ui.label(
                                RichText::new(format!("{} = {}", names.get(k).map_or("?", |s| s.as_str()), v.symbol()))
                                    .size(15.0)
                                    .strong()
                                    .color(symbols::logic_color(v)),
                            );
                            ui.add_space(6.0);
                        }
                    });
                    if let PartKind::Button = part.kind {
                        ui.label(RichText::new("Im Schaltplan gedrückt halten.").small().color(muted()));
                    }
                });
            }
        }
        // Echte Bausteine: Versorgung und Pegel, die der Baustein an seinen Anschlüssen sieht
        if part.kind.is_ic() {
            self.ic_panel(ui, &part);
        }
        if let Some(k) = new_kind {
            self.hist.checkpoint(&self.doc);
            self.doc.part_mut(id).unwrap().kind = k;
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("⟳ Drehen (R)").clicked() {
                self.hist.checkpoint(&self.doc);
                let ids = [id].into();
                self.doc.transform_parts(&ids, &Default::default(), Default::default(), |p| p.rot = (p.rot + 1) % 4);
            }
            if ui.button("🗑 Löschen").clicked() {
                self.hist.checkpoint(&self.doc);
                self.doc.remove(&[id].into(), &Default::default());
                self.ed.clear_selection();
            }
        });
        // Messwerte
        ui.add_space(6.0);
        let vals = self.op.as_ref().ok().and_then(|o| o.parts.get(&id)).copied();
        let probe_v = self.probe_voltage(id);
        if vals.is_some() || probe_v.is_some() {
            egui::Frame::group(ui.style()).inner_margin(8.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let title = if self.msim.is_some() { "Messwerte (jetzt, Echtzeit)" } else { "Messwerte (Arbeitspunkt)" };
                ui.label(RichText::new(title).strong().color(BLUE));
                if let Some(v) = probe_v {
                    big_value(ui, "U", &fmt_si(v, "V"), ACCENT);
                }
                if let (Some(v), PartKind::Transistor { kind }) = (vals, &part.kind) {
                    let bjt = kind.is_bjt();
                    let neg = matches!(kind.family(), elab_app::model::TransistorFamily::Pnp | elab_app::model::TransistorFamily::Pmos);
                    let (uc, uo, ic, io) = match (bjt, neg) {
                        (true, false) => ("U_BE", "U_CE", "I_B", "I_C"),
                        (true, true) => ("U_EB", "U_EC", "I_B", "I_C"),
                        (false, false) => ("U_GS", "U_DS", "I_G", "I_D"),
                        (false, true) => ("U_SG", "U_SD", "I_G", "I_D"),
                    };
                    big_value(ui, uc, &fmt_si(v.ctrl_u, "V"), ACCENT);
                    big_value(ui, uo, &fmt_si(v.u, "V"), ACCENT);
                    big_value(ui, ic, &fmt_si(v.ctrl_i, "A"), BLUE);
                    big_value(ui, io, &fmt_si(v.i, "A"), BLUE);
                    if bjt && v.ctrl_i.abs() > 1e-12 && v.i.abs() > 1e-12 {
                        big_value(ui, "I_C / I_B", &format!("{:.0}", v.i / v.ctrl_i), muted());
                    }
                    big_value(ui, "P", &fmt_si(v.p, "W"), GREEN);
                    if let Some(reg) = v.region {
                        ui.label(RichText::new(reg.text(bjt)).strong());
                    }
                } else if let (Some(v), PartKind::OpAmp { .. }) = (vals, &part.kind) {
                    big_value(ui, "U_aus", &fmt_si(v.u, "V"), ACCENT);
                    big_value(ui, "U₊ − U₋", &fmt_si(v.ctrl_u, "V"), ACCENT);
                    big_value(ui, "I_aus", &fmt_si(v.i, "A"), BLUE);
                    big_value(ui, "I (V+)", &fmt_si(v.ctrl_i, "A"), BLUE);
                    big_value(ui, "P", &fmt_si(v.p, "W"), GREEN);
                } else if let (Some(v), true) = (vals, part.kind.is_ic()) {
                    big_value(ui, "U_VCC", &fmt_si(v.u, "V"), ACCENT);
                    big_value(ui, "I_VCC", &fmt_si(v.i, "A"), BLUE);
                    big_value(ui, "P (Verlust)", &fmt_si(v.p, "W"), GREEN);
                } else if let (Some(v), PartKind::Voltmeter) = (vals, &part.kind) {
                    big_value(ui, "Anzeige", &fmt_si(v.u, "V"), ACCENT);
                } else if let (Some(v), PartKind::Ammeter) = (vals, &part.kind) {
                    big_value(ui, "Anzeige", &fmt_si(v.i, "A"), BLUE);
                    if v.i.abs() > 10.0 {
                        ui.label(RichText::new("Überlast: Das Amperemeter liegt wohl parallel zu einer Spannungsquelle (Kurzschluss).").color(RED));
                    }
                } else if let (Some(v), PartKind::Potentiometer { .. }) = (vals, &part.kind) {
                    big_value(ui, "U_AB", &fmt_si(v.u, "V"), ACCENT);
                    big_value(ui, "U_SB", &fmt_si(v.ctrl_u, "V"), ACCENT);
                    big_value(ui, "I_A", &fmt_si(v.i, "A"), BLUE);
                    big_value(ui, "I_Schleifer", &fmt_si(v.ctrl_i, "A"), BLUE);
                } else if let Some(v) = vals {
                    big_value(ui, "U", &fmt_si(v.u, "V"), ACCENT);
                    big_value(ui, "I", &fmt_si(v.i, "A"), BLUE);
                    let pl = if part.kind.shape() == Shape::Source { "P (abgegeben)" } else { "P" };
                    big_value(ui, pl, &fmt_si(v.p, "W"), GREEN);
                    if let PartKind::Resistor { r } = part.kind {
                        if v.i.abs() > 1e-15 {
                            ui.label(RichText::new(format!("U / I = {}  ✓ = R", fmt_si(v.u / v.i, "Ω"))).color(muted()));
                        }
                        if v.p > 0.25 {
                            ui.label(RichText::new(format!("Achtung: {} übersteigt einen ¼-W-Widerstand", fmt_si(v.p, "W"))).color(RED));
                        }
                        let _ = r;
                    }
                    if let PartKind::Led { .. } = part.kind {
                        if v.i > 0.03 {
                            ui.label(RichText::new("Achtung: Strom über 30 mA, eine echte LED wäre überlastet").color(RED));
                        }
                    }
                    let pn = part.kind.pin_names();
                    if pn.len() == 2 && !matches!(part.kind, PartKind::Voltmeter | PartKind::Ammeter) {
                        ui.label(
                            RichText::new(format!("U gemessen von {} nach {}; I positiv in dieser Richtung.", pn[0], pn[1])).small().color(muted()),
                        );
                    }
                }
            });
        } else if self.nl.as_ref().is_some_and(|nl| nl.excluded.contains(&id)) {
            ui.label(RichText::new("Nicht mit der Masse verbunden: wird nicht simuliert.").color(ACCENT));
        }
    }

    /// Auswahl der Logikfamilie (HC/HCT).
    fn family_combo(&mut self, ui: &mut egui::Ui, id: u32, kind: &PartKind) -> Option<PartKind> {
        let fam = kind.family()?;
        let mut f = fam;
        ui.horizontal(|ui| {
            ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Familie").color(muted())));
            egui::ComboBox::from_id_salt(("fam", id)).selected_text(f.label()).show_ui(ui, |ui| {
                for t in elab_app::model::LogicFamily::ALL {
                    ui.selectable_value(&mut f, t, t.label());
                }
            });
        });
        ui.label(RichText::new(f.describe()).small().color(muted()));
        (f != fam).then(|| {
            let mut k = kind.clone();
            k.set_family(f);
            k
        })
    }

    /// Versorgungszustand und Pegel an den Anschlüssen eines ICs.
    fn ic_panel(&mut self, ui: &mut egui::Ui, part: &elab_app::model::Part) {
        use elab_sim::PowerState;
        let pins = self.pin_logic.get(&part.id).cloned();
        let power = self.ic_power(part.id);
        if pins.is_none() && power.is_none() {
            return;
        }
        ui.add_space(4.0);
        egui::Frame::group(ui.style()).inner_margin(8.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Was der Baustein sieht").strong().color(BLUE));
            match power {
                Some(PowerState::Ok) => {
                    ui.label(RichText::new("✔ Versorgung in Ordnung").color(GREEN));
                }
                Some(PowerState::Low) => {
                    ui.label(RichText::new("⚠ Versorgung fehlt oder zu klein: Ausgänge abgeschaltet").color(ACCENT));
                }
                Some(PowerState::Overvoltage) => {
                    ui.label(RichText::new("✖ Überspannung: Der Baustein ist zerstört. Nach dem Korrigieren neu einschalten (↺).").color(RED));
                }
                Some(PowerState::Reversed) => {
                    ui.label(RichText::new("✖ VCC und GND vertauscht: Der Baustein ist zerstört.").color(RED));
                }
                None => {}
            }
            if let Some(pins) = pins {
                let names = part.kind.pin_names();
                ui.horizontal_wrapped(|ui| {
                    for (k, v) in pins.iter().enumerate() {
                        let Some(v) = v else { continue };
                        ui.label(
                            RichText::new(format!("{} = {}", names.get(k).map_or("?", |s| s.as_str()), v.symbol()))
                                .size(15.0)
                                .strong()
                                .color(symbols::logic_color(*v)),
                        );
                        ui.add_space(6.0);
                    }
                });
                if pins.iter().flatten().any(|v| *v == elab_sim::digital::Logic::X) {
                    ui.label(
                        RichText::new(
                            "X: Spannung zwischen den Schaltschwellen oder Eingang offen. Offene CMOS-Eingänge brauchen einen Pull-up/-down.",
                        )
                        .small()
                        .color(muted()),
                    );
                }
            }
        });
    }

    fn probe_voltage(&self, id: u32) -> Option<f64> {
        let part = self.doc.part(id)?;
        if !matches!(part.kind, PartKind::Probe) {
            return None;
        }
        let nl = self.nl.as_ref()?;
        let op = self.op.as_ref().ok()?;
        op.net_v.get(nl.pin_nets.get(&id)?[0]).copied().flatten()
    }

    // ------------------------------------------------------------------
    // Tutorial
    // ------------------------------------------------------------------

    fn tutorial_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("🎓 Tutorial").size(17.0).strong().color(ACCENT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("✕").on_hover_text("Tutorial ausblenden").clicked() {
                    self.show_tutorial = false;
                }
            });
        });
        ui.label(RichText::new("Klicke auf ein Bauteil in der Palette oder im Schaltplan.").small().color(muted()));
        ui.separator();
        self.lesson_card(ui);
        if self.topic == Topic::Tasks && self.lesson.is_none() {
            egui::Frame::new().fill(Color32::from_rgb(28, 36, 30)).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("🎯 Aufgaben").strong().color(GREEN));
                for t in elab_app::lessons::tasks() {
                    if ui.button(&t.title).on_hover_text(&t.intro).clicked() {
                        self.start_task(t);
                    }
                }
            });
            ui.add_space(6.0);
        }
        let d = tutorial::doc(self.topic);
        ui.label(RichText::new(d.title).size(19.0).strong());
        ui.add_space(4.0);
        if let Some(c) = self.tutorial_current() {
            egui::Frame::new().fill(Color32::from_rgb(40, 34, 24)).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(c).color(ACCENT));
            });
            ui.add_space(4.0);
        }
        ui.label(RichText::new(d.what).size(14.0).color(text()));
        if !d.math.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Formeln").strong().color(BLUE));
            egui::Frame::new().fill(Color32::from_gray(22)).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(d.math).monospace().size(13.0));
            });
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Übliche Werte").strong().color(GREEN));
        ui.label(RichText::new(d.typical).size(14.0).color(text()));
        ui.add_space(6.0);
        ui.label(RichText::new("Beobachten & ausprobieren").strong().color(VIOLET));
        ui.label(RichText::new(d.watch).size(14.0).color(text()));
        if !d.related.is_empty() {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Siehe auch:").color(muted()));
                for &r in d.related {
                    if ui.link(tutorial::doc(r).title).clicked() {
                        self.topic = r;
                    }
                }
            });
        }
        ui.add_space(10.0);
        ui.separator();
        ui.label(RichText::new("Grundlagen").strong().color(muted()));
        ui.horizontal_wrapped(|ui| {
            for t in Topic::BASICS {
                if ui.selectable_label(self.topic == t, tutorial::doc(t).title).clicked() {
                    self.topic = t;
                }
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Transistoren & Messen").strong().color(muted()));
        ui.horizontal_wrapped(|ui| {
            for t in Topic::TRANSISTORS {
                if ui.selectable_label(self.topic == t, tutorial::doc(t).title).clicked() {
                    self.topic = t;
                }
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Echte Bausteine (ICs)").strong().color(muted()));
        ui.horizontal_wrapped(|ui| {
            for t in Topic::MIXED {
                if ui.selectable_label(self.topic == t, tutorial::doc(t).title).clicked() {
                    self.topic = t;
                }
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Digitaltechnik").strong().color(muted()));
        ui.horizontal_wrapped(|ui| {
            for t in Topic::DIGITAL {
                if ui.selectable_label(self.topic == t, tutorial::doc(t).title).clicked() {
                    self.topic = t;
                }
            }
        });
    }

    /// Live-Werte des ausgewählten Bauteils passend zum Thema.
    fn tutorial_current(&self) -> Option<String> {
        let id = self.ed.single_selected()?;
        let part = self.doc.part(id)?;
        if Topic::of(&part.kind) != self.topic {
            return None;
        }
        if let Some(v) = self.probe_voltage(id) {
            return Some(format!("{}: {} gegen Masse", part.name, fmt_si(v, "V")));
        }
        let v = self.op.as_ref().ok()?.parts.get(&id)?;
        let base =
            format!("{} = {}:  U = {},  I = {},  P = {}", part.name, part.kind.value_text(), fmt_si(v.u, "V"), fmt_si(v.i, "A"), fmt_si(v.p, "W"));
        let extra = match &part.kind {
            PartKind::Resistor { r } if v.i.abs() > 1e-15 => {
                format!("\nOhm: U / I = {} = R ✓  ({} · {} = {})", fmt_si(v.u / v.i, "Ω"), fmt_si(*r, "Ω"), fmt_si(v.i, "A"), fmt_si(r * v.i, "V"))
            }
            PartKind::Led { color } => {
                format!("\nFlussspannung bei 20 mA: {}", fmt_si(color.vf(), "V"))
            }
            PartKind::Capacitor { .. } => "\nIm Arbeitspunkt ist der Kondensator geladen: I = 0.".to_string(),
            PartKind::Inductor { .. } => "\nIm Arbeitspunkt wirkt die Spule wie ein Draht: U ≈ 0.".to_string(),
            _ => String::new(),
        };
        Some(base + &extra)
    }

    // ------------------------------------------------------------------
    // Oszilloskop
    // ------------------------------------------------------------------

    fn scope_ui(&mut self, ui: &mut egui::Ui) {
        let live = self.msim.is_some();
        if live && !self.scope_hold {
            if let (Some(nl), Some(ms)) = (&self.nl, &self.msim) {
                self.scope = Some(Ok(sim::live_scope(&self.doc, nl, ms, self.scope_t, self.scope_trigger)));
            }
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Zeitbasis").color(muted()));
            let cur = TIMEBASES.iter().find(|t| t.0 == self.scope_t).map_or_else(|| fmt_si(self.scope_t, "s"), |t| t.1.to_string());
            egui::ComboBox::from_id_salt("tb")
                .selected_text(cur)
                .show_ui(ui, |ui| {
                    for (t, name) in TIMEBASES {
                        ui.selectable_value(&mut self.scope_t, t, name);
                    }
                })
                .response
                .on_hover_text("Breite des Bildschirms: so viel Zeit ist von links nach rechts zu sehen");
            if live {
                ui.checkbox(&mut self.scope_trigger, "Trigger").on_hover_text(
                    "Startet das Bild an einer steigenden Flanke des ersten Messpunkts. Periodische Signale stehen dann still. \
                     Ohne Trigger läuft das Bild durch.",
                );
                let label = if self.scope_hold { "▶ Weiter" } else { "⏸ Bild anhalten" };
                if ui.button(label).on_hover_text("Das aktuelle Bild festhalten, die Simulation läuft weiter").clicked() {
                    self.scope_hold = !self.scope_hold;
                }
                if let Some(Ok(s)) = &self.scope {
                    let (txt, col) = if s.triggered {
                        ("● getriggert", GREEN)
                    } else if self.scope_trigger {
                        ("○ keine Flanke: Bild läuft durch", muted())
                    } else {
                        ("○ läuft durch", muted())
                    };
                    ui.label(RichText::new(txt).color(col));
                }
            } else {
                ui.label(RichText::new("Ohne Kondensator, Spule oder Wechselquelle sind alle Spannungen konstant.").color(muted()));
            }
        });
        let (scale, unit) = if self.scope_t < 1e-3 {
            (1e6, "µs")
        } else if self.scope_t < 1.0 {
            (1e3, "ms")
        } else {
            (1.0, "s")
        };
        match &self.scope {
            None => {
                ui.label(RichText::new("Noch keine Messung.").color(muted()));
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("⚠ {e}")).color(RED));
            }
            Some(Ok(s)) if s.traces.is_empty() => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Setze Messpunkte (📍) in die Schaltung, um Spannungen über die Zeit zu sehen.").size(14.0).color(muted()),
                    );
                });
            }
            Some(Ok(s)) => {
                let t_end = s.t_stop * scale;
                egui_plot::Plot::new(format!("scope{}", s.t_stop))
                    .legend(egui_plot::Legend::default())
                    .x_axis_label(format!("t / {unit}"))
                    .y_axis_label("U / V")
                    .allow_scroll(false)
                    .allow_drag(false)
                    .include_x(0.0)
                    .include_x(t_end)
                    .include_y(0.0)
                    .auto_bounds(egui::Vec2b::new(false, true))
                    .default_x_bounds(0.0, t_end)
                    .show(ui, |p| {
                        for tr in &s.traces {
                            let pts: Vec<[f64; 2]> = tr.pts.iter().map(|q| [q[0] * scale, q[1]]).collect();
                            p.line(
                                egui_plot::Line::new(tr.name.clone(), egui_plot::PlotPoints::from(pts)).color(probe_color(tr.color_idx)).width(2.0),
                            );
                        }
                    });
            }
        }
    }

    // ------------------------------------------------------------------
    // Testskript
    // ------------------------------------------------------------------

    fn run_script(&mut self, ctx: &egui::Context) {
        let Some(run) = &mut self.script else { return };
        ctx.request_repaint();
        if run.busy() {
            run.wait = run.wait.saturating_sub(if run.frames.is_empty() { 1 } else { 0 });
            return;
        }
        let Some(cmd) = run.cmds.pop_front() else {
            return;
        };
        let to_screen = |ed: &Editor, x: f32, y: f32| ed.grid_to_screen(x, y);
        use script::Cmd::*;
        match cmd {
            Example(n) => {
                let all = examples::all();
                let idx = all.iter().position(|e| e.title.starts_with(n.as_str())).or_else(|| n.parse::<usize>().ok());
                match idx {
                    Some(i) if i < all.len() => self.load_example(i),
                    _ => panic!("Skript: Beispiel „{n}“ nicht gefunden"),
                }
            }
            Lesson(cmd) => match cmd.as_str() {
                "stop" => self.lesson = None,
                t if t.starts_with("task ") => {
                    let name = &t[5..];
                    let task = elab_app::lessons::tasks().into_iter().find(|l| l.title.starts_with(name)).expect("Skript: unbekannte Aufgabe");
                    self.start_task(task);
                }
                "skip" => {
                    if let Some(run) = &mut self.lesson {
                        if let Some(c) = run.current {
                            run.progress.skipped.insert(c);
                        }
                    }
                }
                _ => {
                    let kind =
                        self.ed.single_selected().and_then(|id| self.doc.part(id)).map(|p| p.kind.clone()).filter(elab_app::lessons::has_lesson);
                    match kind.or_else(|| lesson_ui::topic_kind(self.topic)) {
                        Some(k) => self.start_lesson(&k),
                        None => panic!("Skript: keine Übung für Auswahl oder Thema"),
                    }
                }
            },
            Tool(name) => {
                self.ed.place_rot = 0;
                self.ed.tool = match name.as_str() {
                    "select" => editor::Tool::Select,
                    "wire" => editor::Tool::Wire,
                    other => {
                        let kind = PartKind::palette().into_iter().flat_map(|c| c.1).find(|k| k.label() == other || k.prefix() == other);
                        match kind {
                            Some(k) => editor::Tool::Place(k),
                            None => panic!("Skript: unbekanntes Bauteil {other}"),
                        }
                    }
                }
            }
            Move(x, y) => {
                let p = to_screen(&self.ed, x, y);
                self.script.as_mut().unwrap().frames.push_back(vec![egui::Event::PointerMoved(p)]);
            }
            Click(x, y, m) => {
                let p = to_screen(&self.ed, x, y);
                self.script.as_mut().unwrap().click(p, m);
            }
            ClickPx(x, y) => self.script.as_mut().unwrap().click(egui::pos2(x, y), Modifiers::NONE),
            PalDrag(label, gx, gy) => {
                // Nur ziehen, wenn die Kachel ruhig liegt (nicht gerade aufgeklappt oder gerollt)
                let find = |v: &[(String, egui::Rect)]| v.iter().find(|t| t.0 == label).map(|t| t.1);
                let stable = find(&self.tile_rects).filter(|r| find(&self.tile_rects_prev) == Some(*r));
                let Some(rect) = stable else {
                    // Gruppe aufklappen und Kachel ins Bild rollen, dann nochmal versuchen
                    let tries = if self.pal_focus.as_deref() == Some(label.as_str()) { 1 } else { 0 };
                    if tries > 0 && self.script.as_ref().unwrap().retries > 20 {
                        panic!(
                            "Skript: Palettenkachel „{label}“ nicht gefunden (sichtbar: {:?})",
                            self.tile_rects.iter().map(|t| &t.0).collect::<Vec<_>>()
                        );
                    }
                    self.pal_filter.clear();
                    self.pal_focus = Some(label.clone());
                    let run = self.script.as_mut().unwrap();
                    run.retries += 1;
                    run.cmds.push_front(PalDrag(label, gx, gy));
                    run.wait = 2;
                    return;
                };
                self.pal_focus = None;
                self.script.as_mut().unwrap().retries = 0;
                let b = to_screen(&self.ed, gx, gy);
                self.script.as_mut().unwrap().drag(rect.center(), b);
            }
            DragPxGrid(px, py, gx, gy) => {
                let b = to_screen(&self.ed, gx, gy);
                self.script.as_mut().unwrap().drag(egui::pos2(px, py), b);
            }
            Type(t) => self.script.as_mut().unwrap().type_text(&t),
            DblClick(x, y) => {
                let p = to_screen(&self.ed, x, y);
                self.script.as_mut().unwrap().dblclick(p);
            }
            Drag(x1, y1, x2, y2) => {
                let (a, b) = (to_screen(&self.ed, x1, y1), to_screen(&self.ed, x2, y2));
                self.script.as_mut().unwrap().drag(a, b);
            }
            Key(k, m) => self.script.as_mut().unwrap().key(k, m),
            Select(name) => {
                if let Some(p) = self.doc.part_by_name(&name) {
                    let id = p.id;
                    self.ed.select_only(id);
                    self.topic = Topic::of(&self.doc.part(id).unwrap().kind);
                }
            }
            Value(name, v) => {
                if let Some(id) = self.doc.part_by_name(&name).map(|p| p.id) {
                    self.hist.checkpoint(&self.doc);
                    match &mut self.doc.part_mut(id).unwrap().kind {
                        PartKind::Resistor { r } => *r = v,
                        PartKind::Capacitor { c } => *c = v,
                        PartKind::Inductor { l } => *l = v,
                        PartKind::DcVoltage { v: u } => *u = v,
                        PartKind::Potentiometer { pos, .. } => *pos = v,
                        other => eprintln!("value: {other:?} hat keinen einfachen Wert"),
                    }
                }
            }
            PalList(on) => self.pal_list = on,
            Scope(on) => self.show_scope = on,
            Tutorial(on) => self.show_tutorial = on,
            Timebase(t) => self.scope_t = t,
            Fit => self.ed.fit_request = true,
            Wait(n) => self.script.as_mut().unwrap().wait = n,
            Shot(path) => {
                let done = Arc::new(AtomicBool::new(false));
                let d2 = done.clone();
                ctx.request_screenshot(move |img| {
                    let [w, h] = img.size;
                    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
                    for c in &img.pixels {
                        out.extend_from_slice(&[c.r(), c.g(), c.b()]);
                    }
                    if let Some(dir) = std::path::Path::new(&path).parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    std::fs::write(&path, out).expect("Screenshot schreiben");
                    eprintln!("Screenshot: {path} ({w}x{h})");
                    d2.store(true, Ordering::SeqCst);
                });
                let run = self.script.as_mut().unwrap();
                run.shot_pending = Some(done);
                run.wait = 2;
            }
            Dump(path) => {
                use serde_json::json;
                let status = match &self.op {
                    Ok(op) => format!("ok newton={}", op.newton),
                    Err(e) => format!("err {e}"),
                };
                let mut values = serde_json::Map::new();
                let mut logic = serde_json::Map::new();
                for p in &self.doc.parts {
                    if let Some(v) = self.op.as_ref().ok().and_then(|o| o.parts.get(&p.id)) {
                        values.insert(p.name.clone(), json!([v.u, v.i]));
                    }
                    if let (Some(ds), Some(nl)) = (&self.dsim, &self.nl) {
                        if p.kind.is_digital() || matches!(p.kind, PartKind::Probe) {
                            let v: String = nl.pin_nets.get(&p.id).into_iter().flatten().map(|&n| ds.net(n).symbol()).collect();
                            logic.insert(p.name.clone(), json!(v));
                        }
                    }
                    if let Some(pins) = self.pin_logic.get(&p.id) {
                        let v: String = pins.iter().map(|l| l.map_or("·", |l| l.symbol())).collect();
                        logic.insert(p.name.clone(), json!(v));
                    }
                }
                let bridged: Vec<String> =
                    self.nl.iter().flat_map(|nl| nl.bridged.iter()).filter_map(|id| self.doc.part(*id)).map(|p| p.name.clone()).collect();
                let selected: Vec<String> = self.ed.sel_parts.iter().filter_map(|id| self.doc.part(*id)).map(|p| p.name.clone()).collect();
                let out = json!({
                    "status": status,
                    "values": values,
                    "logic": logic,
                    "bridged": bridged,
                    "broken": self.doc.parts.iter().filter(|p| p.broken).map(|p| p.name.clone()).collect::<Vec<_>>(),
                    "undo": self.hist.can_undo(),
                    "tool": format!("{:?}", self.ed.tool),
                    "selected": selected,
                    "lesson": self.lesson.as_ref().map(|r| json!({
                        "title": r.lesson.title,
                        "step": r.current.map(|c| c + 1),
                        "done": r.done.iter().filter(|d| **d).count(),
                        "steps": r.lesson.steps.len(),
                    })),
                    "doc": serde_json::to_value(&self.doc).unwrap(),
                });
                let s = serde_json::to_string_pretty(&out).unwrap();
                if path == "-" {
                    println!("{s}");
                } else {
                    if let Some(dir) = std::path::Path::new(&path).parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    std::fs::write(&path, s).expect("Dump schreiben");
                }
            }
            Advance(secs) => {
                if let Some(ds) = &mut self.dsim {
                    let t = ds.time + (secs * 1e9) as u64;
                    ds.run_until(t);
                }
                if let Some(ms) = &mut self.msim {
                    let t = ms.time() + secs;
                    let _ = ms.advance(t, None);
                }
            }
            Hold(x, y) => {
                let id = self.doc.parts.iter().find(|p| p.bbox().0 <= x && p.bbox().2 >= x && p.bbox().1 <= y && p.bbox().3 >= y).map(|p| p.id);
                self.script_pressed = id;
            }
            Release => self.script_pressed = None,
            Panel(name) => {
                self.show_scope = true;
                self.bottom_tab = match name.as_str() {
                    "analyzer" => panels::BottomTab::Analyzer,
                    "truth" => panels::BottomTab::Truth,
                    _ => panels::BottomTab::Scope,
                };
            }
            Pause(on) => self.dig_paused = on,
            SaveIc(name) => {
                if let Err(e) = self.save_custom_ic(&name) {
                    panic!("Skript: IC speichern: {e}");
                }
            }
            Quit => std::process::exit(0),
        }
    }
}

/// Kachel in der Bauteil-Palette mit Schaltzeichen und Kurzname.
/// Name einer Palettenkachel (Suche, Testskripte): die Bauteilart, bei Varianten mit Wert.
fn tile_key(kind: &PartKind) -> String {
    match kind {
        PartKind::DcVoltage { v } if *v == 5.0 => format!("{} 5 V", kind.label()),
        _ => kind.label().to_string(),
    }
}

/// Farben der Palette: Kachelgrund, Rand, Schaltzeichen (hell oder dunkel)
fn tile_colors(selected: bool, hovered: bool) -> (Color32, Color32, Color32) {
    let dark = theme::is_dark();
    let bg = match (selected, hovered, dark) {
        (true, _, _) => ACCENT.gamma_multiply(0.18),
        (false, true, true) => Color32::from_gray(44),
        (false, false, true) => Color32::from_gray(30),
        (false, true, false) => Color32::from_gray(222),
        (false, false, false) => Color32::from_gray(238),
    };
    let stroke = if selected {
        ACCENT
    } else if dark {
        Color32::from_gray(52)
    } else {
        Color32::from_gray(200)
    };
    let icon = if selected {
        ACCENT
    } else if dark {
        Color32::from_rgb(208, 211, 219)
    } else {
        Color32::from_rgb(40, 44, 52)
    };
    (bg, stroke, icon)
}

/// Schaltzeichen eines Bauteils in einem kleinen Feld um `c` (Breite etwa 62 px bei `k` = 1).
fn draw_icon(painter: &egui::Painter, c: egui::Pos2, kind: &PartKind, col: Color32, k: f32) {
    let (rot, s, off) = match kind.shape() {
        Shape::Source => (3, 13.0, Vec2::ZERO),
        Shape::TwoPole => (0, 14.0, Vec2::ZERO),
        Shape::OnePin => (0, 13.0, Vec2::new(0.0, -7.0)),
        Shape::ThreePole => {
            let (x0, y0, x1, y1) = kind.local_bbox();
            (0, 12.0, -Vec2::new((x0 + x1) * 0.5, (y0 + y1) * 0.5) * 12.0)
        }
        Shape::Logic | Shape::Ic => {
            let (x0, y0, x1, y1) = kind.local_bbox();
            let s = (62.0 / (x1 - x0)).min(36.0 / (y1 - y0)).min(13.0);
            (0, s, -Vec2::new((x0 + x1) * 0.5, (y0 + y1) * 0.5) * s)
        }
    };
    let pen = symbols::Pen { painter, origin: c + off * k, s: s * k, rot, color: col, width: 1.5 };
    if let PartKind::Probe = kind {
        let pin = c + Vec2::new(-12.0, 8.0) * k;
        painter.line_segment([pin, pin + Vec2::new(8.0, -8.0) * k], Stroke::new(1.5, ACCENT));
        painter.circle_filled(pin, 3.0 * k, ACCENT);
        let r = egui::Rect::from_min_size(pin + Vec2::new(8.0, -20.0) * k, Vec2::new(24.0, 13.0) * k);
        painter.rect_stroke(r, 2.0, Stroke::new(1.3, ACCENT), StrokeKind::Inside);
        painter.text(r.center(), egui::Align2::CENTER_CENTER, "U", egui::FontId::proportional(10.0 * k), Color32::WHITE);
    } else {
        symbols::draw(&pen, kind, symbols::Live::default());
    }
}

/// Kurzname auf der Kachel
fn tile_short(kind: &PartKind) -> String {
    match kind {
        PartKind::DcVoltage { v } if *v == 5.0 => "5 V (Logik)".into(),
        PartKind::DcVoltage { .. } => "Batterie 9 V".into(),
        PartKind::AcVoltage { .. } => "Wechsel".into(),
        PartKind::SquareVoltage { .. } => "Rechteck".into(),
        PartKind::CurrentSource { .. } => "Strom".into(),
        PartKind::Lamp { .. } => "Lampe".into(),
        PartKind::Led { .. } => "LED".into(),
        PartKind::Transistor { kind } => match kind.family() {
            elab_app::model::TransistorFamily::Npn => "NPN",
            elab_app::model::TransistorFamily::Pnp => "PNP",
            elab_app::model::TransistorFamily::Nmos => "N-MOS",
            elab_app::model::TransistorFamily::Pmos => "P-MOS",
        }
        .into(),
        PartKind::Potentiometer { .. } => "Poti".into(),
        PartKind::Ammeter => "Ampere".into(),
        PartKind::Gate { gate } => gate.label().into(),
        PartKind::LogicInput { .. } => "Schalter".into(),
        PartKind::Clock { .. } => "Takt".into(),
        PartKind::LogicLed { .. } => "Anzeige".into(),
        PartKind::DFlipFlop => "D-FF".into(),
        PartKind::JkFlipFlop => "JK-FF".into(),
        PartKind::Counter4 => "Zähler".into(),
        PartKind::SevenSeg => "7-Segment".into(),
        PartKind::Adc4 => "ADC".into(),
        PartKind::Dac4 => "DAC".into(),
        other if other.is_ic() => other.ic_type().unwrap_or_default().to_string(),
        other => other.label().into(),
    }
}

/// Ein Satz zum Bauteil für die Listenansicht der Palette.
fn blurb(kind: &PartKind) -> &'static str {
    use elab_app::model::GateType as G;
    match kind {
        PartKind::DcVoltage { v } if *v == 5.0 => "Netzteil 5 V für Logik-ICs (74HC verträgt höchstens 7 V)",
        PartKind::DcVoltage { .. } => "Gleichspannung, z. B. Batterie. Spannung im Inspektor",
        PartKind::AcVoltage { .. } => "Sinus-Wechselspannung, z. B. 50 Hz aus dem Trafo",
        PartKind::SquareVoltage { .. } => "Schaltet zwischen zwei Spannungen hin und her",
        PartKind::CurrentSource { .. } => "Liefert einen festen Strom, egal wie groß die Last",
        PartKind::Ground => "Bezugspunkt 0 V. Jede Schaltung braucht eine Masse",
        PartKind::Resistor { .. } => "Begrenzt den Strom: U = R · I",
        PartKind::Potentiometer { .. } => "Einstellbarer Spannungsteiler mit Schleifer",
        PartKind::Capacitor { .. } => "Speichert Ladung, glättet und verzögert",
        PartKind::Inductor { .. } => "Spule: wehrt sich gegen Stromänderungen",
        PartKind::Lamp { .. } => "Glühlampe, leuchtet mit ihrer Leistung",
        PartKind::Diode { .. } => "Lässt Strom nur in eine Richtung durch",
        PartKind::Led { .. } => "Leuchtdiode, braucht immer einen Vorwiderstand",
        PartKind::Transistor { kind } => match kind.family() {
            elab_app::model::TransistorFamily::Npn => "NPN: kleiner Basisstrom schaltet großen Kollektorstrom",
            elab_app::model::TransistorFamily::Pnp => "PNP: schaltet zur Plusseite, Basis gegen Minus ziehen",
            elab_app::model::TransistorFamily::Nmos => "N-Kanal-MOSFET: Spannung am Gate schaltet, fast ohne Strom",
            elab_app::model::TransistorFamily::Pmos => "P-Kanal-MOSFET: schaltet zur Plusseite",
        },
        PartKind::Switch { .. } => "Ein/Aus per Doppelklick",
        PartKind::PushButton => "Leitet nur, solange er gedrückt ist",
        PartKind::Probe => "Zeigt die Spannung gegen Masse und im Oszilloskop",
        PartKind::Voltmeter => "Misst Spannung, wird parallel angeschlossen",
        PartKind::Ammeter => "Misst Strom, wird in die Leitung eingefügt",
        PartKind::Gate { gate } => match gate {
            G::And => "1, wenn alle Eingänge 1 sind",
            G::Or => "1, wenn mindestens ein Eingang 1 ist",
            G::Not => "Kehrt den Pegel um",
            G::Nand => "UND mit umgekehrtem Ausgang, universell",
            G::Nor => "ODER mit umgekehrtem Ausgang",
            G::Xor => "1, wenn genau ein Eingang 1 ist",
            G::Xnor => "1, wenn beide Eingänge gleich sind",
        },
        PartKind::LogicInput { .. } => "Logikschalter: Klick schaltet 0/1",
        PartKind::Button => "Logiktaster: 1, solange gedrückt",
        PartKind::Clock { .. } => "Taktgeber, schaltet regelmäßig um",
        PartKind::LogicHigh => "Fester Pegel 1",
        PartKind::LogicLed { .. } => "Leuchtet bei Pegel 1",
        PartKind::SevenSeg => "Zeigt eine Ziffer aus 4 Bit",
        PartKind::DFlipFlop => "Merkt sich D bei der steigenden Taktflanke",
        PartKind::JkFlipFlop => "Setzen, löschen oder umschalten im Takt",
        PartKind::Counter4 => "Zählt Taktflanken von 0 bis 15",
        PartKind::GateIc { .. } => "Echtes Gatter-IC mit VCC und GND, liest Spannungen",
        PartKind::SchmittIc { .. } => "Inverter mit Hysterese, macht verrauschte Signale sauber",
        PartKind::DffIc { .. } => "D-Flipflop als IC mit Versorgung",
        PartKind::CounterIc { .. } => "4-Bit-Zähler als IC mit Versorgung",
        PartKind::Timer555 => "Timer: Blinker, Taktgeber, Verzögerung",
        PartKind::OpAmp { .. } => "Operationsverstärker: verstärkt die Differenz der Eingänge",
        PartKind::Adc4 => "Wandelt eine Spannung in eine 4-Bit-Zahl",
        PartKind::Dac4 => "Wandelt eine 4-Bit-Zahl in eine Spannung",
        PartKind::Port => "Anschluss für ein eigenes IC",
        PartKind::Custom { .. } => "Eigenes IC aus der Bibliothek",
    }
}

fn tile(ui: &mut egui::Ui, kind: &PartKind, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(72.0, 62.0), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let (bg, stroke, icon) = tile_colors(selected, resp.hovered());
    painter.rect_filled(rect, 5.0, bg);
    painter.rect_stroke(rect, 5.0, Stroke::new(1.0, stroke), StrokeKind::Inside);
    draw_icon(&painter, rect.center_top() + Vec2::new(0.0, 22.0), kind, icon, 1.0);
    painter.text(
        rect.center_bottom() - Vec2::new(0.0, 5.0),
        egui::Align2::CENTER_BOTTOM,
        tile_short(kind),
        egui::FontId::proportional(11.0),
        muted(),
    );
    resp
}

/// Zeile der Listenansicht: Schaltzeichen, Name und ein Satz dazu.
fn list_row(ui: &mut egui::Ui, kind: &PartKind, selected: bool) -> egui::Response {
    let w = ui.available_width();
    let text_w = w - 66.0;
    let name = egui::WidgetText::from(RichText::new(tile_short(kind)).strong().size(13.0).color(text())).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        text_w,
        egui::TextStyle::Body,
    );
    let desc = egui::WidgetText::from(RichText::new(blurb(kind)).size(12.0).color(muted())).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        text_w,
        egui::TextStyle::Body,
    );
    let h = (name.size().y + desc.size().y + 8.0).max(42.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click_and_drag());
    let (bg, stroke, icon) = tile_colors(selected, resp.hovered());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, bg);
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0, stroke), StrokeKind::Inside);
    draw_icon(&painter, rect.left_center() + Vec2::new(30.0, 0.0), kind, icon, 0.75);
    let x = rect.left() + 62.0;
    painter.galley(egui::pos2(x, rect.top() + 4.0), name.clone(), text());
    painter.galley(egui::pos2(x, rect.top() + 6.0 + name.size().y), desc, muted());
    resp
}

fn big_value(ui: &mut egui::Ui, label: &str, value: &str, col: Color32) {
    ui.horizontal(|ui| {
        ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new(label).color(muted())));
        ui.label(RichText::new(value).size(16.0).strong().color(col));
    });
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(run) = &mut self.script {
            if let Some(evs) = run.frames.pop_front() {
                let mut m = Modifiers::NONE;
                for e in &evs {
                    if let egui::Event::PointerButton { modifiers, .. } | egui::Event::Key { modifiers, .. } = e {
                        m = *modifiers;
                    }
                }
                raw.events.push(egui::Event::ModifiersChanged(m));
                raw.events.extend(evs);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        log::frame_start();
        let sys_dark = ctx.global_style().visuals.dark_mode;
        theme::set_dark(sys_dark);
        self.global_keys(&ctx);
        log::activity("Netzliste und Arbeitspunkt");
        self.update_sim();
        log::activity("Digitalsimulation");
        self.update_digital(&ctx);
        log::activity("Echtzeitsimulation");
        self.update_mixed(&ctx);
        log::activity("Grenzwerte");
        self.check_limits(&ctx);
        log::activity("Übungsprüfung");
        self.lesson_tick();
        log::activity("Stromfluss");
        self.flow = match (&self.nl, &self.op, self.show_flow && self.sim_on) {
            (Some(nl), Ok(op), true) => elab_app::flow::wire_flow(&self.doc, nl, op),
            _ => Vec::new(),
        };
        log::activity("Oberfläche");
        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        self.save_ic_dialog(&ctx);
        egui::Panel::left("left").resizable(true).default_size(330.0).min_size(300.0).show(ui, |ui| {
            // Oben die Palette, unten der Inspektor: beide mit eigenem Rollbereich, damit der
            // Inspektor nicht unter aufgeklappten Gruppen verschwindet
            let h = ui.available_height();
            egui::ScrollArea::vertical().id_salt("pal").max_height(h * 0.56).auto_shrink([false, true]).show(ui, |ui| {
                self.palette(ui);
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt("insp").auto_shrink(false).show(ui, |ui| {
                self.inspector(ui);
            });
        });
        if self.show_tutorial {
            // Das Tutorial ist immer dunkel, mit hellem Lesetext
            let dark = theme::dark_visuals();
            let frame = egui::Frame::new().fill(dark.panel_fill).inner_margin(egui::Margin::symmetric(10, 8));
            egui::Panel::right("tutorial").resizable(true).default_size(380.0).min_size(300.0).frame(frame).show(ui, |ui| {
                theme::set_dark(true);
                *ui.visuals_mut() = dark;
                egui::ScrollArea::vertical().id_salt("tut").auto_shrink(false).show(ui, |ui| self.tutorial_ui(ui));
                theme::set_dark(sys_dark);
            });
        }
        if self.show_scope {
            log::activity("Oszilloskop und Messgeräte");
            egui::Panel::bottom("scope").resizable(true).default_size(270.0).min_size(150.0).show(ui, |ui| self.bottom_ui(ui));
        }
        log::activity("Schaltplan zeichnen und bearbeiten");
        egui::CentralPanel::no_frame().show(ui, |ui| {
            // Zeichenfläche: immer dunkel
            theme::set_dark(true);
            *ui.visuals_mut() = theme::dark_visuals();
            let mixed = self.msim.is_some();
            let highlight = self.lesson_highlight();
            let live = LiveData {
                heat: self.thermal.map(),
                highlight: &highlight,
                flow: &self.flow,
                nl: self.nl.as_ref(),
                op: self.op.as_ref().ok(),
                logic: if mixed { Some(&self.mixed_logic[..]) } else { self.dsim.as_ref().map(|d| d.nets()) },
                pin_logic: mixed.then_some(&self.pin_logic),
                pressed: &self.pressed,
            };
            let out = self.ed.ui(ui, &mut self.doc, &mut self.hist, &live);
            if let Some(k) = out.focus {
                self.topic = Topic::of(&k);
            }
            if let Some(id) = out.placed {
                self.lesson_placed(id);
            }
            if let Some(id) = out.toggle {
                if let Some(PartKind::LogicInput { on }) = self.doc.part_mut(id).map(|p| &mut p.kind) {
                    *on = !*on;
                }
            }
            let pressed: std::collections::HashSet<u32> = out.pressed.into_iter().chain(self.script_pressed).collect();
            if pressed != self.pressed {
                self.pressed = pressed;
                ui.ctx().request_repaint();
            }
        });
        theme::set_dark(sys_dark);
        // Fenstertitel
        let name = self.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| {
            if self.doc.title.is_empty() {
                "Neue Schaltung".into()
            } else {
                self.doc.title.clone()
            }
        });
        let dirty = if self.doc != self.saved { " •" } else { "" };
        let title = format!("elab – {name}{dirty}");
        if title != self.title_sent {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title_sent = title;
        }
        log::activity("Sichern");
        self.autosave(&ctx);
        log::activity("Testskript");
        self.run_script(&ctx);
        log::frame_end();
    }
}
