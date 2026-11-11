//! BuildDeed Checksum: a file checksum tool, sold as shareware with a
//! BuildDeed license on NEAR.
//!
//! - Drop a file: the app shows its SHA-256, and the official build that
//!   contains the file, if the license contract has a record of it.
//! - Drop a folder: the folder check compares every file with the last check
//!   of that folder. This feature needs a license.
//!
//! Registration: a license has three computer seats. The app shows this
//! computer's id; the owner registers it on the website (one wallet
//! transaction); the app then finds its seat in the license contract.
//!
//! The license check is in the source code, and anyone may build the app
//! without it (see LICENSE). The official build is the one that the license
//! contract records.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod folder;
mod store;

use builddeed::{ArtifactMatch, Client, Product, Status};
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
/// The seat of this computer, from the contract.
type Registration = builddeed::Seat;

const PRODUCT: &str = "BuildDeed Checksum";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const CONTRACT: &str = "demo.builddeed.testnet";
const SITE_URL: &str = "https://13rac1.github.io/builddeed";
const EXPLORER: &str = "https://testnet.nearblocks.io/address";
/// Earlier versions kept the last license check in this file for offline
/// use. The app no longer reads it, and removes it.
const OLD_REG_FILE: &str = "registration.json";
const NAG_WAIT: Duration = Duration::from_secs(5);
/// While the user registers on the website, the app asks the contract this
/// often, for at most REGISTER_WAIT.
#[cfg(not(test))]
const POLL_EVERY: Duration = Duration::from_secs(5);
#[cfg(test)]
const POLL_EVERY: Duration = Duration::from_millis(50);
const REGISTER_WAIT: Duration = Duration::from_secs(600);

/// The result of a dropped file.
struct FileCheck {
    path: PathBuf,
    sha256: Result<String, String>,
    record: Result<Option<ArtifactMatch>, String>,
}

