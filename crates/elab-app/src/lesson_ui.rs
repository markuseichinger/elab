//! Geführte Übungen in der Oberfläche: Karte im Tutorial, Fortschritt, Markierungen.

use crate::theme::*;
use crate::tutorial::Topic;
use crate::App;
use egui::{Color32, RichText};
use elab_app::lessons::{self, Ctx, Lesson, Progress};
use elab_app::model::{Document, GateType, LedColor, LogicFamily, OpAmpType, PartKind, TransistorType, P};

/// Eine laufende Übung.
pub struct LessonRun {
    pub lesson: Lesson,
    pub anchor: u32,
    pub progress: Progress,
    /// Ergebnis der letzten Prüfung: erledigt je Schritt, aktueller Schritt
    pub done: Vec<bool>,
    pub current: Option<usize>,
    /// Abschluss wurde schon gemeldet
    pub cheered: bool,
    /// Aufgaben: Schritte, deren Tipp aufgeklappt ist
    pub tips: std::collections::HashSet<usize>,
}

/// Bauteil, dessen Übung zu einem Tutorial-Thema passt.
pub fn topic_kind(t: Topic) -> Option<PartKind> {
    Some(match t {
        Topic::Led => PartKind::Led { color: LedColor::Rot },
        Topic::Capacitor => PartKind::Capacitor { c: 100e-9 },
        Topic::Bjt | Topic::TransistorSwitch => PartKind::Transistor { kind: TransistorType::Bc547 },
        Topic::Mosfet => PartKind::Transistor { kind: TransistorType::N2n7000 },
        Topic::RealIc => PartKind::GateIc { gate: GateType::Nand, family: LogicFamily::Hc },
        Topic::Timer555 => PartKind::Timer555,
        Topic::OpAmp => PartKind::OpAmp { kind: OpAmpType::Lm358 },
        _ => return None,
    })
}

fn title_of(k: &PartKind) -> Option<String> {
    lessons::lesson_for(k).map(|l| l.title)
}

impl App {
    /// Startet die Übung zu `kind`: am ausgewählten oder vorhandenen passenden Bauteil,
    /// sonst auf einem neuen Blatt (Rückgängig holt den alten Schaltplan zurück).
    pub(crate) fn start_lesson(&mut self, kind: &PartKind) {
        let Some(lesson) = lessons::lesson_for(kind) else { return };
        let same = |k: &PartKind| title_of(k).as_deref() == Some(lesson.title.as_str());
        let selected = self.ed.single_selected().filter(|id| self.doc.part(*id).is_some_and(|p| same(&p.kind)));
        let anchor = selected.or_else(|| self.doc.parts.iter().find(|p| same(&p.kind)).map(|p| p.id));
        let anchor = match anchor {
            Some(a) => a,
            None => {
                let mut d = Document { title: lesson.title.clone(), ..Default::default() };
                let id = d.add_part(lesson.start_kind.clone(), P::new(12, 0), 0);
                self.replace_doc(d);
                self.flash("Neues Blatt für die Übung. Rückgängig (⌘Z) holt den alten Schaltplan zurück.".into(), BLUE);
                id
            }
        };
        self.ed.select_only(anchor);
        self.show_tutorial = true;
        self.lesson = Some(LessonRun {
            lesson,
            anchor,
            progress: Progress::default(),
            done: Vec::new(),
            current: Some(0),
            cheered: false,
            tips: Default::default(),
        });
    }

    /// Startet eine Aufgabe auf einem neuen Blatt mit ihrem Startbauteil.
    pub(crate) fn start_task(&mut self, lesson: Lesson) {
        let mut d = Document { title: lesson.title.clone(), ..Default::default() };
        let anchor = d.add_part(lesson.start_kind.clone(), P::new(12, 0), 0);
        self.replace_doc(d);
        self.flash("Neues Blatt für die Aufgabe. Rückgängig (⌘Z) holt den alten Schaltplan zurück.".into(), BLUE);
        self.ed.select_only(anchor);
        self.show_tutorial = true;
        self.topic = Topic::Tasks;
        self.lesson = Some(LessonRun {
            lesson,
            anchor,
            progress: Progress::default(),
            done: Vec::new(),
            current: Some(0),
            cheered: false,
            tips: Default::default(),
        });
    }

