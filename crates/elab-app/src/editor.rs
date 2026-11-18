//! Schaltplan-Editor: Zeichenfläche mit Raster, Werkzeugen und Live-Werten.

use crate::symbols::{self, Live, Pen};
use crate::theme::*;
use egui::{Align2, Color32, CursorIcon, FontId, Key, Modifiers, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use elab_app::model::{rot_f, Document, History, IcPin, PartKind, Shape, P};
use elab_app::netlist::Netlist;
use elab_app::sim::OpView;
use elab_sim::digital::Logic;
use elab_sim::fmt_si;
use std::collections::{HashMap, HashSet};

/// Pixel pro Rastereinheit bei Zoom 1.
pub const GRID: f32 = 20.0;
const PART_COL: Color32 = Color32::from_rgb(208, 211, 219);
const PART_HOVER: Color32 = Color32::from_rgb(245, 246, 250);
const PART_OFF: Color32 = Color32::from_rgb(105, 109, 120);
const WIRE_COL: Color32 = Color32::from_rgb(150, 155, 166);

#[derive(Clone, Debug, PartialEq)]
pub enum Tool {
    Select,
    Place(PartKind),
    Wire,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hover {
    None,
    Part(u32),
    Wire(u32),
    Pin(P, u32, usize),
}

enum Drag {
    None,
    Move { origin: P, last: P, base: Document, parts: HashSet<u32>, wires: HashSet<u32> },
    Box { from: Pos2 },
    WireFromPin { from: P },
}

/// Ergebnis eines Frames.
#[derive(Default)]
pub struct Out {
    /// Das Dokument wurde geändert.
    pub changed: bool,
    /// Ein Bauteil wurde angeklickt/platziert: passendes Tutorial-Thema zeigen.
    pub focus: Option<PartKind>,
    /// Ein Bauteil wurde neu gesetzt
    pub placed: Option<u32>,
    /// Logikschalter umgelegt
    pub toggle: Option<u32>,
    /// Taster wird gerade gedrückt gehalten
    pub pressed: Option<u32>,
}

pub struct Editor {
    pub pan: Vec2,
    pub zoom: f32,
    pub tool: Tool,
    pub place_rot: u8,
    pub sel_parts: HashSet<u32>,
    pub sel_wires: HashSet<u32>,
    pub fit_request: bool,
    /// Bauteil, das gerade aus der Palette auf die Zeichenfläche gezogen wird
    pub palette_drag: Option<PartKind>,
    drag: Drag,
    wire_start: Option<P>,
    wire_hfirst: bool,
    hover: Hover,
    last_rect: Rect,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            pan: Vec2::new(300.0, 300.0),
            zoom: 1.0,
            tool: Tool::Select,
            place_rot: 0,
            sel_parts: HashSet::new(),
            sel_wires: HashSet::new(),
            fit_request: true,
            palette_drag: None,
            drag: Drag::None,
            wire_start: None,
            wire_hfirst: true,
            hover: Hover::None,
            last_rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)),
        }
    }
}

/// Werte, die das Zeichnen aus der Simulation braucht.
pub struct LiveData<'a> {
    pub nl: Option<&'a Netlist>,
    pub op: Option<&'a OpView>,
    /// Logikpegel je Netz aus der Digitalsimulation
    pub logic: Option<&'a [Logic]>,
    /// Gemischte Schaltung: Pegel an den Anschlüssen von ICs und gekoppelten Logikbausteinen
    pub pin_logic: Option<&'a HashMap<u32, Vec<Option<Logic>>>>,
    /// Gedrückte Taster
    pub pressed: &'a HashSet<u32>,
    /// Rasterpunkte, die pulsierend markiert werden (geführte Übung)
    pub highlight: &'a [P],
    /// Ströme auf den Leitungsstücken (Stromfluss-Anzeige)
    pub flow: &'a [elab_app::flow::FlowSeg],
    /// Erwärmung der Bauteile (1 = Grenzwert), leer wenn Grenzwerte aus
    pub heat: &'a HashMap<u32, f64>,
}

impl LiveData<'_> {
    /// Logikpegel eines Netzes, falls es digital betrieben wird.
    fn logic_of(&self, net: usize) -> Option<Logic> {
        let nl = self.nl?;
        let is_dig = match (&nl.digital, &nl.mixed) {
            (Some(d), _) => d.is_digital_net.get(net).copied()?,
            (_, Some(m)) => m.digital_net.get(net).copied()?,
            _ => false,
        };
        if is_dig {
            self.logic?.get(net).copied()
        } else {
            None
        }
    }
}

impl Editor {
    fn s(&self) -> f32 {
        GRID * self.zoom
    }

    fn to_screen(&self, r: Rect, x: f32, y: f32) -> Pos2 {
        r.min + self.pan + Vec2::new(x, y) * self.s()
    }

    fn sp(&self, r: Rect, p: P) -> Pos2 {
        self.to_screen(r, p.x as f32, p.y as f32)
    }

    fn to_grid(&self, r: Rect, p: Pos2) -> (f32, f32) {
        let v = (p - r.min - self.pan) / self.s();
        (v.x, v.y)
    }

    fn snap(&self, r: Rect, p: Pos2) -> P {
        let (x, y) = self.to_grid(r, p);
        P::new(x.round() as i32, y.round() as i32)
    }

    /// Bildschirmposition eines Rasterpunkts im zuletzt gezeichneten Frame (für Testskripte).
    pub fn grid_to_screen(&self, x: f32, y: f32) -> Pos2 {
        self.to_screen(self.last_rect, x, y)
    }

    pub fn single_selected(&self) -> Option<u32> {
        if self.sel_parts.len() == 1 && self.sel_wires.is_empty() {
            self.sel_parts.iter().next().copied()
        } else {
            None
        }
    }

    pub fn clear_selection(&mut self) {
        self.sel_parts.clear();
        self.sel_wires.clear();
    }

    pub fn select_only(&mut self, id: u32) {
        self.clear_selection();
        self.sel_parts.insert(id);
    }

    /// Entfernt Auswahl-IDs, die es im Dokument nicht mehr gibt (z. B. nach Undo).
    pub fn prune(&mut self, doc: &Document) {
        self.sel_parts.retain(|id| doc.part(*id).is_some());
        self.sel_wires.retain(|id| doc.wires.iter().any(|w| w.id == *id));
    }

    fn fit(&mut self, doc: &Document, r: Rect) {
        let Some((x0, y0, x1, y1)) = doc.bbox() else {
            self.zoom = 1.0;
            self.pan = r.size() * 0.5;
            return;
        };
        let m = 3.0;
        // Mindestens 28 × 18 Raster: Ein einzelnes Bauteil (Start einer Übung) soll Platz drumherum haben
        let (w, h) = ((x1 - x0 + 2.0 * m).max(28.0) * GRID, (y1 - y0 + 2.0 * m).max(18.0) * GRID);
        self.zoom = (r.width() / w).min(r.height() / h).clamp(0.4, 2.2);
        let c = Vec2::new((x0 + x1) * 0.5, (y0 + y1) * 0.5);
        self.pan = r.size() * 0.5 - c * self.s();
    }

    pub fn zoom_by(&mut self, factor: f32, r: Rect, around: Pos2) {
        let g = self.to_grid(r, around);
        self.zoom = (self.zoom * factor).clamp(0.25, 4.0);
        let now = self.to_screen(r, g.0, g.1);
        self.pan += around - now;
    }

    // -----------------------------------------------------------------
    // Treffertests
    // -----------------------------------------------------------------

