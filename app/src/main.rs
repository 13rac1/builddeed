//! BuildDeed Checksum: a file checksum tool, sold as shareware with a
//! BuildDeed license on NEAR.
//!
//! - Drop a file: the app shows its SHA-256, and the official build that
//!   contains the file, if the license contract has a record of it.
//! - Drop a folder: the folder check compares every file with the last check
//!   of that folder. This feature needs a license.
//!
//! The license check is in the source code, and anyone may build the app
//! without it (see LICENSE). The official build is the one that the license
//! contract records.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod folder;
mod near;
mod signin;
mod store;

use eframe::egui::{self, Color32, RichText};
use near::{ArtifactMatch, Near};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use store::Registration;

const PRODUCT: &str = "BuildDeed Checksum";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const CONTRACT: &str = "demo.builddeed.testnet";
const RPC_URL: &str = "https://rpc.testnet.fastnear.com";
const SITE_URL: &str = "https://13rac1.github.io/builddeed";
const EXPLORER: &str = "https://testnet.nearblocks.io/address";
const REG_FILE: &str = "registration.json";
const NAG_WAIT: Duration = Duration::from_secs(5);

enum Msg {
    Check(Result<Option<Registration>, String>),
    SignIn(Result<Registration, String>),
    File {
        path: PathBuf,
        sha256: Result<String, String>,
        record: Result<Option<ArtifactMatch>, String>,
    },
    Folder(Result<(String, PathBuf), String>),
    SelfCheck(String),
}

#[derive(PartialEq)]
enum State {
    Checking,
    Registered,
    Unregistered,
}

