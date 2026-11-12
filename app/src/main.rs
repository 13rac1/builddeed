//! BuildDeed Checksum: a file checksum tool, sold as shareware with a
//! BuildDeed license on NEAR.
//!
//! - Drop or choose a file: the app shows its SHA-256, and the official
//!   build that contains the file, if the license contract has a record of it.
//! - Drop or choose a folder: the folder check compares every file with the
//!   last check of that folder. This feature needs a license.
//!
//! The window is in ui/app.slint (Slint). Background threads send their
//! results through a channel and then call the window's `messages-arrived`
//! callback in the event loop.
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

slint::include_modules!();

use builddeed::{ArtifactMatch, Client, Product, Status};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::{winit::event::WindowEvent, EventResult, WinitWindowAccessor};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
/// The seat of this computer, from the contract.
type Registration = builddeed::Seat;

const PRODUCT: &str = "BuildDeed Checksum";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const CONTRACT: &str = builddeed::OFFICIAL_TESTNET_CONTRACT;
const PRODUCT_KEY: &str = "checksum";
const SITE_URL: &str = "https://builddeed.dev";
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
/// A monospace font that each system has.
const MONO_FONT: &str = if cfg!(target_os = "windows") {
    "Consolas"
} else if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "DejaVu Sans Mono"
};

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

/// Sends the results of background threads to the window.
#[derive(Clone)]
struct Outbox {
    tx: Sender<Msg>,
    ui: slint::Weak<MainWindow>,
}

impl Outbox {
    fn send(&self, msg: Msg) {
        if self.tx.send(msg).is_ok() {
            let _ = self
                .ui
                .upgrade_in_event_loop(|ui| ui.invoke_messages_arrived());
        }
    }
}

#[derive(PartialEq)]
enum State {
    Checking,
    Registered,
    Unregistered,
}

struct App {
    client: Arc<Client>,
    out: Outbox,
    rx: Receiver<Msg>,
    ui: slint::Weak<MainWindow>,
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
    fn new(ui: &MainWindow) -> Self {
        let (tx, rx) = channel();
        let client = Arc::new(Client::new(product()));
        store::remove(OLD_REG_FILE);
        let device = client
            .device_id()
            .map(str::to_string)
            .map_err(|e| e.to_string());
        let mut app = Self {
            client,
            out: Outbox {
                tx,
                ui: ui.as_weak(),
            },
            rx,
            ui: ui.as_weak(),
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
            Ok(_) => app.spawn_check(),
            Err(e) => {
                app.state = State::Unregistered;
                app.message = e;
            }
        }
        app.spawn_self_check();
        app
    }

    /// Asks the contract for this computer's seat at each start, so that a
    /// license that was sold, or a removed seat, stops working. There is no
    /// offline mode: with no network the app runs unregistered, because a
    /// stored result on this computer could be copied or edited.
    fn spawn_check(&self) {
        let client = self.client.clone();
        let out = self.out.clone();
        std::thread::spawn(move || {
            out.send(Msg::Check(client.status().map_err(|e| e.to_string())));
        });
    }

