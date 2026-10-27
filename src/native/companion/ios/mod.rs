//! Process-owned WatchConnectivity delivery of the public Rust cache, independent of NSH VMs.
mod files;
mod policy;
mod session;

use super::{Manifest, read_manifest};
use crate::native::lifecycle;
use objc2::rc::{Retained, autoreleasepool};
use objc2_foundation::{NSDictionary, NSString, NSURL};
use policy::Retry;
use session::{Delegate, WCSession};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Mutex, Once, OnceLock,
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

/// The OS owns queued transfers; the app never builds an unbounded file backlog.
const MAX_TRANSFERS: usize = 2;
const MAX_COMPLETIONS: usize = 64;

#[derive(Clone)]
struct Transfer {
    stream: String,
    revision: u64,
    hash: String,
    path: PathBuf,
}

#[derive(Default)]
struct Signals {
    root: Option<PathBuf>,
    reset: bool,
    activated: Option<bool>,
    completed: Vec<(Transfer, bool)>,
}

static SIGNALS: Mutex<Signals> = Mutex::new(Signals {
    root: None,
    reset: false,
    activated: None,
    completed: Vec::new(),
});
static WAKE: OnceLock<SyncSender<()>> = OnceLock::new();

/// Registers a process launch callback; neither JavaScript nor a scene starts the session.
pub(crate) fn install() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| lifecycle::install(start));
}

/// Coalesces owner commits. Only a root path crosses this boundary, never image bytes or secrets.
pub(super) fn changed(root: &Path) {
    SIGNALS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .root = Some(root.to_owned());
    wake();
}

fn wake() {
    if let Some(tx) = WAKE.get() {
        let _ = tx.try_send(());
    }
}

fn system_changed() {
    SIGNALS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .reset = true;
    wake();
}

fn activation_finished(success: bool) {
    SIGNALS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .activated = Some(success);
    wake();
}

fn finished(transfer: Transfer, success: bool) {
    let mut signals = SIGNALS.lock().unwrap_or_else(|error| error.into_inner());
    if signals.completed.len() < MAX_COMPLETIONS {
        signals.completed.push((transfer, success));
    } else {
        // A storm of obsolete callbacks needs one reconciliation, not more queued work.
        signals.reset = true;
    }
    drop(signals);
    wake();
}

fn default_root() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    home.is_absolute()
        .then(|| home.join("Library/Application Support/loki-link-v1/companion-v1"))
}

fn start() {
    static START: Once = Once::new();
    START.call_once(|| {
        let Some(root) = default_root() else { return };
        let (tx, rx) = mpsc::sync_channel(1);
        let _ = WAKE.set(tx);
        // A new foreground window can retry work that exhausted its earlier budget.
        static ACTIVITY: OnceLock<lifecycle::ActivityObserver> = OnceLock::new();
        let _ = ACTIVITY.set(lifecycle::observe(|visible| {
            if visible {
                system_changed();
            }
        }));
        let _ = thread::Builder::new()
            .name("loki-watch-cache".into())
            .spawn(move || {
                autoreleasepool(|_| {
                    if WCSession::is_supported() {
                        Actor::new(root).run(rx);
                    }
                });
            });
    });
}

struct Actor {
    root: PathBuf,
    session: Retained<WCSession>,
    _delegate: Retained<Delegate>,
    manifest: Option<Manifest>,
    context: Option<Vec<u8>>,
    context_retry: Retry,
    activation_retry: Retry,
    activation_deadline: Option<Instant>,
    successful: BTreeSet<String>,
    attempts: BTreeMap<String, Retry>,
    local_failures: BTreeSet<String>,
    in_flight: BTreeMap<PathBuf, Transfer>,
}

impl Actor {
    fn new(root: PathBuf) -> Self {
        let session = WCSession::shared();
        let delegate = Delegate::new();
        session.attach(&delegate);
        Self {
            root,
            session,
            _delegate: delegate,
            manifest: None,
            context: None,
            context_retry: Retry::default(),
            activation_retry: Retry::default(),
            activation_deadline: None,
            successful: BTreeSet::new(),
            attempts: BTreeMap::new(),
            local_failures: BTreeSet::new(),
            in_flight: BTreeMap::new(),
        }
    }