struct App {
    near: Arc<Near>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    state: State,
    reg: Option<Registration>,
    nag_open: bool,
    nag_since: Instant,
    signing_in: bool,
    message: String,
    file: Option<(PathBuf, Result<String, String>, Result<Option<ArtifactMatch>, String>)>,
    folder: Option<Result<(String, PathBuf), String>>,
    busy: bool,
    self_check: String,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        cc.egui_ctx.set_zoom_factor(1.1);
        let (tx, rx) = channel();
        let near = Arc::new(Near { rpc_url: RPC_URL.into(), contract: CONTRACT.into() });
        let reg: Option<Registration> = store::load(REG_FILE);
        let mut app = Self {
            near,
            tx,
            rx,
            state: State::Checking,
            reg: reg.clone(),
            nag_open: true,
            nag_since: Instant::now(),
            signing_in: false,
            message: String::new(),
            file: None,
            folder: None,
            busy: false,
            self_check: "Checking this program...".into(),
        };
        match reg {
            Some(r) => app.spawn_recheck(r, cc.egui_ctx.clone()),
            None => app.state = State::Unregistered,
        }
        app.spawn_self_check(cc.egui_ctx.clone());
        app
    }

    /// Checks a stored registration again: a license that was sold stops
    /// working at the next start.
    fn spawn_recheck(&self, r: Registration, ctx: egui::Context) {
        let near = self.near.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = license_for(&near, &r.account_id, &r.public_key).map(|found| {
                found.map(|mut fresh| {
                    fresh.checked_at_ms = store::now_ms();
                    fresh
                })
            });
            let result = match result {
                Err(e) if store::now_ms().saturating_sub(r.checked_at_ms) < store::OFFLINE_GRACE_MS => {
                    // No network: keep the stored result for the grace period.
                    eprintln!("license check failed, using the stored result: {e}");
                    Ok(Some(r))
                }
                other => other,
            };
            let _ = tx.send(Msg::Check(result));
            ctx.request_repaint();
        });
    }

    fn spawn_self_check(&self, ctx: egui::Context) {
        let near = self.near.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let text = match std::env::current_exe().and_then(|p| folder::sha256_file(&p)) {
                Err(e) => format!("Cannot read this program: {e}"),
                Ok(sha) => match near.find_artifact(&sha) {
                    Ok(Some(m)) => format!(
                        "Official build {} (commit {}).\nSHA-256 {sha}",
                        m.version,
                        &m.commit[..12]
                    ),
                    Ok(None) => format!(
                        "Not an official build: the license contract has no record of this program.\nSHA-256 {sha}"
                    ),
                    Err(e) => format!("Cannot check this program now: {e}\nSHA-256 {sha}"),
                },
            };
            let _ = tx.send(Msg::SelfCheck(text));
            ctx.request_repaint();
        });
    }

    fn start_sign_in(&mut self, ctx: &egui::Context) {
        let pending = match signin::start(SITE_URL, PRODUCT) {
            Ok(p) => p,
            Err(e) => {
                self.message = e;
                return;
            }
        };
        open_url(&pending.sign_url);
        self.signing_in = true;
        self.message = "Sign the message in your wallet. The browser sends the result back to this app.".into();
        let near = self.near.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = pending.finish(&near).and_then(|s| {
                license_for(&near, &s.account_id, &s.public_key)?.ok_or(format!(
                    "{} has no license that covers version {VERSION}. Buy one, then try again.",
                    s.account_id
                ))
            });
            let _ = tx.send(Msg::SignIn(result));
            ctx.request_repaint();
        });
    }

    fn on_drop(&mut self, path: PathBuf, ctx: &egui::Context) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        if path.is_dir() {
            if self.state != State::Registered {
                self.message = "The folder check needs a license. Register to use it.".into();
                self.nag_open = true;
                return;
            }
            self.busy = true;
            std::thread::spawn(move || {
                let _ = tx.send(Msg::Folder(check_folder(&path)));
                ctx.request_repaint();
            });
        } else {
            self.busy = true;
            let near = self.near.clone();
            std::thread::spawn(move || {
                let sha256 = folder::sha256_file(&path).map_err(|e| e.to_string());
                let record = match &sha256 {
                    Ok(s) => near.find_artifact(s),
                    Err(_) => Ok(None),
                };
                let _ = tx.send(Msg::File { path, sha256, record });
                ctx.request_repaint();
            });
        }
    }

    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Check(Ok(Some(r))) => {
                    let _ = store::save(REG_FILE, &r);
                    self.reg = Some(r);
                    self.state = State::Registered;
                    self.nag_open = false;
                }
                Msg::Check(Ok(None)) => {
                    store::remove(REG_FILE);
                    self.reg = None;
                    self.state = State::Unregistered;
                    self.message = "The stored license is no longer valid for this version.".into();
                }
                Msg::Check(Err(e)) => {
                    self.state = State::Unregistered;
                    self.message = format!("Cannot check the license: {e}");
                }
                Msg::SignIn(Ok(r)) => {
                    self.signing_in = false;
                    let _ = store::save(REG_FILE, &r);
                    self.message = format!("Registered to {}. Thank you!", r.account_id);
                    self.reg = Some(r);
                    self.state = State::Registered;
                    self.nag_open = false;
                }
                Msg::SignIn(Err(e)) => {
                    self.signing_in = false;
                    self.message = e;
                }
                Msg::File { path, sha256, record } => {
                    self.busy = false;
                    self.file = Some((path, sha256, record));
                }
                Msg::Folder(r) => {
                    self.busy = false;
                    self.folder = Some(r);
                }
                Msg::SelfCheck(t) => self.self_check = t,
            }
        }
    }

    fn nag(&mut self, ctx: &egui::Context) {
        let wait_left = NAG_WAIT.saturating_sub(self.nag_since.elapsed());
        egui::Modal::new(egui::Id::new("nag")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(RichText::new("UNREGISTERED COPY").color(Color32::from_rgb(200, 40, 40)).strong());
            ui.add_space(6.0);
            ui.label(format!("{PRODUCT} {VERSION} is shareware."));
            ui.label("A license pays the developers and the contributors of this program, turns on the folder check, and removes this screen.");
            ui.label("You can resell your license when you no longer need it.");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Buy a license").clicked() {
                    open_url(&format!("{SITE_URL}/buy.html"));
                }
                if ui
                    .add_enabled(!self.signing_in, egui::Button::new("I have a license"))
                    .clicked()
                {
                    self.start_sign_in(ctx);
                }
                let label = if wait_left.is_zero() {
                    "Continue unregistered".to_string()
                } else {
                    format!("Continue unregistered ({})", wait_left.as_secs() + 1)
                };
                if ui.add_enabled(wait_left.is_zero(), egui::Button::new(label)).clicked() {
                    self.nag_open = false;
                }
            });
            if !self.message.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new(&self.message).italics());
            }
        });
        if !wait_left.is_zero() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages();

        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if let Some(p) = dropped.into_iter().next() {
            self.on_drop(p, ctx);
        }

        egui::TopBottomPanel::top("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                match (&self.state, &self.reg) {
                    (State::Registered, Some(r)) => {
                        ui.label(RichText::new(format!(
                            "Registered to {}, updates until {}",
                            r.account_id,
                            date(r.updates_until_ms)
                        )).color(Color32::from_rgb(30, 140, 60)).strong());
                    }
                    (State::Checking, _) => {
                        ui.label("Checking the license...");
                    }
                    _ => {
                        ui.label(RichText::new("UNREGISTERED").color(Color32::from_rgb(200, 40, 40)).strong());
                        if ui.small_button("Register").clicked() {
                            self.nag_open = true;
                            self.nag_since = Instant::now() - NAG_WAIT;
                        }
                    }
                }
            });
        });

        egui::TopBottomPanel::bottom("about").show(ctx, |ui| {
            ui.small(format!("{PRODUCT} {VERSION}. License contract {CONTRACT} on NEAR testnet."));
            ui.small(&self.self_check);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(PRODUCT);
            ui.label("Drop a file on this window to see its SHA-256 and its official build record.");
            ui.label(if self.state == State::Registered {
                "Drop a folder to check it against its last check."
            } else {
                "Drop a folder to check it against its last check (needs a license)."
            });
            if self.busy {
                ui.spinner();
            }
            if let Some((path, sha, record)) = &self.file {
                ui.separator();
                ui.label(RichText::new(path.display().to_string()).strong());
                match sha {
                    Ok(s) => {
                        ui.horizontal(|ui| {
                            ui.monospace(s);
                            if ui.small_button("Copy").clicked() {
                                ui.ctx().copy_text(s.clone());
                            }
                        });
                    }
                    Err(e) => {
                        ui.label(format!("Cannot read the file: {e}"));
                    }
                }
                match record {
                    Ok(Some(m)) => {
                        ui.label(RichText::new(format!(
                            "Official build {} for {}, commit {}, released {}.",
                            m.version, m.artifact.platform, m.commit, date(m.released_at_ms)
                        )).color(Color32::from_rgb(30, 140, 60)));
                        if let Some(url) = &m.artifact.attestation_url {
                            ui.hyperlink_to("Build attestation", url);
                        }
                    }
                    Ok(None) => {
                        if sha.is_ok() {
                            ui.label("The license contract has no record of this file.");
                        }
                    }
                    Err(e) => {
                        ui.label(format!("Cannot look up the record: {e}"));
                    }
                }
            }
            if let Some(result) = &self.folder {
                ui.separator();
                match result {
                    Ok((report, saved)) => {
                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                            ui.monospace(report);
                        });
                        ui.horizontal(|ui| {
                            ui.label(format!("Report saved to {}", saved.display()));
                            if ui.small_button("Copy report").clicked() {
                                ui.ctx().copy_text(report.clone());
                            }
                        });
                    }
                    Err(e) => {
                        ui.label(format!("Folder check failed: {e}"));
                    }
                }
            }
            if !self.nag_open && !self.message.is_empty() {
                ui.separator();
                ui.label(RichText::new(&self.message).italics());
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.hyperlink_to("Licenses and resale", format!("{SITE_URL}/licenses.html"));
                ui.hyperlink_to("Verify a download", format!("{SITE_URL}/verify.html"));
                ui.hyperlink_to("Contract", format!("{EXPLORER}/{CONTRACT}"));
            });
        });

        if self.nag_open && self.state != State::Checking {
            self.nag(ctx);
        }
    }
}