    /// Prüft die laufende Übung gegen den aktuellen Stand (jeden Frame).
    pub(crate) fn lesson_tick(&mut self) {
        let Some(run) = &mut self.lesson else { return };
        if self.doc.part(run.anchor).is_none() {
            let t = run.lesson.title.clone();
            let what = if run.lesson.task { "Aufgabe" } else { "Übung" };
            self.lesson = None;
            self.flash(format!("{what} „{t}“ beendet: Das vorgegebene Bauteil wurde gelöscht."), ACCENT);
            return;
        }
        let Some(nl) = &self.nl else { return };
        let ctx = Ctx {
            doc: &self.doc,
            nl,
            op: self.op.as_ref().ok(),
            anchor: run.anchor,
            scope_open: self.show_scope && self.bottom_tab == crate::panels::BottomTab::Scope,
        };
        let (done, current) = run.progress.update(&run.lesson, &ctx);
        run.done = done;
        run.current = current;
        if current.is_none() && !run.cheered {
            run.cheered = true;
            let t = run.lesson.title.clone();
            let what = if run.lesson.task { "Aufgabe" } else { "Übung" };
            self.flash(format!("🎉 {what} „{t}“ geschafft!"), GREEN);
        }
    }

    /// Rasterpunkte, die gerade markiert werden.
    pub(crate) fn lesson_highlight(&self) -> Vec<P> {
        let Some(run) = &self.lesson else { return Vec::new() };
        let (Some(cur), Some(part)) = (run.current, self.doc.part(run.anchor)) else { return Vec::new() };
        let pins = part.pins();
        run.lesson.steps[cur].pins.iter().filter_map(|&k| pins.get(k).copied()).collect()
    }