    fn spawn_self_check(&self) {
        let client = self.client.clone();
        let out = self.out.clone();
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
            out.send(Msg::SelfCheck(text));
        });
    }

    fn buy(&mut self) {
        if let Err(e) = self.client.open_buy_page() {
            self.message = e.to_string();
        }
    }

    /// Opens the website's register page for this computer, then asks the
    /// contract every few seconds until the computer has a seat.
    fn start_register(&mut self) {
        if let Err(e) = self.client.open_register_page() {
            self.message = e.to_string();
            return;
        }
        self.message = "Register this computer on the website. This app sees the registration a few seconds after the transaction.".into();
        let stop = Arc::new(AtomicBool::new(false));
        self.waiting = Some(stop.clone());
        let client = self.client.clone();
        let out = self.out.clone();
        std::thread::spawn(move || {
            let result = client
                .wait_for_seat(&stop, POLL_EVERY, REGISTER_WAIT)
                .map_err(|e| e.to_string());
            if !stop.load(Ordering::Relaxed) {
                out.send(Msg::Registered(result));
            }
        });
    }

    fn stop_waiting(&mut self) {
        if let Some(stop) = self.waiting.take() {
            stop.store(true, Ordering::Relaxed);
        }
    }

    /// Opens the nag screen with the "Continue unregistered" button enabled.
    fn open_nag(&mut self) {
        self.nag_open = true;
        self.nag_since = Instant::now() - NAG_WAIT;
    }

    /// True, with a message and the nag screen, if the folder check is not
    /// available because this computer has no seat.
    fn folder_needs_license(&mut self) -> bool {
        if self.state == State::Registered {
            return false;
        }
        self.message = "The folder check needs a license. Register to use it.".into();
        self.nag_open = true;
        true
    }

    fn on_drop(&mut self, path: PathBuf) {
        if self.busy {
            self.message = "Wait for the current check to finish.".into();
            return;
        }
        let out = self.out.clone();
        if path.is_dir() {
            if self.folder_needs_license() {
                return;
            }
            self.busy = true;
            std::thread::spawn(move || {
                out.send(Msg::Folder(check_folder(&path)));
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
                out.send(Msg::File(FileCheck {
                    path,
                    sha256,
                    record,
                }));
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

    /// Seconds until "Continue unregistered" is enabled; zero when it is.
    fn wait_left(&self) -> Duration {
        NAG_WAIT.saturating_sub(self.nag_since.elapsed())
    }

    /// Sets the countdown of the "Continue unregistered" button.
    fn show_countdown(&self, ui: &MainWindow) {
        let left = self.wait_left();
        ui.set_can_continue(left.is_zero());
        ui.set_continue_label(if left.is_zero() {
            "Continue unregistered".into()
        } else {
            format!("Continue unregistered ({})", left.as_secs() + 1).into()
        });
    }

    /// Copies the state of the app to the window.
    fn show(&self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        ui.set_app_state(match self.state {
            State::Checking => AppState::Checking,
            State::Registered => AppState::Registered,
            State::Unregistered => AppState::Unregistered,
        });
        ui.set_registered_text(
            self.reg
                .as_ref()
                .map(|r| {
                    format!(
                        "Registered to {}, updates until {}",
                        r.account_id,
                        date(r.updates_until_ms)
                    )
                })
                .unwrap_or_default()
                .into(),
        );
        ui.set_nag_open(self.nag_open && self.state != State::Checking);
        self.show_countdown(&ui);
        ui.set_waiting(self.waiting.is_some());
        ui.set_confirm_code(self.client.confirm_code().unwrap_or_default().into());
        ui.set_message(self.message.as_str().into());
        ui.set_busy(self.busy);

        ui.set_file_shown(self.file.is_some());
        if let Some(f) = &self.file {
            ui.set_file_path(f.path.display().to_string().into());
            let (sha, error) = match &f.sha256 {
                Ok(s) => (s.clone(), String::new()),
                Err(e) => (String::new(), format!("Cannot read the file: {e}")),
            };
            ui.set_file_sha(sha.into());
            ui.set_file_error(error.into());
            let (text, official, url) = match &f.record {
                Ok(Some(m)) => (
                    format!(
                        "Official build {} for {}, commit {}, released {}.",
                        m.version,
                        m.artifact.platform,
                        m.commit,
                        date(m.released_at_ms)
                    ),
                    true,
                    m.artifact.attestation_url.clone().unwrap_or_default(),
                ),
                Ok(None) if f.sha256.is_ok() => (
                    "The license contract has no record of this file.".into(),
                    false,
                    String::new(),
                ),
                Ok(None) => (String::new(), false, String::new()),
                Err(e) => (
                    format!("Cannot look up the record: {e}"),
                    false,
                    String::new(),
                ),
            };
            ui.set_record_text(text.into());
            ui.set_record_official(official);
            ui.set_attestation_url(url.into());
        }

        ui.set_folder_shown(self.folder.is_some());
        match &self.folder {
            Some(Ok((report, saved))) => {
                ui.set_folder_report(report.as_str().into());
                ui.set_folder_saved(format!("Report saved to {}", saved.display()).into());
                ui.set_folder_error("".into());
            }
            Some(Err(e)) => ui.set_folder_error(format!("Folder check failed: {e}").into()),
            None => {}
        }

        ui.set_self_check(self.self_check.as_str().into());
    }
}

/// The product that this app sells.
fn product() -> Product {
    Product::testnet(CONTRACT, PRODUCT_KEY, VERSION).with_site_url(SITE_URL)
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

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    ui.set_mono_font(MONO_FONT.into());
    ui.set_product_version(VERSION.into());
    ui.set_about_line(
        format!("{PRODUCT} {VERSION}. License contract {CONTRACT} on NEAR testnet.").into(),
    );
    ui.set_licenses_url(format!("{SITE_URL}/licenses/").into());
    ui.set_verify_url(format!("{SITE_URL}/verify/").into());
    ui.set_contract_url(format!("{EXPLORER}/{CONTRACT}").into());

    let app = Rc::new(RefCell::new(App::new(&ui)));
    if let Ok(d) = app.borrow().client.short_device_id() {
        ui.set_device_line(format!("This computer: {d}").into());
    }
    app.borrow().show();

    // Each handler changes the app, then copies its state to the window.
    let on = |f: fn(&mut App)| {
        let app = app.clone();
        move || {
            let mut a = app.borrow_mut();
            f(&mut a);
            a.show();
        }
    };
    ui.on_messages_arrived(on(App::handle_messages));
    ui.on_buy(on(App::buy));
    ui.on_register(on(App::start_register));
    ui.on_stop_waiting(on(|a| {
        a.stop_waiting();
        a.message.clear();
    }));
    ui.on_continue_unregistered(on(|a| a.nag_open = false));
    ui.on_open_nag(on(App::open_nag));
    ui.on_choose_file({
        let (app, ui) = (app.clone(), ui.as_weak());
        move || pick(&app, &ui, false)
    });
    ui.on_choose_folder({
        let (app, ui) = (app.clone(), ui.as_weak());
        move || pick(&app, &ui, true)
    });

    // Drag and drop. Slint has no event for dropped files, so the app reads
    // the winit events of the window. winit has no drag and drop on Wayland:
    // there the window tells the user to choose the file.
    ui.window().on_winit_window_event({
        let (app, weak) = (app.clone(), ui.as_weak());
        let mut wayland_checked = false;
        move |w, event| match event {
            _ if !wayland_checked => {
                wayland_checked = true;
                let wayland = matches!(
                    w.window_handle().window_handle().map(|h| h.as_raw()),
                    Ok(RawWindowHandle::Wayland(_))
                );
                if let Some(ui) = weak.upgrade() {
                    ui.set_can_drop(!wayland);
                }
                EventResult::Propagate
            }
            WindowEvent::DroppedFile(path) => {
                if let Some(ui) = weak.upgrade() {
                    ui.set_drop_hover(false);
                }
                let mut a = app.borrow_mut();
                a.on_drop(path.clone());
                a.show();
                EventResult::PreventDefault
            }
            WindowEvent::HoveredFile(_) | WindowEvent::HoveredFileCancelled => {
                if let Some(ui) = weak.upgrade() {
                    ui.set_drop_hover(matches!(event, WindowEvent::HoveredFile(_)));
                }
                EventResult::PreventDefault
            }
            _ => EventResult::Propagate,
        }
    });

    // The countdown of the nag screen.
    let countdown = slint::Timer::default();
    countdown.start(slint::TimerMode::Repeated, Duration::from_millis(250), {
        let (app, ui) = (app.clone(), ui.as_weak());
        move || {
            if let Some(ui) = ui.upgrade() {
                app.borrow().show_countdown(&ui);
            }
        }
    });

    ui.run()
}

/// Shows the system's file or folder dialog, then checks the chosen path.
fn pick(app: &Rc<RefCell<App>>, ui: &slint::Weak<MainWindow>, folder: bool) {
    let Some(ui) = ui.upgrade() else {
        return;
    };
    if folder && app.borrow_mut().folder_needs_license() {
        app.borrow().show();
        return;
    }
    let dialog = rfd::AsyncFileDialog::new().set_parent(&ui.window().window_handle());
    let app = app.clone();
    let _ = slint::spawn_local(async move {
        let picked = if folder {
            dialog.set_title("Check a folder").pick_folder().await
        } else {
            dialog.set_title("Choose a file").pick_file().await
        };
        if let Some(handle) = picked {
            let mut a = app.borrow_mut();
            a.on_drop(handle.path().to_path_buf());
            a.show();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use builddeed::fake_rpc;

    #[test]
    fn product_is_checksum_in_the_official_contract() {
        let p = product();
        assert_eq!(
            (p.contract.as_str(), p.key.as_str()),
            ("licenses.builddeed.testnet", "checksum")
        );
        assert_eq!(p.version, VERSION);
        let c = Client::with_device(p, "a".repeat(64));
        assert!(c
            .register_url()
            .unwrap()
            .starts_with(&format!("{SITE_URL}/register/#product=checksum&device=")));
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