    fn run(mut self, rx: Receiver<()>) {
        loop {
            let signals =
                std::mem::take(&mut *SIGNALS.lock().unwrap_or_else(|error| error.into_inner()));
            self.apply(signals);
            autoreleasepool(|_| self.flush());
            let deadline = self
                .attempts
                .values()
                .filter_map(Retry::deadline)
                .chain(self.context_retry.deadline())
                .chain(self.activation_retry.deadline())
                .chain(self.activation_deadline)
                .filter(|deadline| *deadline > Instant::now())
                .min();
            // With no pending retry the worker sleeps indefinitely. iOS owns suspension.
            match deadline {
                Some(deadline) => {
                    let _ = rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
                }
                None => {
                    if rx.recv().is_err() {
                        return;
                    }
                }
            }
        }
    }

    fn apply(&mut self, signals: Signals) {
        if let Some(root) = signals.root {
            // The product has one fixed public cache; reject accidental arbitrary-path reads.
            if default_root().as_ref() == Some(&root) {
                self.root = root;
                // Newly committed image bytes can arrive after the manifest itself.
                // A local read failure must not permanently exhaust that image.
                self.local_failures.retain(|hash| {
                    let arrived = fs::symlink_metadata(self.root.join("assets").join(hash))
                        .is_ok_and(|meta| meta.is_file() && !meta.file_type().is_symlink());
                    if arrived {
                        self.attempts.remove(hash);
                    }
                    !arrived
                });
            }
        }
        if signals.reset {
            self.context = None;
            self.context_retry = Retry::default();
            self.activation_retry = Retry::default();
            self.successful.clear();
            self.attempts.clear();
            self.local_failures.clear();
            // The OS queue is authoritative after recovery, including callbacks
            // that completed while this process was suspended or unavailable.
            self.in_flight.clear();
        }
        if let Some(success) = signals.activated {
            self.activation_deadline = None;
            if success {
                self.activation_retry = Retry::default();
                self.context = None;
                self.context_retry = Retry::default();
            } else {
                self.activation_retry.failed(Instant::now());
            }
        }
        for (transfer, success) in signals.completed {
            let known = self.in_flight.remove(&transfer.path).is_some();
            files::remove(&self.root, &transfer.path);
            if !known
                || !self
                    .manifest
                    .as_ref()
                    .is_some_and(|manifest| compatible(&transfer, manifest))
            {
                continue;
            }
            if success {
                self.successful.insert(transfer.hash.clone());
                self.attempts.remove(&transfer.hash);
            } else {
                self.attempts
                    .entry(transfer.hash)
                    .or_default()
                    .failed(Instant::now());
            }
        }
    }

