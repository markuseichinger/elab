//! elab – Elektronik-Lernlabor: Schaltungen bauen und live simulieren.

mod editor;
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

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let script = arg("--script").map(|p| {
        let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("Skript {p}: {e}"));
        script::parse(&src).unwrap_or_else(|e| panic!("{e}"))
    });
    let size = arg("--size")
        .and_then(|s| {
            s.split_once('x')
                .and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?]))
        })
        .unwrap_or([1560.0, 980.0]);
    let file = args
        .iter()
        .skip(1)
        .find(|a| a.ends_with(".elab"))
        .map(PathBuf::from);
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("elab – Elektronik-Lernlabor")
            .with_inner_size(size)
            .with_min_inner_size([1100.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "elab",
        opts,
        Box::new(move |cc| Ok(Box::new(App::new(cc, script, file)))),
    )
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
    sim_key: Option<(Document, bool)>,
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
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        script: Option<Vec<script::Cmd>>,
        file: Option<PathBuf>,
    ) -> Self {
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
            self.replace_doc(ex.doc);
            self.saved = self.doc.clone();
            self.file = None;
            self.scope_t = ex.scope_t;
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
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("elab-Schaltung", &["elab"])
            .pick_file()
        {
            self.open_path(p);
        }
    }

    fn open_path(&mut self, p: PathBuf) {
        match std::fs::read_to_string(&p)
            .map_err(|e| e.to_string())
            .and_then(|s| Document::from_json(&s))
        {
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
                let name = if self.doc.title.is_empty() {
                    "Schaltung".to_string()
                } else {
                    self.doc.title.clone()
                };
                rfd::FileDialog::new()
                    .add_filter("elab-Schaltung", &["elab"])
                    .set_file_name(format!("{name}.elab"))
                    .save_file()
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
        let key_ok = self
            .sim_key
            .as_ref()
            .is_some_and(|(d, on)| *d == self.doc && *on == self.sim_on);
        if !key_ok {
            let nl = netlist::build(&self.doc);
            self.op = if self.sim_on {
                sim::operating_point(&nl)
            } else {
                Err(String::new())
            };
            self.nl = Some(nl);
            self.sim_key = Some((self.doc.clone(), self.sim_on));
        }
        let t_bits = self.scope_t.to_bits();
        let stale = self
            .scope_key
            .as_ref()
            .is_none_or(|(d, t)| *d != self.doc || *t != t_bits);
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
            if ui
                .add(big("🗋 Neu"))
                .on_hover_text("Leerer Schaltplan (⌘N). Rückgängig holt den alten zurück.")
                .clicked()
            {
                self.new_doc();
            }
            if ui
                .add(big("📂 Öffnen"))
                .on_hover_text("Schaltung laden (⌘O)")
                .clicked()
            {
                self.open_dialog();
            }
            if ui
                .add(big("💾 Speichern"))
                .on_hover_text("Speichern (⌘S), Rechtsklick: speichern unter")
                .clicked()
            {
                self.save(false);
            }
            ui.menu_button(RichText::new("📚 Beispiele").size(15.0), |ui| {
                for (k, ex) in examples::all().iter().enumerate() {
                    if ui
                        .button(RichText::new(ex.title).size(14.0))
                        .on_hover_text(ex.summary)
                        .clicked()
                    {
                        self.load_example(k);
                        ui.close();
                    }
                }
            });
            ui.separator();
            if ui
                .add_enabled(self.hist.can_undo(), big("⟲"))
                .on_hover_text("Rückgängig (⌘Z)")
                .clicked()
            {
                self.undo();
            }
            if ui
                .add_enabled(self.hist.can_redo(), big("⟳"))
                .on_hover_text("Wiederholen (⇧⌘Z)")
                .clicked()
            {
                self.redo();
            }
            ui.separator();
            let r = ui
                .add(
                    egui::Button::new(RichText::new("⚡ Live-Simulation").size(15.0))
                        .selected(self.sim_on),
                )
                .on_hover_text(
                    "Arbeitspunkt nach jeder Änderung neu berechnen und Spannungen einfärben",
                );
            if r.clicked() {
                self.sim_on = !self.sim_on;
                self.topic = Topic::OperatingPoint;
            }
            let r = ui.add(
                egui::Button::new(RichText::new("📈 Oszilloskop").size(15.0))
                    .selected(self.show_scope),
            );
            if r.clicked() {
                self.show_scope = !self.show_scope;
                self.topic = Topic::Scope;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.show_tutorial {
                    "🎓 Tutorial an"
                } else {
                    "🎓 Tutorial"
                };
                if ui
                    .add(
                        egui::Button::new(RichText::new(label).size(15.0))
                            .selected(self.show_tutorial),
                    )
                    .clicked()
                {
                    self.show_tutorial = !self.show_tutorial;
                }
                ui.separator();
                if ui
                    .add(big("⤢ Einpassen"))
                    .on_hover_text("Ganze Schaltung anzeigen")
                    .clicked()
                {
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
                Ok(op) => {
                    let nl = self.nl.as_ref().unwrap();
                    let n_nets = nl.nets.iter().filter(|n| n.reachable && !n.is_gnd).count();
                    ui.label(
                        RichText::new("✔ Arbeitspunkt berechnet")
                            .color(GREEN)
                            .strong(),
                    );
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
                    if !nl.excluded.is_empty() {
                        let names: Vec<String> = nl
                            .excluded
                            .iter()
                            .filter_map(|id| self.doc.part(*id))
                            .map(|p| p.name.clone())
                            .collect();
                        ui.label(
                            RichText::new(format!("· nicht verbunden: {}", names.join(", ")))
                                .color(ACCENT),
                        );
                    }
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
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(500));
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
                .add(
                    egui::Button::new(RichText::new("⬉ Auswahl").size(14.0))
                        .selected(self.ed.tool == Tool::Select),
                )
                .on_hover_text("Auswählen, verschieben, vom Anschluss aus verdrahten (Esc)")
                .clicked()
            {
                self.ed.tool = Tool::Select;
            }
            if ui
                .add(
                    egui::Button::new(RichText::new("╱ Draht").size(14.0))
                        .selected(self.ed.tool == Tool::Wire),
                )
                .on_hover_text("Drähte Ecke für Ecke verlegen (W)")
                .clicked()
            {
                self.ed.tool = Tool::Wire;
                self.topic = Topic::Editor;
            }
        });
        ui.add_space(4.0);
        for (cat, kinds) in PartKind::palette() {
            ui.label(RichText::new(cat).small().color(MUTED));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                for kind in kinds {
                    let selected = matches!(&self.ed.tool, Tool::Place(k) if std::mem::discriminant(k) == std::mem::discriminant(&kind));
                    let r = tile(ui, &kind, selected).on_hover_text(format!("{}: anklicken, dann im Schaltplan absetzen", kind.label()));
                    if r.clicked() {
                        self.topic = Topic::of(&kind);
                        self.ed.tool = Tool::Place(kind);
                        self.ed.place_rot = 0;
                    }
                }
            });
            ui.add_space(2.0);
        }
    }

    // ------------------------------------------------------------------
    // Inspektor
    // ------------------------------------------------------------------

    /// Textfeld mit SI-Eingabe. Gibt den neuen Wert zurück, wenn er bestätigt wurde.
    fn si_field(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        unit: &str,
        value: f64,
        positive: bool,
        key: &str,
    ) -> Option<f64> {
        let k = format!("{key}:{}", value.to_bits());
        let buf = self.bufs.entry(k).or_insert_with(|| fmt_si(value, unit));
        let mut result = None;
        ui.horizontal(|ui| {
            ui.add_sized(
                [96.0, 20.0],
                egui::Label::new(RichText::new(label).color(MUTED)),
            );
            let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(120.0));
            let parsed = parse_si(buf).filter(|v| v.is_finite() && (!positive || *v > 0.0));
            match parsed {
                None => {
                    ui.label(RichText::new("⚠").color(RED))
                        .on_hover_text("Wert nicht lesbar. Beispiele: 4k7, 100n, 2,2 mA, 1M");
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
                        self.doc.transform_parts(
                            &ids,
                            &Default::default(),
                            Default::default(),
                            |p| p.rot = (p.rot + 1) % 4,
                        );
                    }
                    if ui.button("🗑 Löschen (Entf)").clicked() {
                        self.hist.checkpoint(&self.doc);
                        self.doc.remove(&self.ed.sel_parts, &self.ed.sel_wires);
                        self.ed.clear_selection();
                    }
                });
            } else {
                ui.label(
                    RichText::new("Klicke ein Bauteil an, um seine Werte zu sehen und zu ändern.")
                        .color(MUTED),
                );
            }
            return;
        };
        let Some(part) = self.doc.part(id).cloned() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(part.kind.label()).size(15.0).strong());
            if ui
                .small_button("?")
                .on_hover_text("Erklärung im Tutorial")
                .clicked()
            {
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
                ui.add_sized(
                    [96.0, 20.0],
                    egui::Label::new(RichText::new("Name").color(MUTED)),
                );
                let r = ui.add(egui::TextEdit::singleline(buf).desired_width(120.0));
                let trimmed = buf.trim().to_string();
                let taken = self
                    .doc
                    .parts
                    .iter()
                    .any(|p| p.id != id && p.name == trimmed);
                if taken {
                    ui.label(RichText::new("⚠").color(RED))
                        .on_hover_text("Name bereits vergeben");
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
                ui.label(
                    RichText::new(format!(
                        "Ersatzwiderstand: {}",
                        fmt_si(v_nom * v_nom / p_nom, "Ω")
                    ))
                    .color(MUTED),
                );
            }
            PartKind::DcVoltage { v } => {
                if let Some(x) = self.si_field(ui, "Spannung", "V", v, false, "v") {
                    new_kind = Some(PartKind::DcVoltage { v: x });
                }
            }
            PartKind::AcVoltage { amp, freq, offset } => {
                if let Some(x) = self.si_field(ui, "Amplitude", "V", amp, false, "amp") {
                    new_kind = Some(PartKind::AcVoltage {
                        amp: x.abs(),
                        freq,
                        offset,
                    });
                }
                if let Some(x) = self.si_field(ui, "Frequenz", "Hz", freq, true, "f") {
                    new_kind = Some(PartKind::AcVoltage {
                        amp,
                        freq: x,
                        offset,
                    });
                }
                if let Some(x) = self.si_field(ui, "Offset", "V", offset, false, "off") {
                    new_kind = Some(PartKind::AcVoltage {
                        amp,
                        freq,
                        offset: x,
                    });
                }
                ui.label(
                    RichText::new(format!("Effektivwert: {}", fmt_si(amp / 2f64.sqrt(), "V")))
                        .color(MUTED),
                );
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
                    ui.add_sized(
                        [96.0, 20.0],
                        egui::Label::new(RichText::new("Typ").color(MUTED)),
                    );
                    egui::ComboBox::from_id_salt(("dt", id))
                        .selected_text(k.label())
                        .show_ui(ui, |ui| {
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
                    ui.add_sized(
                        [96.0, 20.0],
                        egui::Label::new(RichText::new("Farbe").color(MUTED)),
                    );
                    egui::ComboBox::from_id_salt(("lc", id))
                        .selected_text(c.label())
                        .show_ui(ui, |ui| {
                            for t in LedColor::ALL {
                                ui.selectable_value(
                                    &mut c,
                                    t,
                                    format!("{} ({})", t.label(), fmt_si(t.vf(), "V")),
                                );
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
                    ui.add_sized(
                        [96.0, 20.0],
                        egui::Label::new(RichText::new("Zustand").color(MUTED)),
                    );
                    ui.checkbox(&mut c, "geschlossen");
                });
                if c != closed {
                    new_kind = Some(PartKind::Switch { closed: c });
                }
            }
            PartKind::Ground | PartKind::Probe => {}
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
                self.doc
                    .transform_parts(&ids, &Default::default(), Default::default(), |p| {
                        p.rot = (p.rot + 1) % 4
                    });
            }
            if ui.button("🗑 Löschen").clicked() {
                self.hist.checkpoint(&self.doc);
                self.doc.remove(&[id].into(), &Default::default());
                self.ed.clear_selection();
            }
        });
        // Messwerte
        ui.add_space(6.0);
        let vals = self
            .op
            .as_ref()
            .ok()
            .and_then(|o| o.parts.get(&id))
            .copied();
        let probe_v = self.probe_voltage(id);
        if vals.is_some() || probe_v.is_some() {
            egui::Frame::group(ui.style())
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("Messwerte (Arbeitspunkt)")
                            .strong()
                            .color(BLUE),
                    );
                    if let Some(v) = probe_v {
                        big_value(ui, "U", &fmt_si(v, "V"), ACCENT);
                    }
                    if let Some(v) = vals {
                        big_value(ui, "U", &fmt_si(v.u, "V"), ACCENT);
                        big_value(ui, "I", &fmt_si(v.i, "A"), BLUE);
                        let pl = if part.kind.shape() == Shape::Source {
                            "P (abgegeben)"
                        } else {
                            "P"
                        };
                        big_value(ui, pl, &fmt_si(v.p, "W"), GREEN);
                        if let PartKind::Resistor { r } = part.kind {
                            if v.i.abs() > 1e-15 {
                                ui.label(
                                    RichText::new(format!(
                                        "U / I = {}  ✓ = R",
                                        fmt_si(v.u / v.i, "Ω")
                                    ))
                                    .color(MUTED),
                                );
                            }
                            if v.p > 0.25 {
                                ui.label(
                                    RichText::new(format!(
                                        "Achtung: {} übersteigt einen ¼-W-Widerstand",
                                        fmt_si(v.p, "W")
                                    ))
                                    .color(RED),
                                );
                            }
                            let _ = r;
                        }
                        if let PartKind::Led { .. } = part.kind {
                            if v.i > 0.03 {
                                ui.label(
                                    RichText::new(
                                        "Achtung: Strom über 30 mA, eine echte LED wäre überlastet",
                                    )
                                    .color(RED),
                                );
                            }
                        }
                        let pn = part.kind.pin_names();
                        if pn.len() == 2 {
                            ui.label(
                                RichText::new(format!(
                                    "U gemessen von {} nach {}; I positiv in dieser Richtung.",
                                    pn[0], pn[1]
                                ))
                                .small()
                                .color(MUTED),
                            );
                        }
                    }
                });
        } else if self.nl.as_ref().is_some_and(|nl| nl.excluded.contains(&id)) {
            ui.label(
                RichText::new("Nicht mit der Masse verbunden: wird nicht simuliert.").color(ACCENT),
            );
        }
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
            ui.label(
                RichText::new("🎓 Tutorial")
                    .size(17.0)
                    .strong()
                    .color(ACCENT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("✕")
                    .on_hover_text("Tutorial ausblenden")
                    .clicked()
                {
                    self.show_tutorial = false;
                }
            });
        });
        ui.label(
            RichText::new("Klicke auf ein Bauteil in der Palette oder im Schaltplan.")
                .small()
                .color(MUTED),
        );
        ui.separator();
        let d = tutorial::doc(self.topic);
        ui.label(RichText::new(d.title).size(19.0).strong());
        ui.add_space(4.0);
        if let Some(c) = self.tutorial_current() {
            egui::Frame::new()
                .fill(Color32::from_rgb(40, 34, 24))
                .corner_radius(4.0)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(c).color(ACCENT));
                });
            ui.add_space(4.0);
        }
        ui.label(RichText::new(d.what).size(14.0));
        if !d.math.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Formeln").strong().color(BLUE));
            egui::Frame::new()
                .fill(Color32::from_gray(22))
                .corner_radius(4.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(d.math).monospace().size(13.0));
                });
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Übliche Werte").strong().color(GREEN));
        ui.label(d.typical);
        ui.add_space(6.0);
        ui.label(
            RichText::new("Beobachten & ausprobieren")
                .strong()
                .color(VIOLET),
        );
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
                if ui
                    .selectable_label(self.topic == t, tutorial::doc(t).title)
                    .clicked()
                {
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
        let base = format!(
            "{} = {}:  U = {},  I = {},  P = {}",
            part.name,
            part.kind.value_text(),
            fmt_si(v.u, "V"),
            fmt_si(v.i, "A"),
            fmt_si(v.p, "W")
        );
        let extra = match &part.kind {
            PartKind::Resistor { r } if v.i.abs() > 1e-15 => format!(
                "\nOhm: U / I = {} = R ✓  ({} · {} = {})",
                fmt_si(v.u / v.i, "Ω"),
                fmt_si(*r, "Ω"),
                fmt_si(v.i, "A"),
                fmt_si(r * v.i, "V")
            ),
            PartKind::Led { color } => {
                format!("\nFlussspannung bei 20 mA: {}", fmt_si(color.vf(), "V"))
            }
            PartKind::Capacitor { .. } => {
                "\nIm Arbeitspunkt ist der Kondensator geladen: I = 0.".to_string()
            }
            PartKind::Inductor { .. } => {
                "\nIm Arbeitspunkt wirkt die Spule wie ein Draht: U ≈ 0.".to_string()
            }
            _ => String::new(),
        };
        Some(base + &extra)
    }

    // ------------------------------------------------------------------
    // Oszilloskop
    // ------------------------------------------------------------------

    fn scope_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let r = ui.label(
                RichText::new("📈 Oszilloskop")
                    .size(16.0)
                    .strong()
                    .color(ACCENT),
            );
            if r.clicked() {
                self.topic = Topic::Scope;
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Zeitbasis").color(MUTED));
            let cur = TIMEBASES
                .iter()
                .find(|t| t.0 == self.scope_t)
                .map_or_else(|| fmt_si(self.scope_t, "s"), |t| t.1.to_string());
            egui::ComboBox::from_id_salt("tb")
                .selected_text(cur)
                .show_ui(ui, |ui| {
                    for (t, name) in TIMEBASES {
                        ui.selectable_value(&mut self.scope_t, t, name);
                    }
                });
            if ui
                .button("⟳ Messen")
                .on_hover_text("Zeitverlauf neu simulieren")
                .clicked()
            {
                self.scope_now = true;
            }
            ui.checkbox(&mut self.scope_auto, "automatisch")
                .on_hover_text("Nach jeder Änderung neu messen");
            if let Some(Ok(s)) = &self.scope {
                ui.label(
                    RichText::new(format!(
                        "{} Zeitschritte · {}",
                        s.steps,
                        fmt_si(s.millis * 1e-3, "s")
                    ))
                    .color(MUTED),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("✕")
                    .on_hover_text("Oszilloskop ausblenden")
                    .clicked()
                {
                    self.show_scope = false;
                }
            });
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
                ui.label(
                    RichText::new("Noch keine Messung. „⟳ Messen“ startet die Zeitsimulation.")
                        .color(MUTED),
                );
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
                            let pts: Vec<[f64; 2]> =
                                tr.pts.iter().map(|q| [q[0] * scale, q[1]]).collect();
                            p.line(
                                egui_plot::Line::new(
                                    tr.name.clone(),
                                    egui_plot::PlotPoints::from(pts),
                                )
                                .color(probe_color(tr.color_idx))
                                .width(2.0),
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
            run.wait = run
                .wait
                .saturating_sub(if run.frames.is_empty() { 1 } else { 0 });
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
                        let kind = PartKind::palette()
                            .into_iter()
                            .flat_map(|c| c.1)
                            .find(|k| k.label() == other || k.prefix() == other);
                        match kind {
                            Some(k) => editor::Tool::Place(k),
                            None => panic!("Skript: unbekanntes Bauteil {other}"),
                        }
                    }
                }
            }
            Move(x, y) => {
                let p = to_screen(&self.ed, x, y);
                self.script
                    .as_mut()
                    .unwrap()
                    .frames
                    .push_back(vec![egui::Event::PointerMoved(p)]);
            }
            Click(x, y, m) => {
                let p = to_screen(&self.ed, x, y);
                self.script.as_mut().unwrap().click(p, m);
            }
            ClickPx(x, y) => self
                .script
                .as_mut()
                .unwrap()
                .click(egui::pos2(x, y), Modifiers::NONE),
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
                let status = match &self.op {
                    Ok(op) => format!("ok newton={} ", op.newton),
                    Err(e) => format!("err {e}"),
                };
                let s = format!(
                    "{{\"status\": {:?}, \"undo\": {}, \"tool\": {:?}, \"selected\": {:?}, \"doc\": {}}}",
                    status,
                    self.hist.can_undo(),
                    format!("{:?}", self.ed.tool),
                    self.ed.sel_parts.iter().filter_map(|id| self.doc.part(*id)).map(|p| p.name.clone()).collect::<Vec<_>>(),
                    self.doc.to_json()
                );
                if path == "-" {
                    println!("{s}");
                } else {
                    if let Some(dir) = std::path::Path::new(&path).parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    std::fs::write(&path, s).expect("Dump schreiben");
                }
            }
            Quit => std::process::exit(0),
        }
    }
}

