//! Protokoll für die Entwicklungsphase: ~/.elab/logs/elab.log
//!
//! Neben Start, Fehlern und Abstürzen fängt ein Wächter Hänger ein: Braucht ein Bild länger
//! als zwei Sekunden, schreibt er, womit die App gerade beschäftigt ist, und legt den
//! aktuellen Schaltplan als `haenger-….elab` daneben. So lässt sich ein Einfrieren auch
//! ohne Debugger nachvollziehen.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static ACTIVITY: Mutex<&'static str> = Mutex::new("Start");
static DOC: Mutex<Option<String>> = Mutex::new(None);
/// Beginn des laufenden Bildes in ms seit Programmstart, 0 = kein Bild in Arbeit
static FRAME_START: AtomicU64 = AtomicU64::new(0);
static ENABLED: AtomicBool = AtomicBool::new(false);
static START: Mutex<Option<Instant>> = Mutex::new(None);

pub fn dir() -> PathBuf {
    super::data_dir().join("logs")
}

fn path() -> PathBuf {
    dir().join("elab.log")
}

fn now_ms() -> u64 {
    let start = *START.lock().unwrap();
    start.map_or(0, |s| s.elapsed().as_millis() as u64 + 1)
}

/// Zeitstempel in UTC (ohne Zusatzbibliothek)
fn stamp() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86400, secs % 86400);
    // Kalenderdatum aus Tagen seit 1970 (Algorithmus von H. Hinnant)
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", rem / 3600, rem / 60 % 60, rem % 60)
}

/// Schreibt eine Zeile ins Protokoll.
pub fn write(msg: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let _ = std::fs::create_dir_all(dir());
    // Nicht endlos wachsen lassen
    if std::fs::metadata(path()).is_ok_and(|m| m.len() > 5_000_000) {
        let _ = std::fs::rename(path(), dir().join("elab.alt.log"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path()) {
        let _ = writeln!(f, "{} {msg}", stamp());
    }
}

/// Startet das Protokoll und den Hänger-Wächter.
pub fn init() {
    *START.lock().unwrap() = Some(Instant::now());
    ENABLED.store(true, Ordering::Relaxed);
    write(&format!("elab {} gestartet ({} {})", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH));
    std::thread::Builder::new()
        .name("elab-waechter".into())
        .spawn(|| {
            let mut reported = 0u64;
            loop {
                std::thread::sleep(Duration::from_millis(250));
                let start = FRAME_START.load(Ordering::Relaxed);
                if start == 0 {
                    if reported != 0 {
                        reported = 0;
                    }
                    continue;
                }
                let busy = now_ms().saturating_sub(start);
                if busy > 2000 && reported != start {
                    reported = start;
                    let what = *ACTIVITY.lock().unwrap();
                    let file = dir().join(format!("haenger-{}.elab", SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())));
                    if let Some(doc) = DOC.lock().unwrap().clone() {
                        let _ = std::fs::write(&file, doc);
                    }
                    write(&format!("HÄNGER: Bild läuft seit {:.1} s, beschäftigt mit: {what}. Schaltplan: {}", busy as f64 / 1000.0, file.display()));
                } else if busy > 2000 && busy % 10_000 < 250 {
                    let what = *ACTIVITY.lock().unwrap();
                    write(&format!("… hängt weiter ({:.0} s), beschäftigt mit: {what}", busy as f64 / 1000.0));
                }
            }
        })
        .ok();
}

/// Womit ist die App gerade beschäftigt? (erscheint im Protokoll, falls es hängt)
pub fn activity(what: &'static str) {
    *ACTIVITY.lock().unwrap() = what;
}

pub fn frame_start() {
    FRAME_START.store(now_ms(), Ordering::Relaxed);
    activity("Bildaufbau");
}

pub fn frame_end() {
    let start = FRAME_START.swap(0, Ordering::Relaxed);
    let took = now_ms().saturating_sub(start);
    if start != 0 && took > 2000 {
        write(&format!("Hänger vorbei nach {:.1} s", took as f64 / 1000.0));
    } else if start != 0 && took > 500 {
        write(&format!("Langsames Bild: {took} ms ({})", *ACTIVITY.lock().unwrap()));
    }
}

/// Merkt sich den Schaltplan (als JSON) für einen möglichen Hängerbericht.
pub fn set_doc(json: String) {
    *DOC.lock().unwrap() = Some(json);
}

/// Inhalt des Protokolls für Absturzberichte: der Schaltplan zum Zeitpunkt des Absturzes.
pub fn last_doc() -> Option<String> {
    DOC.lock().ok()?.clone()
}