    fn flush(&mut self) {
        let now = Instant::now();
        if self
            .activation_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.activation_deadline = None;
            self.activation_retry.failed(now);
        }
        if self.session.activation_state() != 2 {
            // Inactive (1) must finish deactivating before reactivation for another Watch.
            if self.session.activation_state() == 0
                && self.activation_deadline.is_none()
                && self.activation_retry.ready(now)
            {
                self.activation_retry.started();
                self.activation_deadline = Some(now + Duration::from_secs(15));
                self.session.activate();
            }
            return;
        }
        if !self.session.paired() || !self.session.installed() {
            self.cancel_outstanding(None);
            self.context = None;
            return;
        }
        let Ok(manifest) = read_manifest(&self.root) else {
            return;
        };
        let switched = self.manifest.as_ref().is_some_and(|previous| {
            previous.stream_id != manifest.stream_id || previous.source_id != manifest.source_id
        });
        if switched || manifest.snapshot.is_none() {
            self.cancel_outstanding(None);
            self.successful.clear();
            self.attempts.clear();
            self.local_failures.clear();
        } else {
            self.cancel_outstanding(Some(&manifest));
        }
        if self.manifest.as_ref() != Some(&manifest) {
            self.context = None;
            self.context_retry = Retry::default();
            self.successful
                .retain(|hash| manifest.assets.iter().any(|asset| &asset.hash == hash));
            self.attempts
                .retain(|hash, _| manifest.assets.iter().any(|asset| &asset.hash == hash));
            self.local_failures
                .retain(|hash| manifest.assets.iter().any(|asset| &asset.hash == hash));
            self.manifest = Some(manifest.clone());
        }
        if self.context.is_none() && self.context_retry.ready(now) {
            self.context_retry.started();
            if let Ok(bytes) = serde_json::to_vec(&manifest) {
                if bytes.len() <= 48 * 1024 && self.session.context(&bytes).is_ok() {
                    self.context = Some(bytes);
                } else {
                    self.context_retry.failed(now);
                }
            }
        }
        let mut occupied: BTreeSet<_> = self
            .in_flight
            .values()
            .map(|transfer| transfer.hash.clone())
            .collect();
        for asset in &manifest.assets {
            if self.in_flight.len() >= MAX_TRANSFERS {
                break;
            }
            if self.successful.contains(&asset.hash)
                || occupied.contains(&asset.hash)
                || !self
                    .attempts
                    .entry(asset.hash.clone())
                    .or_default()
                    .ready(now)
            {
                continue;
            }
            self.attempts
                .entry(asset.hash.clone())
                .or_default()
                .started();
            match files::stage(&self.root, asset) {
                Ok(path) => {
                    self.local_failures.remove(&asset.hash);
                    // Staging can take time; a source switch on the owner must win.
                    if !read_manifest(&self.root).is_ok_and(|latest| {
                        latest.stream_id == manifest.stream_id
                            && latest.source_id == manifest.source_id
                            && latest.assets.iter().any(|candidate| candidate == asset)
                    }) {
                        files::remove(&self.root, &path);
                        wake();
                        return;
                    }
                    let transfer = Transfer {
                        stream: manifest.stream_id.clone(),
                        revision: manifest.revision.parse().unwrap_or(0),
                        hash: asset.hash.clone(),
                        path,
                    };
                    let keys: Vec<_> = ["kind", "streamId", "revision", "hash"]
                        .iter()
                        .map(|key| NSString::from_str(key))
                        .collect();
                    let values: Vec<_> = [
                        "asset",
                        &manifest.stream_id,
                        &manifest.revision,
                        &asset.hash,
                    ]
                    .iter()
                    .map(|value| NSString::from_str(value))
                    .collect();
                    let metadata = NSDictionary::<NSString, NSString>::from_slices(
                        &keys.iter().map(|key| &**key).collect::<Vec<_>>(),
                        &values.iter().map(|value| &**value).collect::<Vec<_>>(),
                    );
                    let file = NSURL::fileURLWithPath(&NSString::from_str(
                        &transfer.path.to_string_lossy(),
                    ));
                    let _ = self.session.transfer_file(&file, Some(&metadata));
                    occupied.insert(transfer.hash.clone());
                    self.in_flight.insert(transfer.path.clone(), transfer);
                }
                Err(_) => {
                    self.local_failures.insert(asset.hash.clone());
                    self.attempts
                        .entry(asset.hash.clone())
                        .or_default()
                        .failed(now);
                }
            }
        }
    }

    fn cancel_outstanding(&mut self, manifest: Option<&Manifest>) {
        let outstanding = self.session.outstanding();
        let mut retained = BTreeSet::new();
        for index in 0..outstanding.count() {
            let transfer = outstanding.objectAtIndex(index);
            let Some(description) = transfer.description() else {
                continue;
            };
            if manifest.is_some_and(|manifest| compatible(&description, manifest)) {
                retained.insert(description.path.clone());
                self.in_flight
                    .entry(description.path.clone())
                    .or_insert(description);
            } else {
                transfer.cancel();
                self.in_flight.remove(&description.path);
            }
        }
        // Our system-owned queue may outlive this process. Reconcile its files before pruning.
        self.in_flight
            .retain(|_, transfer| manifest.is_some_and(|manifest| compatible(transfer, manifest)));
        retained.extend(self.in_flight.keys().cloned());
        if let Ok(files) = fs::read_dir(self.root.join("staging")) {
            for file in files.flatten() {
                if !retained.contains(&file.path()) {
                    files::remove(&self.root, &file.path());
                }
            }
        }
    }
}

fn compatible(transfer: &Transfer, manifest: &Manifest) -> bool {
    policy::reusable(
        &transfer.stream,
        transfer.revision,
        &manifest.stream_id,
        manifest.revision.parse().unwrap_or(0),
        manifest
            .assets
            .iter()
            .any(|asset| asset.hash == transfer.hash),
    )
}