/// Kachel in der Bauteil-Palette mit Schaltzeichen und Kurzname.
fn tile(ui: &mut egui::Ui, kind: &PartKind, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(72.0, 62.0), Sense::click());
    let painter = ui.painter_at(rect);
    let bg = if selected {
        ACCENT.gamma_multiply(0.18)
    } else if resp.hovered() {
        Color32::from_gray(44)
    } else {
        Color32::from_gray(30)
    };
    painter.rect_filled(rect, 5.0, bg);
    painter.rect_stroke(
        rect,
        5.0,
        Stroke::new(
            1.0,
            if selected {
                ACCENT
            } else {
                Color32::from_gray(52)
            },
        ),
        StrokeKind::Inside,
    );
    let icon_c = rect.center_top() + Vec2::new(0.0, 22.0);
    let col = if selected {
        ACCENT
    } else {
        Color32::from_rgb(208, 211, 219)
    };
    let (rot, s, off) = match kind.shape() {
        Shape::Source => (3, 13.0, Vec2::ZERO),
        Shape::TwoPole => (0, 14.0, Vec2::ZERO),
        Shape::OnePin => (0, 13.0, Vec2::new(0.0, -7.0)),
    };
    let pen = symbols::Pen {
        painter: &painter,
        origin: icon_c + off,
        s,
        rot,
        color: col,
        width: 1.5,
    };
    if let PartKind::Probe = kind {
        let pin = icon_c + Vec2::new(-12.0, 8.0);
        painter.line_segment([pin, pin + Vec2::new(8.0, -8.0)], Stroke::new(1.5, ACCENT));
        painter.circle_filled(pin, 3.0, ACCENT);
        let r = egui::Rect::from_min_size(pin + Vec2::new(8.0, -20.0), Vec2::new(24.0, 13.0));
        painter.rect_stroke(r, 2.0, Stroke::new(1.3, ACCENT), StrokeKind::Inside);
        painter.text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            "U",
            egui::FontId::proportional(10.0),
            Color32::WHITE,
        );
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
        other => other.label(),
    };
    painter.text(
        rect.center_bottom() - Vec2::new(0.0, 5.0),
        egui::Align2::CENTER_BOTTOM,
        short,
        egui::FontId::proportional(11.0),
        MUTED,
    );
    resp
}