enum Msg {
    Check(Result<Status, String>),
    Registered(Result<Registration, String>),
    File(FileCheck),
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
    client: Arc<Client>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    state: State,
    reg: Option<Registration>,
    nag_open: bool,
    nag_since: Instant,
    /// This computer's id, or why the app cannot read it.
    device: Result<String, String>,
    /// Set while the app waits for the registration; store true to stop.
    waiting: Option<Arc<AtomicBool>>,
    message: String,
    file: Option<FileCheck>,
    folder: Option<Result<(String, PathBuf), String>>,
    busy: bool,
    self_check: String,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        cc.egui_ctx.set_zoom_factor(1.1);
        let (tx, rx) = channel();
        let client = Arc::new(Client::new(product()));
        store::remove(OLD_REG_FILE);
        let device = client
            .device_id()
            .map(str::to_string)
            .map_err(|e| e.to_string());
        let mut app = Self {
            client,
            tx,
            rx,
            state: State::Checking,
            reg: None,
            nag_open: true,
            nag_since: Instant::now(),
            device,
            waiting: None,
            message: String::new(),
            file: None,
            folder: None,
            busy: false,
            self_check: "Checking this program...".into(),
        };
        match app.device.clone() {
            Ok(_) => app.spawn_check(cc.egui_ctx.clone()),
            Err(e) => {
                app.state = State::Unregistered;
                app.message = e;
            }
        }
        app.spawn_self_check(cc.egui_ctx.clone());
        app
    }

    /// Asks the contract for this computer's seat at each start, so that a
    /// license that was sold, or a removed seat, stops working. There is no
    /// offline mode: with no network the app runs unregistered, because a
    /// stored result on this computer could be copied or edited.
    fn spawn_check(&self, ctx: egui::Context) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Check(client.status().map_err(|e| e.to_string())));
            ctx.request_repaint();
        });
    }

    fn spawn_self_check(&self, ctx: egui::Context) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let text = match std::env::current_exe().and_then(|p| builddeed::sha256_file(&p)) {
                Err(e) => format!("Cannot read this program: {e}"),
                Ok(sha) => match client.find_artifact(&sha) {
                    Ok(Some(m)) => format!(
                        "Official build {} (commit {}).\nSHA-256 {sha}",
                        m.version,
                        m.commit.get(..12).unwrap_or(&m.commit)
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

    /// Opens the website's register page for this computer, then asks the
    /// contract every few seconds until the computer has a seat.
    fn start_register(&mut self, ctx: &egui::Context) {
        if let Err(e) = self.client.open_register_page() {
            self.message = e.to_string();
            return;
        }
        self.message = "Register this computer on the website. This app sees the registration a few seconds after the transaction.".into();
        let stop = Arc::new(AtomicBool::new(false));
        self.waiting = Some(stop.clone());
        let client = self.client.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = client
                .wait_for_seat(&stop, POLL_EVERY, REGISTER_WAIT)
                .map_err(|e| e.to_string());
            if !stop.load(Ordering::Relaxed) {
                let _ = tx.send(Msg::Registered(result));
                ctx.request_repaint();
            }
        });
    }

    fn stop_waiting(&mut self) {
        if let Some(stop) = self.waiting.take() {
            stop.store(true, Ordering::Relaxed);
        }
    }

    fn on_drop(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.busy {
            self.message = "Wait for the current check to finish.".into();
            return;
        }
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
            let client = self.client.clone();
            std::thread::spawn(move || {
                let sha256 = builddeed::sha256_file(&path).map_err(|e| e.to_string());
                let record = match &sha256 {
                    Ok(s) => client.find_artifact(s).map_err(|e| e.to_string()),
                    Err(_) => Ok(None),
                };
                let _ = tx.send(Msg::File(FileCheck {
                    path,
                    sha256,
                    record,
                }));
                ctx.request_repaint();
            });
        }
    }

    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Check(Ok(Status::Registered(r))) => self.registered(r),
                Msg::Check(Ok(Status::NoSeat)) => {
                    self.reg = None;
                    self.state = State::Unregistered;
                }
                Msg::Check(Ok(Status::LicenseEnded { token_id })) => {
                    self.reg = None;
                    self.state = State::Unregistered;
                    self.message = format!(
                        "License #{token_id} ended before version {VERSION}. Renew it on the website."
                    );
                }
                Msg::Check(Err(e)) => {
                    self.state = State::Unregistered;
                    self.message =
                        format!("Cannot check the license, so the app runs unregistered: {e}");
                }
                Msg::Registered(Ok(r)) => {
                    self.waiting = None;
                    self.message = format!("Registered to {}. Thank you!", r.account_id);
                    self.registered(r);
                }
                Msg::Registered(Err(e)) => {
                    self.waiting = None;
                    self.message = e;
                }
                Msg::File(f) => {
                    self.busy = false;
                    self.file = Some(f);
                }
                Msg::Folder(r) => {
                    self.busy = false;
                    self.folder = Some(r);
                }
                Msg::SelfCheck(t) => self.self_check = t,
            }
        }
    }

    fn registered(&mut self, r: Registration) {
        self.reg = Some(r);
        self.state = State::Registered;
        self.nag_open = false;
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
                    if let Err(e) = self.client.open_buy_page() {
                        self.message = e.to_string();
                    }
                }
                if self.waiting.is_some() {
                    if ui.button("Stop waiting").clicked() {
                        self.stop_waiting();
                        self.message.clear();
                    }
                } else if ui.button("I have a license").clicked() {
                    self.start_register(ctx);
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
            if self.waiting.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Waiting for the registration...");
                });
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

        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(p) = dropped.into_iter().next() {
            self.on_drop(p, ctx);
        }

        egui::TopBottomPanel::top("status").show(ctx, |ui| {
            ui.horizontal(|ui| match (&self.state, &self.reg) {
                (State::Registered, Some(r)) => {
                    ui.label(
                        RichText::new(format!(
                            "Registered to {}, updates until {}",
                            r.account_id,
                            date(r.updates_until_ms)
                        ))
                        .color(Color32::from_rgb(30, 140, 60))
                        .strong(),
                    );
                }
                (State::Checking, _) => {
                    ui.label("Checking the license...");
                }
                _ => {
                    ui.label(
                        RichText::new("UNREGISTERED")
                            .color(Color32::from_rgb(200, 40, 40))
                            .strong(),
                    );
                    if ui.small_button("Register").clicked() {
                        self.nag_open = true;
                        self.nag_since = Instant::now() - NAG_WAIT;
                    }
                }
            });
        });

        egui::TopBottomPanel::bottom("about").show(ctx, |ui| {
            ui.small(format!(
                "{PRODUCT} {VERSION}. License contract {CONTRACT} on NEAR testnet."
            ));
            ui.small(&self.self_check);
            if let Ok(d) = self.client.short_device_id() {
                ui.small(format!("This computer: {d}"));
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(PRODUCT);
            ui.label(
                "Drop a file on this window to see its SHA-256 and its official build record.",
            );
            ui.label(if self.state == State::Registered {
                "Drop a folder to check it against its last check."
            } else {
                "Drop a folder to check it against its last check (needs a license)."
            });
            if self.busy {
                ui.spinner();
            }
            if let Some(FileCheck {
                path,
                sha256: sha,
                record,
            }) = &self.file
            {
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
                        ui.label(
                            RichText::new(format!(
                                "Official build {} for {}, commit {}, released {}.",
                                m.version,
                                m.artifact.platform,
                                m.commit,
                                date(m.released_at_ms)
                            ))
                            .color(Color32::from_rgb(30, 140, 60)),
                        );
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
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
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

/// The product that this app sells.
fn product() -> Product {
    Product::testnet(CONTRACT, "demo", VERSION).with_site_url(SITE_URL)
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
    let saved = store::save_text("last-report.txt", &report)?;
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
    use super::*;
    use builddeed::fake_rpc;

    #[test]
    fn product_is_the_demo_contract_and_this_version() {
        let p = product();
        assert_eq!(
            (p.contract.as_str(), p.key.as_str()),
            ("demo.builddeed.testnet", "demo")
        );
        assert_eq!(p.version, VERSION);
        let c = Client::with_device(p, "a".repeat(64));
        assert!(c
            .register_url()
            .unwrap()
            .starts_with(&format!("{SITE_URL}/register.html#product=demo&device=")));
    }

    #[test]
    fn status_tells_ended_from_no_seat() {
        let url = fake_rpc::serve(vec![
            fake_rpc::seat("2", "a.testnet", false),
            fake_rpc::no_seat(),
        ]);
        let c = Client::with_device(product().with_rpc_url(url), "a".repeat(64));
        assert_eq!(
            c.status(),
            Ok(Status::LicenseEnded {
                token_id: "2".into()
            })
        );
        assert_eq!(c.status(), Ok(Status::NoSeat));
    }

    #[test]
    fn date_formats_utc_days() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_762_732_800_000), "2025-11-10");
        assert_eq!(date(951_782_400_000), "2000-02-29");
    }
}