    /// Karte im Tutorial: laufende Übung oder Angebot zum aktuellen Thema.
    pub(crate) fn lesson_card(&mut self, ui: &mut egui::Ui) {
        if self.lesson.is_some() {
            self.lesson_running(ui);
            return;
        }
        // Angebot: passendes Bauteil ausgewählt oder Thema mit Übung
        let kind = self
            .ed
            .single_selected()
            .and_then(|id| self.doc.part(id))
            .map(|p| p.kind.clone())
            .filter(lessons::has_lesson)
            .or_else(|| topic_kind(self.topic));
        let Some(kind) = kind else { return };
        let Some(lesson) = lessons::lesson_for(&kind) else { return };
        egui::Frame::new().fill(Color32::from_rgb(28, 36, 30)).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(format!("🧩 Geführte Übung: {}", lesson.title)).strong().color(GREEN));
            ui.label(RichText::new(&lesson.intro).size(13.0));
            if ui.button(RichText::new("▶ Übung starten").strong()).on_hover_text("Schritt für Schritt aufbauen, elab prüft jeden Schritt").clicked()
            {
                self.start_lesson(&kind);
            }
        });
        ui.add_space(6.0);
    }

    fn lesson_running(&mut self, ui: &mut egui::Ui) {
        let mut stop = false;
        let mut skip = None;
        let mut restart = false;
        let mut tip = None;
        {
            let run = self.lesson.as_ref().unwrap();
            let doc = &self.doc;
            let nl = self.nl.as_ref();
            let n = run.lesson.steps.len();
            egui::Frame::new()
                .fill(Color32::from_rgb(30, 34, 26))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(90, 120, 60)))
                .corner_radius(6.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let icon = if run.lesson.task { "🎯" } else { "🧩" };
                        ui.label(RichText::new(format!("{icon} {}", run.lesson.title)).size(16.0).strong().color(GREEN));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕ Beenden").clicked() {
                                stop = true;
                            }
                        });
                    });
                    if run.lesson.task {
                        ui.label(RichText::new(&run.lesson.intro).size(13.0));
                    }
                    let done_n = run.done.iter().filter(|d| **d).count();
                    ui.add(egui::ProgressBar::new(done_n as f32 / n.max(1) as f32).desired_height(6.0));
                    ui.add_space(4.0);
                    for (i, s) in run.lesson.steps.iter().enumerate() {
                        let is_cur = run.current == Some(i);
                        let ok = run.done.get(i).copied().unwrap_or(false);
                        let (icon, col) = if ok {
                            ("✔", GREEN)
                        } else if is_cur {
                            ("▶", ACCENT)
                        } else {
                            ("○", MUTED)
                        };
                        ui.label(RichText::new(format!("{icon} {}. {}", i + 1, s.title)).color(col).strong());
                        if is_cur {
                            egui::Frame::new().inner_margin(egui::Margin { left: 18, right: 0, top: 2, bottom: 4 }).show(ui, |ui| {
                                ui.label(RichText::new(&s.text).size(14.0));
                                if !run.lesson.task {
                                    ui.label(RichText::new(format!("Warum? {}", s.why)).size(12.5).color(Color32::from_rgb(170, 176, 190)));
                                } else if run.tips.contains(&i) {
                                    ui.label(RichText::new(format!("💡 {}", s.why)).size(12.5).color(Color32::from_rgb(170, 176, 190)));
                                }
                                if let Some(nl) = nl {
                                    let ctx = Ctx { doc, nl, op: self.op.as_ref().ok(), anchor: run.anchor, scope_open: self.show_scope };
                                    if let Some(h) = (s.hint)(&ctx) {
                                        ui.label(RichText::new(format!("⚠ {h}")).color(RED));
                                    }
                                    if let Some(l) = (s.live)(&ctx) {
                                        ui.label(RichText::new(l).monospace().color(ACCENT));
                                    }
                                }
                                if !s.pins.is_empty() {
                                    ui.label(RichText::new("Die betroffenen Anschlüsse blinken im Schaltplan.").small().color(MUTED));
                                }
                                if run.lesson.task {
                                    if !run.tips.contains(&i) && ui.small_button("💡 Tipp").clicked() {
                                        tip = Some(i);
                                    }
                                } else if ui.small_button("Überspringen ▷").on_hover_text("Diesen Schritt als erledigt markieren").clicked() {
                                    skip = Some(i);
                                }
                            });
                        } else if ok {
                            // erledigte Schritte: Messwerte weiter anzeigen
                            if let Some(nl) = nl {
                                let ctx = Ctx { doc, nl, op: self.op.as_ref().ok(), anchor: run.anchor, scope_open: self.show_scope };
                                if let Some(l) = (s.live)(&ctx) {
                                    ui.label(RichText::new(format!("    {l}")).small().monospace().color(MUTED));
                                }
                            }
                        }
                    }
                    if run.current.is_none() {
                        ui.add_space(4.0);
                        ui.label(RichText::new("🎉 Geschafft!").size(16.0).strong().color(GREEN));
                        ui.label(&run.lesson.outro);
                        ui.horizontal(|ui| {
                            if ui.button("↺ Nochmal von vorn").clicked() {
                                restart = true;
                            }
                            if ui.button("Schließen").clicked() {
                                stop = true;
                            }
                        });
                    }
                });
        }
        if let (Some(i), Some(run)) = (tip, &mut self.lesson) {
            run.tips.insert(i);
        }
        if let Some(i) = skip {
            if let Some(run) = &mut self.lesson {
                run.progress.skipped.insert(i);
            }
        }
        if restart {
            if let Some(run) = &mut self.lesson {
                run.progress = Progress::default();
                run.cheered = false;
            }
        }
        if stop {
            self.lesson = None;
        }
        ui.add_space(6.0);
    }

    /// Hinweis nach dem Setzen eines Bauteils mit Übung (einmal je Übung).
    pub(crate) fn lesson_placed(&mut self, id: u32) {
        if self.lesson.is_some() {
            return;
        }
        let Some(t) = self.doc.part(id).and_then(|p| title_of(&p.kind)) else { return };
        if self.lesson_hinted.insert(t.clone()) {
            self.show_tutorial = true;
            self.flash(format!("Tipp: Zu diesem Bauteil gibt es die geführte Übung „{t}“ – rechts im Tutorial auf ▶ Übung starten."), GREEN);
        }
    }
}