fn big_value(ui: &mut egui::Ui, label: &str, value: &str, col: Color32) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [96.0, 20.0],
            egui::Label::new(RichText::new(label).color(MUTED)),
        );
        ui.label(RichText::new(value).size(16.0).strong().color(col));
    });
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(run) = &mut self.script {
            if let Some(evs) = run.frames.pop_front() {
                let mut m = Modifiers::NONE;
                for e in &evs {
                    if let egui::Event::PointerButton { modifiers, .. }
                    | egui::Event::Key { modifiers, .. } = e
                    {
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
        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::left("left")
            .resizable(true)
            .default_size(330.0)
            .min_size(300.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        self.palette(ui);
                        ui.separator();
                        self.inspector(ui);
                    });
            });
        if self.show_tutorial {
            egui::Panel::right("tutorial")
                .resizable(true)
                .default_size(380.0)
                .min_size(300.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("tut")
                        .auto_shrink(false)
                        .show(ui, |ui| self.tutorial_ui(ui));
                });
        }
        if self.show_scope {
            egui::Panel::bottom("scope")
                .resizable(true)
                .default_size(270.0)
                .min_size(150.0)
                .show(ui, |ui| self.scope_ui(ui));
        }
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let live = LiveData {
                nl: self.nl.as_ref(),
                op: self.op.as_ref().ok(),
            };
            let out = self.ed.ui(ui, &mut self.doc, &mut self.hist, &live);
            if let Some(k) = out.focus {
                self.topic = Topic::of(&k);
            }
        });
        // Fenstertitel
        let name = self
            .file
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| {
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
        self.run_script(&ctx);
    }
}