/// Finds a license of `account_id` that covers this version, and checks that
/// `public_key` is still a full-access key of the account.
fn license_for(near: &Near, account_id: &str, public_key: &str) -> Result<Option<Registration>, String> {
    if !near.is_full_access_key(account_id, public_key)? {
        return Ok(None);
    }
    if !near.is_entitled(account_id, VERSION)? {
        return Ok(None);
    }
    let best = near
        .licenses_for_owner(account_id)?
        .into_iter()
        .max_by_key(|l| l.updates_until_ms);
    Ok(best.map(|l| Registration {
        account_id: account_id.to_string(),
        public_key: public_key.to_string(),
        token_id: l.token_id,
        updates_until_ms: l.updates_until_ms,
        checked_at_ms: store::now_ms(),
    }))
}

fn check_folder(path: &std::path::Path) -> Result<(String, PathBuf), String> {
    let name = folder::snapshot_name(path);
    let new = folder::snapshot(path, store::now_ms()).map_err(|e| e.to_string())?;
    let report = match store::load::<folder::Snapshot>(&name) {
        Some(old) => {
            let c = folder::compare(&old, &new);
            format!(
                "Compared with the check of {}.\n{}",
                date(old.taken_at_ms),
                folder::report(&new, &c)
            )
        }
        None => format!(
            "First check of this folder: {} files recorded. Drop the folder again later to see the changes.\n",
            new.files.len()
        ),
    };
    store::save(&name, &new)?;
    let saved = store::save("last-report.txt", &report)?;
    Ok((report, saved))
}

/// UTC date as YYYY-MM-DD.
fn date(ms: u64) -> String {
    let days = (ms / 86_400_000) as i64;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn open_url(url: &str) {
    let result = if cfg!(target_os = "windows") {
        // Not "cmd /C start": cmd reads each "&" in the URL as a command separator.
        std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(e) = result {
        eprintln!("cannot open {url}: {e}");
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(PRODUCT)
            .with_inner_size([720.0, 520.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(PRODUCT, options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[cfg(test)]
mod tests {
    #[test]
    fn date_formats_utc_days() {
        assert_eq!(super::date(0), "1970-01-01");
        assert_eq!(super::date(1_762_732_800_000), "2025-11-10");
        assert_eq!(super::date(951_782_400_000), "2000-02-29");
    }
}
