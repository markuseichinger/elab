//! elab – Elektronik-Lernlabor: Schaltungen bauen und live simulieren.

mod editor;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PalTab {
    Analog,
    Digital,
    Ic,
}

impl PalTab {
    fn groups(self) -> Vec<(&'static str, Vec<PartKind>)> {
        match self {
            PalTab::Analog => PartKind::palette_analog(),
            PalTab::Digital => PartKind::palette_digital(),
            PalTab::Ic => PartKind::palette_ic(),
        }
    }
}

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
    scope_t: f64,
    scope_auto: bool,
    scope_now: bool,
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
    palette: PalTab,
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
    /// Bibliothek eigener ICs (Titel = Name)
    ic_lib: Vec<Document>,
    /// Dialog „Als IC speichern“: eingegebener Name
    save_ic: Option<String>,
    /// Vom Testskript gedrückter Taster
    script_pressed: Option<u32>,
    /// Palettenkacheln des letzten Bildes (für Testskripte)
    tile_rects: Vec<(String, egui::Rect)>,
    /// Zuletzt automatisch gesicherter Stand und Zeitpunkt
    autosaved: Option<Document>,
    autosave_due: f64,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, script: Option<Vec<script::Cmd>>, file: Option<PathBuf>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
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
            scope_t: ex.scope_t,
            scope_auto: true,
            scope_now: false,
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
            palette: PalTab::Analog,
            msim: None,
            msim_key: None,
            nl_gen: 0,
            msim_gen: 0,
            mixed_logic: Vec::new(),
            pin_logic: HashMap::new(),
            msim_lag: false,
            ic_lib: load_ic_lib(),
            save_ic: None,
            script_pressed: None,
            tile_rects: Vec::new(),
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
            let has_ic = ex.doc_has_ic();
            self.replace_doc(ex.doc);
            self.saved = self.doc.clone();
            self.file = None;
            self.scope_t = ex.scope_t;
            self.palette = if has_ic {
                PalTab::Ic
            } else if ex.digital {
                PalTab::Digital
            } else {
                PalTab::Analog
            };
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
        }
    }

    fn redo(&mut self) {
        if self.hist.redo(&mut self.doc) {
            self.ed.prune(&self.doc);
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
    }

    // ------------------------------------------------------------------
    // Simulation
    // ------------------------------------------------------------------

    fn update_sim(&mut self) {
        let mut pressed: Vec<u32> = self.pressed.iter().copied().collect();
        pressed.sort();
        let key_ok = self.sim_key.as_ref().is_some_and(|(d, on, p)| *d == self.doc && *on == self.sim_on && *p == pressed);
        if !key_ok {
            let nl = netlist::build_with(&self.doc, &self.pressed);
            self.nl_gen += 1;
            self.op = if !self.sim_on {
                Err(String::new())
            } else if nl.mixed.is_some() {
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
        if self.show_scope && (self.scope_now || (self.scope_auto && stale)) {
            if let Some(nl) = &self.nl {
                self.scope = Some(sim::record(&self.doc, nl, self.scope_t));
            }
            self.scope_key = Some((self.doc.clone(), t_bits));
            self.scope_now = false;
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
                for (k, ex) in examples::all().iter().enumerate() {
                    if ui.button(RichText::new(ex.title).size(14.0)).on_hover_text(ex.summary).clicked() {
                        self.load_example(k);
                        ui.close();
                    }
                }
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
                    ui.label(RichText::new("Simulation aus").color(MUTED).strong());
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
                        .color(MUTED),
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
                ui.label(RichText::new(format!("Digital: {} Bausteine · t = {}", d.comps.len(), fmt_si(ds.time as f64 * 1e-9, "s"))).color(MUTED));
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
            for (tab, label) in [(PalTab::Analog, "Analog"), (PalTab::Digital, "Digital"), (PalTab::Ic, "ICs")] {
                if ui.selectable_label(self.palette == tab, RichText::new(label).size(14.0).strong()).clicked() {
                    self.palette = tab;
                    match tab {
                        PalTab::Digital => self.topic = Topic::Logic,
                        PalTab::Ic => self.topic = Topic::RealIc,
                        PalTab::Analog => {}
                    }
                }
            }
        });
        let mut groups = self.palette.groups();
        if self.palette == PalTab::Ic {
            let mut own = vec![PartKind::Port];
            own.extend(self.ic_lib.iter().map(|d| PartKind::Custom { name: d.title.clone(), def: Box::new(d.clone()) }));
            groups.push(("Eigene ICs", own));
        }
        let mut remove_ic: Option<String> = None;
        self.tile_rects.clear();
        for (cat, kinds) in groups {
            ui.label(RichText::new(cat).small().color(MUTED));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                for kind in kinds {
                    let selected = matches!(&self.ed.tool, Tool::Place(k) if *k == kind);
                    let r =
                        tile(ui, &kind, selected).on_hover_text(format!("{}: in den Schaltplan ziehen oder anklicken und absetzen", kind.label()));
                    self.tile_rects.push((kind.label().to_string(), r.rect));
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
            });
            ui.add_space(2.0);
        }
        if self.palette == PalTab::Ic
            && ui
                .button("💾 Schaltplan als IC speichern …")
                .on_hover_text("Die aktuelle Schaltung wird ein Baustein. Ihre IC-Anschlüsse werden die Pins.")
                .clicked()
        {
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
        self.palette = PalTab::Ic;
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
                        ui.label(RichText::new(format!("Anschlüsse {label}: {t}")).color(MUTED));
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
    fn si_field(&mut self, ui: &mut egui::Ui, label: &str, unit: &str, value: f64, positive: bool, key: &str) -> Option<f64> {
        let k = format!("{key}:{}", value.to_bits());
        let buf = self.bufs.entry(k).or_insert_with(|| fmt_si(value, unit));
        let mut result = None;
        ui.horizontal(|ui| {
            ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new(label).color(MUTED)));
            let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(120.0));
            let parsed = parse_si(buf).filter(|v| v.is_finite() && (!positive || *v > 0.0));
            match parsed {
                None => {
                    ui.label(RichText::new("⚠").color(RED)).on_hover_text("Wert nicht lesbar. Beispiele: 4k7, 100n, 2,2 mA, 1M");
                }
                Some(v) if resp.lost_focus() && v != value => result = Some(v),
                _ => {}
            }
        });
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
                ui.label(RichText::new("Klicke ein Bauteil an, um seine Werte zu sehen und zu ändern.").color(MUTED));
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
        // Name
        if !matches!(part.kind, PartKind::Ground) {
            let k = format!("name:{id}:{}", part.name);
            let buf = self.bufs.entry(k).or_insert_with(|| part.name.clone());
            let mut new_name = None;
            ui.horizontal(|ui| {
                ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Name").color(MUTED)));
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
                ui.label(RichText::new(format!("Ersatzwiderstand: {}", fmt_si(v_nom * v_nom / p_nom, "Ω"))).color(MUTED));
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
                ui.label(RichText::new(format!("Effektivwert: {}", fmt_si(amp / 2f64.sqrt(), "V"))).color(MUTED));
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
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Typ").color(MUTED)));
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
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Farbe").color(MUTED)));
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
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Zustand").color(MUTED)));
                    ui.checkbox(&mut c, "geschlossen");
                });
                if c != closed {
                    new_kind = Some(PartKind::Switch { closed: c });
                }
            }
            PartKind::Gate { gate } => {
                let mut g = gate;
                ui.horizontal(|ui| {
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Funktion").color(MUTED)));
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
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Pegel").color(MUTED)));
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
                ui.label(RichText::new(format!("Periode: {}", fmt_si(1.0 / freq, "s"))).color(MUTED));
            }
            PartKind::LogicLed { color } => {
                let mut c = color;
                ui.horizontal(|ui| {
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Farbe").color(MUTED)));
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
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Typ").color(MUTED)));
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
                ui.label(RichText::new(format!("Modell: {info}")).color(MUTED));
            }
            PartKind::Potentiometer { r, pos } => {
                if let Some(v) = self.si_field(ui, "Widerstand", "Ω", r, true, "pr") {
                    new_kind = Some(PartKind::Potentiometer { r: v, pos });
                }
                let mut p = pos * 100.0;
                let resp = ui
                    .horizontal(|ui| {
                        ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Stellung").color(MUTED)));
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
                ui.label(RichText::new("Anteil zwischen Schleifer S und B. Pfeiltasten verstellen ebenfalls.").small().color(MUTED));
            }
            PartKind::GateIc { gate, family } => {
                let mut g = gate;
                ui.horizontal(|ui| {
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Funktion").color(MUTED)));
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
                    ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Typ").color(MUTED)));
                    egui::ComboBox::from_id_salt(("oa", id)).selected_text(k.label()).show_ui(ui, |ui| {
                        for t in elab_app::model::OpAmpType::ALL {
                            ui.selectable_value(&mut k, t, t.label());
                        }
                    });
                });
                if k != kind {
                    new_kind = Some(PartKind::OpAmp { kind: k });
                }
                ui.label(RichText::new(k.describe()).small().color(MUTED));
            }
            PartKind::Timer555 => {
                ui.label(RichText::new("Bipolarer Timer, 4,5 … 16 V. Ausgang 1 ≈ VCC − 1,7 V.").small().color(MUTED));
            }
            PartKind::Adc4 => {
                ui.label(RichText::new("Code = ⌊16 · U_IN / U_REF⌋, Ausgänge wie 74HC.").small().color(MUTED));
            }
            PartKind::Dac4 => {
                ui.label(RichText::new("U_OUT = U_REF · Code / 16, Innenwiderstand 10 kΩ (R-2R).").small().color(MUTED));
            }
            PartKind::Port => {
                ui.label(
                    RichText::new("Der Name wird zum Pin-Namen, wenn du die Schaltung als IC speicherst. VCC und GND landen oben und unten.")
                        .small()
                        .color(MUTED),
                );
            }
            PartKind::Custom { name, def } => {
                let ports: Vec<String> = elab_app::model::custom_ports(&def).into_iter().map(|p| p.1).collect();
                let n = def.parts.iter().filter(|p| !matches!(p.kind, PartKind::Port)).count();
                ui.label(RichText::new(format!("„{name}“: {n} Bauteile innen, Anschlüsse {}", ports.join(", "))).color(MUTED));
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
                        ui.label(RichText::new("Im Schaltplan gedrückt halten.").small().color(MUTED));
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
                        big_value(ui, "I_C / I_B", &format!("{:.0}", v.i / v.ctrl_i), MUTED);
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
                            ui.label(RichText::new(format!("U / I = {}  ✓ = R", fmt_si(v.u / v.i, "Ω"))).color(MUTED));
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
                            RichText::new(format!("U gemessen von {} nach {}; I positiv in dieser Richtung.", pn[0], pn[1])).small().color(MUTED),
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
            ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new("Familie").color(MUTED)));
            egui::ComboBox::from_id_salt(("fam", id)).selected_text(f.label()).show_ui(ui, |ui| {
                for t in elab_app::model::LogicFamily::ALL {
                    ui.selectable_value(&mut f, t, t.label());
                }
            });
        });
        ui.label(RichText::new(f.describe()).small().color(MUTED));
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
                        .color(MUTED),
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
        ui.label(RichText::new("Klicke auf ein Bauteil in der Palette oder im Schaltplan.").small().color(MUTED));
        ui.separator();
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
        ui.label(RichText::new(d.what).size(14.0));
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
        ui.label(d.typical);
        ui.add_space(6.0);
        ui.label(RichText::new("Beobachten & ausprobieren").strong().color(VIOLET));
        ui.label(d.watch);
        if !d.related.is_empty() {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Siehe auch:").color(MUTED));
                for &r in d.related {
                    if ui.link(tutorial::doc(r).title).clicked() {
                        self.topic = r;
                    }
                }
            });
        }
        ui.add_space(10.0);
        ui.separator();
        ui.label(RichText::new("Grundlagen").strong().color(MUTED));
        ui.horizontal_wrapped(|ui| {
            for t in Topic::BASICS {
                if ui.selectable_label(self.topic == t, tutorial::doc(t).title).clicked() {
                    self.topic = t;
                }
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Transistoren & Messen").strong().color(MUTED));
        ui.horizontal_wrapped(|ui| {
            for t in Topic::TRANSISTORS {
                if ui.selectable_label(self.topic == t, tutorial::doc(t).title).clicked() {
                    self.topic = t;
                }
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Echte Bausteine (ICs)").strong().color(MUTED));
        ui.horizontal_wrapped(|ui| {
            for t in Topic::MIXED {
                if ui.selectable_label(self.topic == t, tutorial::doc(t).title).clicked() {
                    self.topic = t;
                }
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Digitaltechnik").strong().color(MUTED));
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
        ui.horizontal(|ui| {
            ui.label(RichText::new("Zeitbasis").color(MUTED));
            let cur = TIMEBASES.iter().find(|t| t.0 == self.scope_t).map_or_else(|| fmt_si(self.scope_t, "s"), |t| t.1.to_string());
            egui::ComboBox::from_id_salt("tb").selected_text(cur).show_ui(ui, |ui| {
                for (t, name) in TIMEBASES {
                    ui.selectable_value(&mut self.scope_t, t, name);
                }
            });
            if ui.button("⟳ Messen").on_hover_text("Zeitverlauf neu simulieren").clicked() {
                self.scope_now = true;
            }
            ui.checkbox(&mut self.scope_auto, "automatisch").on_hover_text("Nach jeder Änderung neu messen");
            if let Some(Ok(s)) = &self.scope {
                ui.label(RichText::new(format!("{} Zeitschritte · {}", s.steps, fmt_si(s.millis * 1e-3, "s"))).color(MUTED));
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
                ui.label(RichText::new("Noch keine Messung. „⟳ Messen“ startet die Zeitsimulation.").color(MUTED));
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("⚠ {e}")).color(RED));
            }
            Some(Ok(s)) if s.traces.is_empty() => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Setze Messpunkte (📍) in die Schaltung, um Spannungen über die Zeit zu sehen.").size(14.0).color(MUTED));
                });
            }
            Some(Ok(s)) => {
                egui_plot::Plot::new("scope")
                    .legend(egui_plot::Legend::default())
                    .x_axis_label(format!("t / {unit}"))
                    .y_axis_label("U / V")
                    .allow_scroll(false)
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
            Example(n) => self.load_example(n),
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
                let own = [PartKind::Port.label(), "Eigenes IC"];
                let has = |t: PalTab| {
                    t.groups().iter().flat_map(|g| g.1.iter()).any(|k| k.label() == label) || (t == PalTab::Ic && own.contains(&label.as_str()))
                };
                let tab = if has(self.palette) {
                    self.palette
                } else {
                    [PalTab::Analog, PalTab::Digital, PalTab::Ic].into_iter().find(|&t| has(t)).unwrap_or(self.palette)
                };
                if tab != self.palette {
                    // Erst den Palettenreiter wechseln, dann im nächsten Schritt ziehen
                    self.palette = tab;
                    let run = self.script.as_mut().unwrap();
                    run.cmds.push_front(PalDrag(label, gx, gy));
                    run.wait = 2;
                    return;
                }
                let Some(rect) = self.tile_rects.iter().find(|t| t.0 == label).map(|t| t.1) else {
                    panic!(
                        "Skript: Palettenkachel „{label}“ nicht gefunden (vorhanden: {:?})",
                        self.tile_rects.iter().map(|t| &t.0).collect::<Vec<_>>()
                    );
                };
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
                    "undo": self.hist.can_undo(),
                    "tool": format!("{:?}", self.ed.tool),
                    "selected": selected,
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
            Palette(tab) => {
                self.palette = match tab {
                    1 => PalTab::Digital,
                    2 => PalTab::Ic,
                    _ => PalTab::Analog,
                }
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
fn tile(ui: &mut egui::Ui, kind: &PartKind, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(72.0, 62.0), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let bg = if selected {
        ACCENT.gamma_multiply(0.18)
    } else if resp.hovered() {
        Color32::from_gray(44)
    } else {
        Color32::from_gray(30)
    };
    painter.rect_filled(rect, 5.0, bg);
    painter.rect_stroke(rect, 5.0, Stroke::new(1.0, if selected { ACCENT } else { Color32::from_gray(52) }), StrokeKind::Inside);
    let icon_c = rect.center_top() + Vec2::new(0.0, 22.0);
    let col = if selected { ACCENT } else { Color32::from_rgb(208, 211, 219) };
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
    let pen = symbols::Pen { painter: &painter, origin: icon_c + off, s, rot, color: col, width: 1.5 };
    if let PartKind::Probe = kind {
        let pin = icon_c + Vec2::new(-12.0, 8.0);
        painter.line_segment([pin, pin + Vec2::new(8.0, -8.0)], Stroke::new(1.5, ACCENT));
        painter.circle_filled(pin, 3.0, ACCENT);
        let r = egui::Rect::from_min_size(pin + Vec2::new(8.0, -20.0), Vec2::new(24.0, 13.0));
        painter.rect_stroke(r, 2.0, Stroke::new(1.3, ACCENT), StrokeKind::Inside);
        painter.text(r.center(), egui::Align2::CENTER_CENTER, "U", egui::FontId::proportional(10.0), Color32::WHITE);
    } else {
        symbols::draw(&pen, kind, symbols::Live::default());
    }
    let short = match kind {
        PartKind::DcVoltage { .. } => "Batterie",
        PartKind::AcVoltage { .. } => "Wechsel",
        PartKind::SquareVoltage { .. } => "Rechteck",
        PartKind::CurrentSource { .. } => "Strom",
        PartKind::Lamp { .. } => "Lampe",
        PartKind::Led { .. } => "LED",
        PartKind::Transistor { kind } => match kind.family() {
            elab_app::model::TransistorFamily::Npn => "NPN",
            elab_app::model::TransistorFamily::Pnp => "PNP",
            elab_app::model::TransistorFamily::Nmos => "N-MOS",
            elab_app::model::TransistorFamily::Pmos => "P-MOS",
        },
        PartKind::Potentiometer { .. } => "Poti",
        PartKind::Ammeter => "Ampere",
        PartKind::Gate { gate } => gate.label(),
        PartKind::LogicInput { .. } => "Schalter",
        PartKind::Clock { .. } => "Takt",
        PartKind::LogicLed { .. } => "Anzeige",
        PartKind::DFlipFlop => "D-FF",
        PartKind::JkFlipFlop => "JK-FF",
        PartKind::Counter4 => "Zähler",
        PartKind::SevenSeg => "7-Segment",
        PartKind::Adc4 => "ADC",
        PartKind::Dac4 => "DAC",
        other if other.is_ic() => {
            let t = other.ic_type().unwrap_or_default();
            painter.text(rect.center_bottom() - Vec2::new(0.0, 5.0), egui::Align2::CENTER_BOTTOM, t, egui::FontId::proportional(11.0), MUTED);
            return resp;
        }
        other => other.label(),
    };
    painter.text(rect.center_bottom() - Vec2::new(0.0, 5.0), egui::Align2::CENTER_BOTTOM, short, egui::FontId::proportional(11.0), MUTED);
    resp
}

fn big_value(ui: &mut egui::Ui, label: &str, value: &str, col: Color32) {
    ui.horizontal(|ui| {
        ui.add_sized([96.0, 20.0], egui::Label::new(RichText::new(label).color(MUTED)));
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
        self.global_keys(&ctx);
        self.update_sim();
        self.update_digital(&ctx);
        self.update_mixed(&ctx);
        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        self.save_ic_dialog(&ctx);
        egui::Panel::left("left").resizable(true).default_size(330.0).min_size(300.0).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                self.palette(ui);
                ui.separator();
                self.inspector(ui);
            });
        });
        if self.show_tutorial {
            egui::Panel::right("tutorial").resizable(true).default_size(380.0).min_size(300.0).show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("tut").auto_shrink(false).show(ui, |ui| self.tutorial_ui(ui));
            });
        }
        if self.show_scope {
            egui::Panel::bottom("scope").resizable(true).default_size(270.0).min_size(150.0).show(ui, |ui| self.bottom_ui(ui));
        }
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let mixed = self.msim.is_some();
            let live = LiveData {
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
        self.autosave(&ctx);
        self.run_script(&ctx);
    }
}