    fn pin_at(doc: &Document, g: (f32, f32), radius: f32) -> Option<(P, u32, usize)> {
        let mut best: Option<(f32, P, u32, usize)> = None;
        for part in &doc.parts {
            for (k, p) in part.pins().into_iter().enumerate() {
                let d = ((p.x as f32 - g.0).powi(2) + (p.y as f32 - g.1).powi(2)).sqrt();
                if d < radius && best.is_none_or(|b| d < b.0) {
                    best = Some((d, p, part.id, k));
                }
            }
        }
        best.map(|b| (b.1, b.2, b.3))
    }

    fn part_at(doc: &Document, g: (f32, f32)) -> Option<u32> {
        doc.parts
            .iter()
            .rev()
            .find(|p| {
                let (x0, y0, x1, y1) = p.bbox();
                g.0 >= x0 - 0.1 && g.0 <= x1 + 0.1 && g.1 >= y0 - 0.1 && g.1 <= y1 + 0.1
            })
            .map(|p| p.id)
    }

    fn wire_at(doc: &Document, g: (f32, f32), tol: f32) -> Option<u32> {
        doc.wires
            .iter()
            .find(|w| {
                let (ax, ay, bx, by) = (w.a.x as f32, w.a.y as f32, w.b.x as f32, w.b.y as f32);
                let (dx, dy) = (bx - ax, by - ay);
                let len2 = dx * dx + dy * dy;
                let t = (((g.0 - ax) * dx + (g.1 - ay) * dy) / len2).clamp(0.0, 1.0);
                let (px, py) = (ax + t * dx, ay + t * dy);
                ((g.0 - px).powi(2) + (g.1 - py).powi(2)).sqrt() < tol
            })
            .map(|w| w.id)
    }

    fn hit(&self, doc: &Document, g: (f32, f32)) -> Hover {
        if self.tool == Tool::Select {
            if let Some((p, id, k)) = Self::pin_at(doc, g, 0.42) {
                return Hover::Pin(p, id, k);
            }
        }
        if let Some(id) = Self::part_at(doc, g) {
            return Hover::Part(id);
        }
        if let Some(id) = Self::wire_at(doc, g, 0.45) {
            return Hover::Wire(id);
        }
        Hover::None
    }

    /// Nächster Rasterpunkt auf einem Draht (für Abzweige).
    fn wire_point(doc: &Document, id: u32, g: (f32, f32)) -> Option<P> {
        let w = doc.wires.iter().find(|w| w.id == id)?;
        let p = if w.a.y == w.b.y {
            P::new((g.0.round() as i32).clamp(w.a.x.min(w.b.x), w.a.x.max(w.b.x)), w.a.y)
        } else {
            P::new(w.a.x, (g.1.round() as i32).clamp(w.a.y.min(w.b.y), w.a.y.max(w.b.y)))
        };
        Some(p)
    }

    fn corner(a: P, b: P, hfirst: bool) -> P {
        if hfirst {
            P::new(b.x, a.y)
        } else {
            P::new(a.x, b.y)
        }
    }

    /// Setzt ein Bauteil ab. Liegt es auf einer Leitung, wird es eingefügt.
    fn place(&mut self, doc: &mut Document, hist: &mut History, kind: PartKind, p: P, out: &mut Out) {
        hist.checkpoint(doc);
        let rot = doc.snap_rotation(&kind, p, self.place_rot);
        let id = doc.add_part(kind.clone(), p, rot);
        doc.normalize();
        self.select_only(id);
        self.tool = Tool::Select;
        out.changed = true;
        out.focus = Some(kind);
        out.placed = Some(id);
    }

    fn rotate_selection(&mut self, doc: &mut Document, hist: &mut History) -> bool {
        if self.sel_parts.is_empty() {
            return false;
        }
        hist.checkpoint(doc);
        doc.transform_parts(&self.sel_parts, &HashSet::new(), P::default(), |p| p.rot = (p.rot + 1) % 4);
        true
    }

    fn delete_selection(&mut self, doc: &mut Document, hist: &mut History) -> bool {
        if self.sel_parts.is_empty() && self.sel_wires.is_empty() {
            return false;
        }
        hist.checkpoint(doc);
        doc.remove(&self.sel_parts, &self.sel_wires);
        self.clear_selection();
        true
    }

    // -----------------------------------------------------------------
    // Hauptfunktion
    // -----------------------------------------------------------------

    pub fn ui(&mut self, ui: &mut Ui, doc: &mut Document, hist: &mut History, live: &LiveData) -> Out {
        let mut out = Out::default();
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let r = resp.rect;
        let painter = painter.with_clip_rect(r);
        self.last_rect = r;
        if self.fit_request {
            self.fit(doc, r);
            self.fit_request = false;
        }
        let pointer = resp.hover_pos();
        let g = pointer.map(|p| self.to_grid(r, p));
        let gs = pointer.map(|p| self.snap(r, p));
        let shift = ui.input(|i| i.modifiers.shift);

        // --- Bauteil aus der Palette hereingezogen ---
        let mut palette_ghost: Option<P> = None;
        if let Some(kind) = self.palette_drag.clone() {
            let pos = ui.input(|i| i.pointer.latest_pos()).filter(|p| r.contains(*p));
            let released = ui.input(|i| i.pointer.any_released() || !i.pointer.any_down());
            if released {
                self.palette_drag = None;
                if let Some(p) = pos {
                    let gp = self.snap(r, p);
                    self.place(doc, hist, kind, gp, &mut out);
                }
            } else {
                palette_ghost = pos.map(|p| self.snap(r, p));
            }
        }

        // --- Ansicht: verschieben und zoomen ---
        if resp.hovered() {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if zoom != 1.0 {
                if let Some(p) = pointer {
                    self.zoom_by(zoom, r, p);
                }
            }
            self.pan += scroll;
        }
        if resp.dragged_by(PointerButton::Middle) || resp.dragged_by(PointerButton::Secondary) {
            self.pan += resp.drag_delta();
        }

        // --- Hover ---
        // Nach einem Drop aus der Palette passt die Netzliste erst im nächsten Bild zum Schaltplan
        if out.changed {
            self.hover = Hover::None;
        } else if matches!(self.drag, Drag::None) {
            self.hover = g.map_or(Hover::None, |g| self.hit(doc, g));
        }

        // --- Tastatur ---
        if !ui.ctx().egui_wants_keyboard_input() {
            let (esc, rk, del, wk, space, all) = ui.input_mut(|i| {
                (
                    i.key_pressed(Key::Escape),
                    i.key_pressed(Key::R) && !i.modifiers.command,
                    i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
                    i.key_pressed(Key::W) && !i.modifiers.command,
                    i.key_pressed(Key::Space),
                    i.consume_key(Modifiers::COMMAND, Key::A),
                )
            });
            // Potentiometer mit den Pfeiltasten verstellen
            let step = ui.input(|i| {
                let fine = if i.modifiers.shift { 0.01 } else { 0.05 };
                if i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::ArrowUp) {
                    fine
                } else if i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::ArrowDown) {
                    -fine
                } else {
                    0.0
                }
            });
            if step != 0.0 {
                if let Some(id) = self.single_selected() {
                    if let Some(PartKind::Potentiometer { r, pos }) = doc.part(id).map(|p| p.kind.clone()) {
                        hist.checkpoint(doc);
                        let pos = ((pos + step) * 100.0).round() / 100.0;
                        doc.part_mut(id).unwrap().kind = PartKind::Potentiometer { r, pos: pos.clamp(0.0, 1.0) };
                        out.changed = true;
                    }
                }
            }
            if esc {
                if let Drag::Move { base, .. } = &self.drag {
                    *doc = base.clone();
                } else if self.palette_drag.is_some() {
                    self.palette_drag = None;
                } else if self.wire_start.is_some() {
                    self.wire_start = None;
                } else if self.tool != Tool::Select {
                    self.tool = Tool::Select;
                } else {
                    self.clear_selection();
                }
                self.drag = Drag::None;
            }
            if rk {
                if let Drag::Move { base, parts, wires, last, .. } = &mut self.drag {
                    // Drehen während des Ziehens: im Ausgangszustand drehen, dann neu verschieben
                    base.transform_parts(parts, &HashSet::new(), P::default(), |p| p.rot = (p.rot + 1) % 4);
                    let d = *last;
                    *doc = base.clone();
                    doc.transform_parts(parts, wires, d, |p| p.pos = p.pos + d);
                } else if matches!(self.tool, Tool::Place(_)) || self.palette_drag.is_some() {
                    self.place_rot = (self.place_rot + 1) % 4;
                } else if self.rotate_selection(doc, hist) {
                    out.changed = true;
                }
            }
            if del && self.delete_selection(doc, hist) {
                out.changed = true;
            }
            if wk {
                self.tool = Tool::Wire;
                self.wire_start = None;
            }
            if space {
                self.wire_hfirst = !self.wire_hfirst;
            }
            if all {
                self.sel_parts = doc.parts.iter().map(|p| p.id).collect();
                self.sel_wires = doc.wires.iter().map(|w| w.id).collect();
            }
        }
        if resp.secondary_clicked() && !matches!(self.tool, Tool::Select) {
            self.tool = Tool::Select;
            self.wire_start = None;
        }

        // --- Taster gedrückt halten ---
        if self.tool == Tool::Select && resp.is_pointer_button_down_on() {
            if let Some(o) = ui.input(|i| i.pointer.press_origin()) {
                if let Hover::Part(id) | Hover::Pin(_, id, _) = self.hit(doc, self.to_grid(r, o)) {
                    if matches!(doc.part(id).map(|p| &p.kind), Some(PartKind::Button | PartKind::PushButton)) {
                        out.pressed = Some(id);
                    }
                }
            }
        }

        // --- Werkzeuge ---
        match self.tool.clone() {
            Tool::Select => self.select_tool(&resp, r, doc, hist, gs, g, shift, &mut out),
            Tool::Place(kind) => {
                if resp.clicked() {
                    if let Some(p) = gs {
                        self.place(doc, hist, kind.clone(), p, &mut out);
                        // Mit gedrückter Umschalttaste bleibt das Werkzeug aktiv
                        if shift {
                            self.tool = Tool::Place(kind);
                        }
                    }
                }
            }
            Tool::Wire => {
                if let Some(p) = gs {
                    if resp.double_clicked() {
                        self.wire_start = None;
                    } else if resp.clicked() {
                        match self.wire_start {
                            None => self.wire_start = Some(p),
                            Some(a) if a != p => {
                                let ends_on_connection = doc.is_connection(p);
                                hist.checkpoint(doc);
                                let c = Self::corner(a, p, self.wire_hfirst);
                                doc.add_wire(a, c);
                                doc.add_wire(c, p);
                                doc.normalize();
                                out.changed = true;
                                self.wire_start = if ends_on_connection { None } else { Some(p) };
                            }
                            Some(_) => {}
                        }
                    }
                }
            }
        }

        // --- Zeichnen ---
        let ghost = match (&self.tool, gs, &self.palette_drag, palette_ghost) {
            (_, _, Some(k), Some(p)) => Some((k.clone(), p)),
            (Tool::Place(k), Some(p), None, _) => Some((k.clone(), p)),
            _ => None,
        };
        self.paint(&painter, r, doc, live, gs, ghost);
        if !live.flow.is_empty() && !matches!(self.drag, Drag::Move { .. }) {
            // Stromfluss: Punkte wandern in technischer Stromrichtung, schneller bei mehr Strom
            let t = ui.input(|i| i.time);
            let s = self.s();
            let mut moving = false;
            for &(a, b, i) in live.flow {
                if i.abs() < 10e-6 {
                    continue;
                }
                moving = true;
                let (from, to) = if i > 0.0 { (a, b) } else { (b, a) };
                let speed = (1.0 + 0.9 * (i.abs() / 1e-5).log10()).clamp(0.4, 5.0);
                let len = ((to.x - from.x).abs() + (to.y - from.y).abs()) as f64;
                let spacing = 1.0;
                let mut d = (t * speed).rem_euclid(spacing);
                let (dx, dy) = ((to.x - from.x).signum() as f64, (to.y - from.y).signum() as f64);
                let alpha = (90.0 + 40.0 * (i.abs() / 1e-5).log10()).clamp(90.0, 255.0) as u8;
                while d < len {
                    let c = self.to_screen(r, (from.x as f64 + dx * d) as f32, (from.y as f64 + dy * d) as f32);
                    painter.circle_filled(c, (s * 0.11).max(1.5), Color32::from_rgba_unmultiplied(255, 236, 140, alpha));
                    d += spacing;
                }
            }
            if moving {
                ui.ctx().request_repaint();
            }
        }
        if !live.highlight.is_empty() {
            // Geführte Übung: Anschlüsse, um die es gerade geht, pulsieren
            let t = ui.input(|i| i.time) as f32;
            let k = 0.5 + 0.5 * (t * 4.0).sin();
            let s = self.s();
            for p in live.highlight {
                let c = self.to_screen(r, p.x as f32, p.y as f32);
                let rad = s * (0.45 + 0.25 * k);
                painter.circle_stroke(c, rad, Stroke::new(2.5, Color32::from_rgba_unmultiplied(255, 170, 40, (120.0 + 135.0 * k) as u8)));
                painter.circle_filled(c, s * 0.18, Color32::from_rgb(255, 170, 40));
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
        }
        self.paint_overlay(&painter, r, doc, live, pointer, gs);

        // Mauszeiger
        if resp.hovered() {
            let icon = match (&self.tool, &self.drag, self.hover) {
                (_, Drag::Move { .. }, _) => CursorIcon::Grabbing,
                (Tool::Wire, _, _) | (_, Drag::WireFromPin { .. }, _) => CursorIcon::Crosshair,
                (Tool::Place(_), _, _) => CursorIcon::Copy,
                (Tool::Select, _, Hover::Pin(..)) => CursorIcon::Crosshair,
                (Tool::Select, _, Hover::Part(_)) => CursorIcon::Grab,
                (Tool::Select, _, Hover::Wire(_)) => CursorIcon::PointingHand,
                _ => CursorIcon::Default,
            };
            ui.ctx().set_cursor_icon(icon);
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn select_tool(
        &mut self,
        resp: &egui::Response,
        r: Rect,
        doc: &mut Document,
        hist: &mut History,
        gs: Option<P>,
        g: Option<(f32, f32)>,
        shift: bool,
        out: &mut Out,
    ) {
        if resp.drag_started_by(PointerButton::Primary) {
            let origin = resp.ctx.input(|i| i.pointer.press_origin()).unwrap_or(r.center());
            let og = self.to_grid(r, origin);
            let osnap = self.snap(r, origin);
            match self.hit(doc, og) {
                Hover::Pin(p, ..) => self.drag = Drag::WireFromPin { from: p },
                Hover::Part(id) => {
                    if !self.sel_parts.contains(&id) {
                        if !shift {
                            self.clear_selection();
                        }
                        self.sel_parts.insert(id);
                    }
                    self.drag = Drag::Move {
                        origin: osnap,
                        last: P::default(),
                        base: doc.clone(),
                        parts: self.sel_parts.clone(),
                        wires: self.sel_wires.clone(),
                    };
                }
                Hover::Wire(id) => {
                    // Von einer Leitung aus ziehen: Leitung verlängern bzw. abzweigen
                    if let Some(from) = Self::wire_point(doc, id, og) {
                        self.drag = Drag::WireFromPin { from };
                    }
                }
                Hover::None => self.drag = Drag::Box { from: origin },
            }
        }
        if resp.dragged_by(PointerButton::Primary) {
            if let (Drag::Move { origin, last, base, parts, wires }, Some(cur)) = (&mut self.drag, gs) {
                let delta = cur - *origin;
                if delta != *last {
                    *doc = base.clone();
                    doc.transform_parts(parts, wires, delta, |p| p.pos = p.pos + delta);
                    *last = delta;
                }
            }
        }
        if resp.drag_stopped() {
            match std::mem::replace(&mut self.drag, Drag::None) {
                Drag::Move { last, base, .. } => {
                    if last != P::default() {
                        hist.checkpoint(&base);
                        out.changed = true;
                    }
                }
                Drag::Box { from } => {
                    if let Some(to) = resp.ctx.input(|i| i.pointer.interact_pos()) {
                        let (a, b) = (self.to_grid(r, from), self.to_grid(r, to));
                        let (x0, x1, y0, y1) = (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1));
                        if !shift {
                            self.clear_selection();
                        }
                        for p in &doc.parts {
                            let (px0, py0, px1, py1) = p.bbox();
                            if px0 >= x0 && px1 <= x1 && py0 >= y0 && py1 <= y1 {
                                self.sel_parts.insert(p.id);
                            }
                        }
                        let inside = |p: P| (p.x as f32) >= x0 && (p.x as f32) <= x1 && (p.y as f32) >= y0 && (p.y as f32) <= y1;
                        for w in &doc.wires {
                            if inside(w.a) && inside(w.b) {
                                self.sel_wires.insert(w.id);
                            }
                        }
                    }
                }
                Drag::WireFromPin { from } => {
                    if let Some(to) = gs {
                        if to != from {
                            hist.checkpoint(doc);
                            let d = to - from;
                            let c = Self::corner(from, to, d.x.abs() >= d.y.abs());
                            doc.add_wire(from, c);
                            doc.add_wire(c, to);
                            doc.normalize();
                            out.changed = true;
                        }
                    }
                }
                Drag::None => {}
            }
        }
        if resp.double_clicked() {
            if let Some(Hover::Part(id) | Hover::Pin(_, id, _)) = g.map(|g| self.hit(doc, g)) {
                if let Some(PartKind::Switch { .. }) = doc.part(id).map(|p| &p.kind) {
                    hist.checkpoint(doc);
                    if let Some(PartKind::Switch { closed }) = doc.part_mut(id).map(|p| &mut p.kind) {
                        *closed = !*closed;
                    }
                    out.changed = true;
                }
            }
        } else if resp.clicked() {
            match g.map(|g| self.hit(doc, g)).unwrap_or(Hover::None) {
                Hover::Part(id) | Hover::Pin(_, id, _) => {
                    if shift {
                        if !self.sel_parts.remove(&id) {
                            self.sel_parts.insert(id);
                        }
                    } else {
                        self.select_only(id);
                    }
                    out.focus = doc.part(id).map(|p| p.kind.clone());
                    if let Some(PartKind::LogicInput { .. }) = doc.part(id).map(|p| &p.kind) {
                        out.toggle = Some(id);
                    }
                }
                Hover::Wire(id) => {
                    if !shift {
                        self.clear_selection();
                    }
                    self.sel_wires.insert(id);
                }
                Hover::None => self.clear_selection(),
            }
        }
    }

    // -----------------------------------------------------------------
    // Zeichnen
    // -----------------------------------------------------------------

    fn paint(&self, painter: &egui::Painter, r: Rect, doc: &Document, live: &LiveData, gs: Option<P>, ghost: Option<(PartKind, P)>) {
        let s = self.s();
        painter.rect_filled(r, 0.0, CANVAS_BG);

        // Raster
        let (gx0, gy0) = self.to_grid(r, r.min);
        let (gx1, gy1) = self.to_grid(r, r.max);
        let step = if s < 9.0 { 5 } else { 1 };
        let (ix0, iy0) = ((gx0.floor() as i32).div_euclid(step) * step, (gy0.floor() as i32).div_euclid(step) * step);
        for y in (iy0..=gy1.ceil() as i32).step_by(step as usize) {
            for x in (ix0..=gx1.ceil() as i32).step_by(step as usize) {
                let major = x % 5 == 0 && y % 5 == 0;
                let c = if major { Color32::from_rgb(56, 59, 68) } else { Color32::from_rgb(36, 38, 45) };
                painter.circle_filled(self.to_screen(r, x as f32, y as f32), if major { 1.4 } else { 1.0 }, c);
            }
        }

        let width = (s * 0.11).clamp(1.4, 3.6);
        let vmax = live.op.map_or(1.0, |o| o.max_abs_v().max(1e-3));
        let net_color = |net: usize| -> Color32 {
            if let Some(v) = live.logic_of(net) {
                return symbols::logic_color(v);
            }
            match (live.nl, live.op) {
                (Some(_), Some(op)) => match op.net_v.get(net).copied().flatten() {
                    Some(v) => volt_color(v, vmax),
                    None => PART_OFF,
                },
                _ => WIRE_COL,
            }
        };
        let point_color = |p: P| -> Color32 {
            match live.nl.and_then(|nl| nl.net_of.get(&p)) {
                Some(&n) => net_color(n),
                None => WIRE_COL,
            }
        };

        // Drähte
        for w in &doc.wires {
            let (a, b) = (self.sp(r, w.a), self.sp(r, w.b));
            if self.sel_wires.contains(&w.id) {
                painter.line_segment([a, b], Stroke::new(width + 5.0, ACCENT.gamma_multiply(0.45)));
            }
            let col = live.nl.and_then(|nl| nl.wire_net.get(&w.id)).map_or(WIRE_COL, |&n| net_color(n));
            let wd = if self.hover == Hover::Wire(w.id) { width + 1.2 } else { width };
            painter.line_segment([a, b], Stroke::new(wd, col));
        }

        // Knotenpunkte und offene Enden
        let mut pts: Vec<P> = doc.wires.iter().flat_map(|w| [w.a, w.b]).collect();
        pts.extend(doc.parts.iter().filter(|p| !matches!(p.kind, PartKind::Probe)).flat_map(|p| {
            // Ausgänge und Steueranschlüsse von ICs dürfen offen bleiben
            let roles = p.kind.ic_pins();
            p.pins().into_iter().enumerate().filter(move |(k, _)| !matches!(roles.get(*k), Some(IcPin::Out | IcPin::Analog))).map(|(_, q)| q)
        }));
        pts.sort();
        pts.dedup();
        for p in pts {
            let deg = doc.degree(p);
            if deg >= 3 {
                painter.circle_filled(self.sp(r, p), width * 1.7, point_color(p));
            } else if deg == 1 {
                painter.circle_stroke(self.sp(r, p), (0.2 * s).max(3.0), Stroke::new(1.3, RED.gamma_multiply(0.85)));
            }
        }

        // Bauteile
        let excluded: HashSet<u32> = live.nl.map(|nl| nl.excluded.iter().copied().collect()).unwrap_or_default();
        let probe_idx: Vec<u32> = doc.parts.iter().filter(|p| matches!(p.kind, PartKind::Probe)).map(|p| p.id).collect();
        let font = FontId::proportional((s * 0.58).clamp(9.0, 16.0));
        let pw = (s * 0.09).clamp(1.2, 3.2);
        for part in &doc.parts {
            let selected = self.sel_parts.contains(&part.id);
            let hovered = matches!(self.hover, Hover::Part(id) | Hover::Pin(_, id, _) if id == part.id);
            let origin = self.sp(r, part.pos);
            let vals = live.op.and_then(|o| o.parts.get(&part.id));
            if let PartKind::Probe = part.kind {
                let k = probe_idx.iter().position(|&i| i == part.id).unwrap_or(0);
                let col = probe_color(k);
                let net = live.nl.and_then(|nl| nl.pin_nets.get(&part.id).map(|p| p[0]));
                let text = match net.and_then(|n| live.logic_of(n)) {
                    Some(l) => Some(l.symbol().to_string()),
                    None => live
                        .nl
                        .zip(live.op)
                        .and_then(|(nl, op)| op.net_v.get(nl.pin_nets.get(&part.id)?[0]).copied().flatten())
                        .map(|v| fmt_si(v, "V")),
                };
                self.paint_probe(painter, origin, &part.name, text, col, selected || hovered, s);
                continue;
            }
            let bridged = live.nl.is_some_and(|nl| nl.bridged.contains(&part.id));
            let heat = live.heat.get(&part.id).copied().unwrap_or(0.0);
            let color = if part.broken {
                if selected {
                    ACCENT
                } else {
                    Color32::from_rgb(120, 90, 90)
                }
            } else if selected {
                ACCENT
            } else if heat > 0.6 && live.op.is_some() {
                // Wird heiß: von Orange nach Rot
                let k = ((heat - 0.6) / 0.4).clamp(0.0, 1.0) as f32;
                let pulse = 0.75 + 0.25 * (painter.ctx().input(|i| i.time) * 8.0).sin() as f32;
                Color32::from_rgb(255, (170.0 * (1.0 - k)) as u8 + 40, 40).gamma_multiply(pulse)
            } else if bridged && live.op.is_some() {
                Color32::from_rgb(235, 200, 70)
            } else if hovered {
                PART_HOVER
            } else if excluded.contains(&part.id) && live.op.is_some() {
                PART_OFF
            } else {
                PART_COL
            };
            let glow = match (&part.kind, vals) {
                _ if part.broken => 0.0,
                (PartKind::Led { .. }, Some(v)) if v.i > 1e-4 => (v.i / 0.02).clamp(0.0, 1.0).sqrt() as f32,
                (PartKind::Lamp { p_nom, .. }, Some(v)) => (v.p / p_nom).clamp(0.0, 1.0) as f32,
                _ => 0.0,
            };
            if selected {
                let (x0, y0, x1, y1) = part.bbox();
                let rr = Rect::from_min_max(self.to_screen(r, x0, y0), self.to_screen(r, x1, y1)).expand(3.0);
                painter.rect_filled(rr, 4.0, ACCENT.gamma_multiply(0.08));
            }
            let pen = Pen { painter, origin, s, rot: part.rot, color, width: pw };
            let pins: Vec<Option<Logic>> = if let Some(pl) = live.pin_logic.and_then(|m| m.get(&part.id)) {
                pl.clone()
            } else if part.kind.is_digital() {
                live.nl
                    .and_then(|nl| nl.pin_nets.get(&part.id))
                    .map(|pn| pn.iter().map(|&n| Some(live.logic_of(n).unwrap_or(Logic::Z))).collect())
                    .filter(|_| live.logic.is_some())
                    .unwrap_or_default()
            } else if part.kind.is_custom() {
                // Eigene ICs: Pegel der angeschlossenen Digitalnetze
                live.nl.and_then(|nl| nl.pin_nets.get(&part.id)).map(|pn| pn.iter().map(|&n| live.logic_of(n)).collect()).unwrap_or_default()
            } else {
                Vec::new()
            };
            symbols::draw(&pen, &part.kind, Live { glow, pins, pressed: live.pressed.contains(&part.id) });
            if part.broken {
                let (x0, y0, x1, y1) = part.bbox();
                let rr = Rect::from_min_max(self.to_screen(r, x0, y0), self.to_screen(r, x1, y1)).shrink(s * 0.15);
                let st = Stroke::new(pw * 1.4, RED);
                painter.line_segment([rr.left_top(), rr.right_bottom()], st);
                painter.line_segment([rr.right_top(), rr.left_bottom()], st);
                painter.text(rr.center_bottom() + Vec2::new(0.0, 2.0), Align2::CENTER_TOP, "defekt", font.clone(), RED);
            }
            if matches!(part.kind, PartKind::Voltmeter | PartKind::Ammeter) {
                self.paint_meter(painter, r, part, vals.copied(), live.op.is_some(), s);
                continue;
            }
            if let PartKind::Port = part.kind {
                let (dx, dy) = rot_f(1.4, 0.0, part.rot);
                painter.text(origin + Vec2::new(dx, dy) * s, Align2::CENTER_CENTER, &part.name, font.clone(), ACCENT);
                continue;
            }
            if s >= 9.0 && !matches!(part.kind, PartKind::Ground | PartKind::LogicHigh) {
                if part.kind.is_ic() || part.kind.is_custom() {
                    // Name (und Typ) oben rechts am Gehäuse
                    let (lx, ly, with_type) = match part.kind.ic_sides() {
                        Some(sides) => {
                            let (_, y0, x1, _) = sides.body();
                            (x1 + 0.15, y0 - 0.1, false)
                        }
                        None if matches!(part.kind, PartKind::OpAmp { .. }) => (0.5, -1.25, true),
                        None => (0.4, if part.local_pins_len() == 4 { -1.6 } else { -2.1 }, true),
                    };
                    let (dx, dy) = rot_f(lx, ly, part.rot);
                    let t = if with_type { format!("{} · {}", part.name, part.kind.value_text()) } else { part.name.clone() };
                    painter.text(origin + Vec2::new(dx, dy) * s, Align2::LEFT_BOTTOM, t, font.clone(), Color32::from_rgb(228, 230, 236));
                } else if part.kind.is_digital() {
                    let (x0, y0, x1, _) = part.bbox();
                    let at = self.to_screen(r, (x0 + x1) * 0.5, y0) - Vec2::new(0.0, 2.0);
                    let v = part.kind.value_text();
                    let t = if v.is_empty() { part.name.clone() } else { format!("{} · {v}", part.name) };
                    painter.text(at, Align2::CENTER_BOTTOM, t, font.clone(), Color32::from_rgb(228, 230, 236));
                } else {
                    self.paint_label(painter, origin, part.rot, &part.kind, &part.name, s, &font);
                }
            }
        }

        // Werkzeug-Vorschau
        if let Some((kind, p)) = &ghost {
            // Grün, wenn das Bauteil in eine bestehende Leitung eingefügt wird
            let rot = doc.snap_rotation(kind, *p, self.place_rot);
            let probe = elab_app::model::Part { id: 0, name: String::new(), kind: kind.clone(), pos: *p, rot, broken: false };
            let inserts = doc.would_insert(kind, &probe.pins());
            let col = if inserts { GREEN } else { ACCENT.gamma_multiply(0.75) };
            let pen = Pen { painter, origin: self.sp(r, *p), s, rot, color: col, width: pw };
            if matches!(kind, PartKind::Probe) {
                self.paint_probe(painter, pen.origin, "M", None, col, true, s);
            } else {
                symbols::draw(&pen, kind, Live::default());
            }
            if inserts {
                let (x0, y0, _, _) = probe.bbox();
                painter.text(
                    self.to_screen(r, x0, y0) - Vec2::new(0.0, 4.0),
                    Align2::LEFT_BOTTOM,
                    "wird in die Leitung eingefügt",
                    FontId::proportional(12.0),
                    GREEN,
                );
            }
        }
        let preview = |a: P, b: P, hfirst: bool| {
            let c = Self::corner(a, b, hfirst);
            let col = ACCENT.gamma_multiply(0.8);
            painter.line_segment([self.sp(r, a), self.sp(r, c)], Stroke::new(width, col));
            painter.line_segment([self.sp(r, c), self.sp(r, b)], Stroke::new(width, col));
            painter.circle_filled(self.sp(r, b), width * 1.5, col);
        };
        if let (Tool::Wire, Some(b)) = (&self.tool, gs) {
            match self.wire_start {
                Some(a) => preview(a, b, self.wire_hfirst),
                None => {
                    painter.circle_stroke(self.sp(r, b), 5.0, Stroke::new(1.5, ACCENT));
                }
            }
        }
        if let (Drag::WireFromPin { from }, Some(b)) = (&self.drag, gs) {
            let d = b - *from;
            preview(*from, b, d.x.abs() >= d.y.abs());
        }
        if let (Drag::Box { from }, Some(to)) = (&self.drag, painter.ctx().input(|i| i.pointer.interact_pos())) {
            let rr = Rect::from_two_pos(*from, to);
            painter.rect_filled(rr, 2.0, ACCENT.gamma_multiply(0.07));
            painter.rect_stroke(rr, 2.0, Stroke::new(1.0, ACCENT.gamma_multiply(0.6)), StrokeKind::Inside);
        }
        // Anschluss bzw. Abzweigpunkt unter der Maus hervorheben
        if let Hover::Pin(p, ..) = self.hover {
            painter.circle_stroke(self.sp(r, p), (0.32 * s).max(5.0), Stroke::new(2.0, ACCENT));
        }
        if let (Hover::Wire(id), Tool::Select, Drag::None, Some(gp)) = (self.hover, &self.tool, &self.drag, gs) {
            if let Some(p) = Self::wire_point(doc, id, (gp.x as f32, gp.y as f32)) {
                painter.circle_stroke(self.sp(r, p), (0.25 * s).max(4.0), Stroke::new(1.8, ACCENT));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_label(&self, painter: &egui::Painter, origin: Pos2, rot: u8, kind: &PartKind, name: &str, s: f32, font: &FontId) {
        let (ax, ay) = match (kind.shape(), kind) {
            (Shape::Source, _) => (1.25, 0.0),
            (_, PartKind::Led { .. }) => (0.0, -1.5),
            (_, PartKind::Transistor { .. }) => (1.85, 0.0),
            (_, PartKind::Potentiometer { .. }) => (0.0, 1.05),
            (_, PartKind::PushButton) => (0.0, 0.75),
            _ => (0.0, -1.05),
        };
        let (dx, dy) = rot_f(ax, ay, rot);
        let at = origin + Vec2::new(dx, dy) * s;
        let value = kind.value_text();
        let name_col = Color32::from_rgb(228, 230, 236);
        let lh = font.size * 1.15;
        if dy < -0.1 {
            painter.text(at, Align2::CENTER_BOTTOM, &value, font.clone(), MUTED);
            painter.text(at - Vec2::new(0.0, lh), Align2::CENTER_BOTTOM, name, font.clone(), name_col);
        } else if dy > 0.1 {
            painter.text(at, Align2::CENTER_TOP, name, font.clone(), name_col);
            painter.text(at + Vec2::new(0.0, lh), Align2::CENTER_TOP, &value, font.clone(), MUTED);
        } else {
            let al = if dx > 0.0 { Align2::LEFT_BOTTOM } else { Align2::RIGHT_BOTTOM };
            let al2 = if dx > 0.0 { Align2::LEFT_TOP } else { Align2::RIGHT_TOP };
            painter.text(at, al, name, font.clone(), name_col);
            painter.text(at, al2, &value, font.clone(), MUTED);
        }
    }

    /// Anzeige eines Messgeräts: LCD-Kasten über dem Symbol.
    fn paint_meter(&self, painter: &egui::Painter, r: Rect, part: &elab_app::model::Part, v: Option<elab_app::sim::PartValues>, sim: bool, s: f32) {
        let volt = matches!(part.kind, PartKind::Voltmeter);
        let text = match (v, sim) {
            (Some(v), _) if volt => fmt_si(v.u, "V"),
            (Some(v), _) if v.i.abs() > 10.0 => "ÜBERLAST".to_string(),
            (Some(v), _) => fmt_si(v.i, "A"),
            (None, true) => "– – –".to_string(),
            (None, false) => "aus".to_string(),
        };
        let (x0, y0, x1, y1) = part.bbox();
        let fs = (s * 0.75).clamp(11.0, 22.0);
        let g = painter.layout_no_wrap(text, FontId::monospace(fs), Color32::from_rgb(140, 230, 255));
        let name = painter.layout_no_wrap(part.name.clone(), FontId::proportional((fs * 0.6).max(9.0)), MUTED);
        let w = g.size().x.max(name.size().x) + 14.0;
        let h = g.size().y + name.size().y + 8.0;
        // Über dem Symbol, bei senkrechter Lage rechts daneben
        let vertical = part.rot % 2 == 1;
        let rect = if vertical {
            let c = self.to_screen(r, x1, (y0 + y1) * 0.5);
            let right = Rect::from_min_size(c + Vec2::new(6.0, -h * 0.5), Vec2::new(w, h));
            if right.right() > r.right() - 4.0 {
                // Am rechten Rand: links neben das Symbol
                let c = self.to_screen(r, x0, (y0 + y1) * 0.5);
                Rect::from_min_size(c + Vec2::new(-6.0 - w, -h * 0.5), Vec2::new(w, h))
            } else {
                right
            }
        } else {
            let c = self.to_screen(r, (x0 + x1) * 0.5, y0);
            Rect::from_min_size(c + Vec2::new(-w * 0.5, -h - 4.0), Vec2::new(w, h))
        };
        painter.rect_filled(rect, 4.0, Color32::from_rgb(14, 26, 30));
        painter.rect_stroke(rect, 4.0, Stroke::new(1.2, Color32::from_rgb(60, 110, 125)), StrokeKind::Inside);
        let nh = name.size().y;
        painter.galley(rect.min + Vec2::new(7.0, 3.0), name, MUTED);
        painter.galley(rect.min + Vec2::new(7.0, 4.0 + nh), g, Color32::WHITE);
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_probe(&self, painter: &egui::Painter, pin: Pos2, name: &str, v: Option<String>, col: Color32, hi: bool, s: f32) {
        let corner = pin + Vec2::new(0.6, -0.6) * s;
        painter.line_segment([pin, corner], Stroke::new(1.6, col));
        painter.circle_filled(pin, (0.17 * s).max(3.0), col);
        let fs = (s * 0.6).clamp(10.0, 16.0);
        let text = match v {
            Some(v) => format!("{name}  {v}"),
            None => name.to_string(),
        };
        let galley = painter.layout_no_wrap(text, FontId::proportional(fs), Color32::WHITE);
        let pad = Vec2::new(5.0, 2.0);
        let rect = Rect::from_min_size(corner - Vec2::new(0.0, galley.size().y + 2.0 * pad.y), galley.size() + 2.0 * pad);
        painter.rect_filled(rect, 3.0, Color32::from_rgba_unmultiplied(20, 21, 26, 235));
        painter.rect_stroke(rect, 3.0, Stroke::new(if hi { 2.0 } else { 1.2 }, col), StrokeKind::Inside);
        painter.galley(rect.min + pad, galley, Color32::WHITE);
    }

    fn paint_overlay(&self, painter: &egui::Painter, r: Rect, doc: &Document, live: &LiveData, pointer: Option<Pos2>, gs: Option<P>) {
        // Hinweiszeile unten links
        let hint = match (&self.tool, self.wire_start) {
            (Tool::Select, _) => {
                "Bauteil ziehen: verschieben · Von Anschluss oder Leitung ziehen: Draht · R: drehen · Entf: löschen · Doppelklick: Schalter"
            }
            (Tool::Place(_), _) => "Klicken: absetzen (auf einer Leitung: einfügen) · ⇧-Klick: mehrere · R: drehen · Esc: abbrechen",
            (Tool::Wire, None) => "Klicken: Draht beginnen · Esc: Werkzeug beenden",
            (Tool::Wire, Some(_)) => "Klicken: Ecke setzen · Leertaste: Knick wechseln · Doppelklick/Esc: beenden",
        };
        let pos = gs.map(|p| format!("   ({}, {})", p.x, p.y)).unwrap_or_default();
        painter.text(
            r.left_bottom() + Vec2::new(10.0, -8.0),
            Align2::LEFT_BOTTOM,
            format!("{hint}   ·   {:.0} %{pos}", self.zoom * 100.0),
            FontId::proportional(12.0),
            MUTED,
        );

        // Farblegende unten rechts
        if let Some(op) = live.op {
            let vmax = op.max_abs_v();
            if vmax > 0.0 {
                let w = 160.0;
                let base = r.right_top() + Vec2::new(-w - 16.0, 26.0);
                for k in 0..=40 {
                    let t = k as f64 / 40.0 * 2.0 - 1.0;
                    let x = base.x + w * k as f32 / 40.0;
                    painter.line_segment([Pos2::new(x, base.y), Pos2::new(x, base.y + 8.0)], Stroke::new(w / 40.0 + 0.6, volt_color(t * vmax, vmax)));
                }
                let f = FontId::proportional(11.0);
                painter.text(base + Vec2::new(0.0, -3.0), Align2::LEFT_BOTTOM, fmt_si(-vmax, "V"), f.clone(), MUTED);
                painter.text(base + Vec2::new(w * 0.5, -3.0), Align2::CENTER_BOTTOM, "0 V", f.clone(), MUTED);
                painter.text(base + Vec2::new(w, -3.0), Align2::RIGHT_BOTTOM, fmt_si(vmax, "V"), f, MUTED);
            }
        }

        // Infofenster am Mauszeiger
        let quiet = matches!(self.tool, Tool::Place(_)) || self.palette_drag.is_some();
        let (Some(p), true, false) = (pointer, matches!(self.drag, Drag::None), quiet) else {
            return;
        };
        let mut lines: Vec<(String, Color32, bool)> = Vec::new();
        let net_v = |net: usize| live.op.and_then(|o| o.net_v.get(net).copied().flatten());
        match self.hover {
            Hover::Part(id) | Hover::Pin(_, id, _) => {
                let Some(part) = doc.part(id) else { return };
                lines.push((format!("{} · {}", part.name, part.kind.label()), Color32::WHITE, true));
                if part.kind.has_value() {
                    lines.push((part.kind.value_text(), MUTED, false));
                }
                if part.broken {
                    lines.push(("Durchgebrannt: leitet nicht mehr. Im Inspektor ersetzen.".into(), RED, false));
                } else if let Some(&h) = live.heat.get(&id).filter(|h| **h > 0.3) {
                    let col = if h > 0.8 { RED } else { Color32::from_rgb(235, 200, 70) };
                    lines.push((format!("Belastung {:.0} % des Grenzwerts{}", h * 100.0, if h > 0.8 { " – wird heiß!" } else { "" }), col, false));
                }
                if let Hover::Pin(_, _, k) = self.hover {
                    let pname = part.kind.pin_names().get(k).cloned().unwrap_or_else(|| "?".into());
                    let mut l = if part.kind.local_pins().len() == 1 { "Anschluss".to_string() } else { format!("Anschluss {pname}") };
                    if let Some(nl) = live.nl {
                        let Some(&n) = nl.pin_nets.get(&id).and_then(|v| v.get(k)) else { return };
                        l += &format!(" · Netz {}", nl.nets[n].name);
                        if let Some(v) = net_v(n) {
                            l += &format!(" · {}", fmt_si(v, "V"));
                        }
                    }
                    lines.push((l, BLUE, false));
                    if self.tool == Tool::Select {
                        lines.push(("Ziehen, um einen Draht zu verlegen".into(), MUTED, false));
                    }
                } else if let (Some(v), PartKind::Transistor { kind }) = (live.op.and_then(|o| o.parts.get(&id)), &part.kind) {
                    let bjt = kind.is_bjt();
                    let pnp = matches!(kind.family(), elab_app::model::TransistorFamily::Pnp | elab_app::model::TransistorFamily::Pmos);
                    let (uo, uc, io, ic) = match (bjt, pnp) {
                        (true, false) => ("U_CE", "U_BE", "I_C", "I_B"),
                        (true, true) => ("U_EC", "U_EB", "I_C", "I_B"),
                        (false, false) => ("U_DS", "U_GS", "I_D", ""),
                        (false, true) => ("U_SD", "U_SG", "I_D", ""),
                    };
                    lines.push((format!("{uc} = {}   {uo} = {}", fmt_si(v.ctrl_u, "V"), fmt_si(v.u, "V")), ACCENT, false));
                    if bjt {
                        lines.push((format!("{ic} = {}   {io} = {}", fmt_si(v.ctrl_i, "A"), fmt_si(v.i, "A")), BLUE, false));
                        if v.ctrl_i.abs() > 1e-12 && v.i.abs() > 1e-12 {
                            lines.push((format!("I_C / I_B = {:.0}", v.i / v.ctrl_i), MUTED, false));
                        }
                    } else {
                        lines.push((format!("{io} = {}   (Gate stromlos)", fmt_si(v.i, "A")), BLUE, false));
                    }
                    lines.push((format!("P = {} (Verlustleistung)", fmt_si(v.p, "W")), GREEN, false));
                    if let Some(reg) = v.region {
                        lines.push((reg.text(bjt).to_string(), Color32::WHITE, false));
                    }
                } else if let (Some(v), PartKind::Potentiometer { .. }) = (live.op.and_then(|o| o.parts.get(&id)), &part.kind) {
                    lines.push((format!("U_AB = {}   U_SB = {}", fmt_si(v.u, "V"), fmt_si(v.ctrl_u, "V")), ACCENT, false));
                    lines.push((format!("I_A = {}   I_Schleifer = {}", fmt_si(v.i, "A"), fmt_si(v.ctrl_i, "A")), BLUE, false));
                    lines.push(("Wenn ausgewählt: ← / → verstellt den Schleifer".into(), MUTED, false));
                } else if let (Some(v), PartKind::OpAmp { .. }) = (live.op.and_then(|o| o.parts.get(&id)), &part.kind) {
                    lines.push((format!("U_aus = {}   U₊ − U₋ = {}", fmt_si(v.u, "V"), fmt_si(v.ctrl_u, "V")), ACCENT, false));
                    lines.push((format!("I_aus = {}", fmt_si(v.i, "A")), BLUE, false));
                    let state = if v.ctrl_u.abs() < 1e-3 { "regelt: U₊ ≈ U₋ (Gegenkopplung)" } else { "übersteuert: Ausgang an der Grenze" };
                    lines.push((state.into(), Color32::WHITE, false));
                } else if let (Some(v), true) = (live.op.and_then(|o| o.parts.get(&id)), part.kind.is_ic()) {
                    lines.push((format!("U_VCC = {}   I_VCC = {}", fmt_si(v.u, "V"), fmt_si(v.i, "A")), ACCENT, false));
                    lines.push((format!("P = {} (Verlustleistung)", fmt_si(v.p, "W")), GREEN, false));
                    if let Some(pins) = live.pin_logic.and_then(|m| m.get(&id)) {
                        let names = part.kind.pin_names();
                        let vals: Vec<String> = pins
                            .iter()
                            .enumerate()
                            .filter_map(|(k, v)| v.map(|v| format!("{} = {}", names.get(k).map_or("?", |s| s.as_str()), v.symbol())))
                            .collect();
                        lines.push((vals.join("  ·  "), symbols::logic_color(Logic::L1), false));
                        if pins.iter().flatten().any(|v| *v == Logic::X) {
                            lines.push(("X: Spannung zwischen den Schwellen oder Eingang offen".into(), symbols::logic_color(Logic::X), false));
                        }
                    }
                } else if let (Some(v), PartKind::Voltmeter) = (live.op.and_then(|o| o.parts.get(&id)), &part.kind) {
                    lines.push((format!("U = {} (+ gegen −)", fmt_si(v.u, "V")), ACCENT, false));
                    lines.push(("Misst parallel, belastet die Schaltung nicht".into(), MUTED, false));
                } else if let (Some(v), PartKind::Ammeter) = (live.op.and_then(|o| o.parts.get(&id)), &part.kind) {
                    lines.push((format!("I = {} (von + nach −)", fmt_si(v.i, "A")), BLUE, false));
                    lines.push(("Misst in Reihe, Innenwiderstand 0 Ω".into(), MUTED, false));
                } else if let Some(v) = live.op.and_then(|o| o.parts.get(&id)) {
                    lines.push((format!("U = {}", fmt_si(v.u, "V")), ACCENT, false));
                    lines.push((format!("I = {}", fmt_si(v.i, "A")), BLUE, false));
                    let pl = if matches!(part.kind.shape(), Shape::Source) { "abgegeben" } else { "aufgenommen" };
                    lines.push((format!("P = {} ({pl})", fmt_si(v.p, "W")), GREEN, false));
                    match part.kind {
                        PartKind::Diode { .. } | PartKind::Led { .. } => {
                            let state = if v.i > 1e-4 {
                                "leitet"
                            } else if v.u.abs() < 1e-3 {
                                "stromlos"
                            } else if v.u < 0.0 {
                                "sperrt"
                            } else {
                                "leitet kaum"
                            };
                            lines.push((format!("Diode {state}"), MUTED, false));
                        }
                        PartKind::Resistor { .. } if v.i.abs() > 1e-12 => {
                            lines.push((format!("U / I = {}", fmt_si(v.u / v.i, "Ω")), MUTED, false));
                        }
                        _ => {}
                    }
                } else if live.nl.is_some_and(|nl| nl.excluded.contains(&id)) {
                    lines.push(("Nicht mit der Masse verbunden: wird nicht simuliert".into(), RED, false));
                }
                if live.nl.is_some_and(|nl| nl.bridged.contains(&id)) {
                    lines.push((
                        "Überbrückt: Beide Anschlüsse hängen am selben Netz, es fließt kein Strom.".into(),
                        Color32::from_rgb(235, 200, 70),
                        false,
                    ));
                }
                if let PartKind::Probe = part.kind {
                    if let Some(nl) = live.nl {
                        let Some(&n) = nl.pin_nets.get(&id).and_then(|v| v.first()) else { return };
                        if let Some(v) = net_v(n) {
                            lines.push((format!("U = {} gegen Masse", fmt_si(v, "V")), ACCENT, false));
                        }
                    }
                }
                if let PartKind::Switch { .. } = part.kind {
                    lines.push(("Doppelklick: umschalten".into(), MUTED, false));
                }
                if part.kind.is_digital() {
                    if let (Some(nl), Some(_)) = (live.nl, live.logic) {
                        let names = part.kind.pin_names();
                        let Some(pn) = nl.pin_nets.get(&id) else { return };
                        let vals: Vec<String> = pn
                            .iter()
                            .enumerate()
                            .map(|(k, &n)| format!("{} = {}", names.get(k).map_or("?", |s| s.as_str()), live.logic_of(n).map_or("–", |l| l.symbol())))
                            .collect();
                        lines.push((vals.join("  ·  "), symbols::logic_color(Logic::L1), false));
                    }
                    match part.kind {
                        PartKind::LogicInput { .. } => lines.push(("Klick: umschalten".into(), MUTED, false)),
                        PartKind::Button => lines.push(("Gedrückt halten: 1".into(), MUTED, false)),
                        _ => {}
                    }
                }
            }
            Hover::Wire(id) => {
                let Some(nl) = live.nl else { return };
                let Some(&n) = nl.wire_net.get(&id) else { return };
                lines.push((format!("Netz {}", nl.nets[n].name), Color32::WHITE, true));
                if let Some(l) = live.logic_of(n) {
                    let what = match l {
                        Logic::L1 => "Pegel 1 (High)",
                        Logic::L0 => "Pegel 0 (Low)",
                        Logic::X => "unbestimmt (X): Konflikt oder offener Eingang",
                        Logic::Z => "hochohmig (Z): kein Ausgang treibt das Netz",
                    };
                    lines.push((what.into(), symbols::logic_color(l), false));
                }
                match net_v(n) {
                    Some(v) => lines.push((format!("U = {} gegen Masse", fmt_si(v, "V")), ACCENT, false)),
                    None if !nl.nets[n].reachable && live.logic_of(n).is_none() => lines.push(("Keine Verbindung zur Masse".into(), RED, false)),
                    None => {}
                }
            }
            Hover::None => return,
        }
        info_box(painter, r, p + Vec2::new(16.0, 14.0), &lines);
    }
}

/// Kleines Infofenster aus mehreren Zeilen.
pub fn info_box(painter: &egui::Painter, clip: Rect, at: Pos2, lines: &[(String, Color32, bool)]) {
    if lines.is_empty() {
        return;
    }
    let galleys: Vec<_> =
        lines.iter().map(|(t, c, strong)| painter.layout_no_wrap(t.clone(), FontId::proportional(if *strong { 13.5 } else { 12.5 }), *c)).collect();
    let w = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max);
    let h: f32 = galleys.iter().map(|g| g.size().y + 2.0).sum();
    let pad = 8.0;
    let mut rect = Rect::from_min_size(at, Vec2::new(w + 2.0 * pad, h + 2.0 * pad - 2.0));
    if rect.right() > clip.right() - 4.0 {
        rect = rect.translate(Vec2::new(clip.right() - 4.0 - rect.right(), 0.0));
    }
    if rect.bottom() > clip.bottom() - 4.0 {
        rect = rect.translate(Vec2::new(0.0, -(rect.height() + 28.0)));
    }
    painter.rect_filled(rect, 5.0, Color32::from_rgba_unmultiplied(30, 32, 38, 242));
    painter.rect_stroke(rect, 5.0, Stroke::new(1.0, Color32::from_gray(70)), StrokeKind::Inside);
    let mut y = rect.top() + pad;
    for g in galleys {
        let hgt = g.size().y;
        painter.galley(Pos2::new(rect.left() + pad, y), g, Color32::WHITE);
        y += hgt + 2.0;
    }
}
