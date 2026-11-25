use eframe::egui;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;
use ureq::tls::{TlsConfig, TlsProvider};

const RELEASES: &str = "https://github.com/matfigueroac/redbeam/releases";
const LATEST_API: &str = "https://api.github.com/repos/matfigueroac/redbeam/releases/latest";
const INSTALLER_NAME: &str = "RedBeam-Setup-x64.exe";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_SIZE: u64 = 64 * 1024 * 1024;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    state: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Clone)]
struct Installer {
    version: Version,
    size: u64,
    sha256: [u8; 32],
}

impl Installer {
    fn valid(&self, bytes: &[u8]) -> bool {
        bytes.len() as u64 == self.size && Sha256::digest(bytes)[..] == self.sha256[..]
    }
}

#[derive(Default)]
enum Update {
    #[default]
    Unknown,
    Checking,
    Downloading,
    Available(Installer),
    DownloadFailed(Installer),
    Ready(Installer, PathBuf),
    Current,
    NoRelease,
    Unavailable,
}

#[derive(Default)]
pub(crate) struct UpdateCheck {
    state: Update,
    worker: Option<Receiver<Update>>,
    visible: bool,
    install_dir: Option<PathBuf>,
}

fn version_available(release: Release, current: &Version) -> Option<Update> {
    let version = Version::parse(release.tag_name.strip_prefix('v')?).ok()?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return None;
    }
    if version <= *current {
        return Some(Update::Current);
    }
    let asset = release.assets.into_iter().find(|asset| {
        asset.name == INSTALLER_NAME
            && asset.state == "uploaded"
            && (1..=MAX_SIZE).contains(&asset.size)
    })?;
    let digest = asset.digest.as_deref()?.strip_prefix("sha256:")?;
    let mut sha256 = [0; 32];
    hex::decode_to_slice(digest, &mut sha256).ok()?;
    Some(Update::Available(Installer {
        version,
        size: asset.size,
        sha256,
    }))
}

fn agent(seconds: u64, redirects: u32) -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .build(),
        )
        .timeout_global(Some(Duration::from_secs(seconds)))
        .https_only(true)
        .max_redirects(redirects)
        .user_agent(concat!("RedBeam/", env!("CARGO_PKG_VERSION")))
        .build()
        .new_agent()
}

fn fetch_update() -> Update {
    let mut response = match agent(8, 0)
        .get(LATEST_API)
        .header("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(response) if response.status() == 200 => response,
        Err(ureq::Error::StatusCode(404)) => return Update::NoRelease,
        _ => return Update::Unavailable,
    };
    let current = Version::parse(CURRENT_VERSION).expect("Cargo valida la versión del paquete");
    response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_json::<Release>()
        .ok()
        .and_then(|release| version_available(release, &current))
        .unwrap_or(Update::Unavailable)
}

fn download(installer: &Installer) -> Option<PathBuf> {
    let url = format!(
        "{RELEASES}/download/v{}/{INSTALLER_NAME}",
        installer.version
    );
    let mut response = agent(90, 5).get(&url).call().ok()?;
    if response.status() != 200 {
        return None;
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(installer.size + 1)
        .read_to_vec()
        .ok()?;
    if !installer.valid(&bytes) {
        return None;
    }
    let project = directories::ProjectDirs::from("cl", "RedBIM", "RedBIM")?;
    let cache = project.cache_dir().join("updates");
    std::fs::create_dir_all(&cache).ok()?;
    let mut file = tempfile::NamedTempFile::new_in(&cache).ok()?;
    file.write_all(&bytes).ok()?;
    file.as_file().sync_all().ok()?;
    mark_download(file.path(), &url)?;
    let path = cache.join(INSTALLER_NAME);
    file.persist(&path).ok()?;
    Some(path)
}

#[cfg(windows)]
fn mark_download(path: &Path, url: &str) -> Option<()> {
    let mut zone = path.as_os_str().to_owned();
    zone.push(":Zone.Identifier");
    std::fs::write(
        zone,
        format!("[ZoneTransfer]\r\nZoneId=3\r\nHostUrl={url}\r\n"),
    )
    .ok()
}

#[cfg(not(windows))]
fn mark_download(_: &Path, _: &str) -> Option<()> {
    Some(())
}

#[cfg(windows)]
pub(crate) fn is_packaged() -> bool {
    use windows_sys::Win32::{
        Foundation::ERROR_INSUFFICIENT_BUFFER, Storage::Packaging::Appx::GetCurrentPackageFullName,
    };

    let mut length = 0;
    let result = unsafe { GetCurrentPackageFullName(&mut length, std::ptr::null_mut()) };
    result == ERROR_INSUFFICIENT_BUFFER
}

#[cfg(not(windows))]
pub(crate) fn is_packaged() -> bool {
    false
}

#[cfg(windows)]
fn registered_install_dir() -> Option<PathBuf> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{C5034EAE-B272-4175-92B5-F6284A1187A1}_is1")
        .ok()?;
    let registered: String = key.get_value("InstallLocation").ok()?;
    let executable = std::env::current_exe().ok()?;
    let actual = executable.parent()?;
    (actual.canonicalize().ok()? == Path::new(&registered).canonicalize().ok()?)
        .then(|| actual.to_path_buf())
}

#[cfg(not(windows))]
fn registered_install_dir() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn open_installer(path: &Path, install_dir: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

    let file: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut arguments = std::ffi::OsString::from("/DIR=\"");
    arguments.push(install_dir);
    arguments.push("\" /NORESTART");
    let args: Vec<u16> = arguments.encode_wide().chain([0]).collect();
    // Use the Windows shell so the downloaded installer is subject to its normal checks.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            file.as_ptr(),
            args.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    result as isize > 32
}

#[cfg(not(windows))]
fn open_installer(_: &Path, _: &Path) -> bool {
    false
}

impl UpdateCheck {
    pub(crate) fn check_on_startup(&mut self, ctx: &egui::Context) {
        if is_packaged() {
            return;
        }
        self.install_dir = registered_install_dir();
        if self.install_dir.is_some() {
            self.start(ctx, Update::Checking, fetch_update);
        }
    }

    pub(crate) fn open(&mut self, ctx: &egui::Context) {
        self.visible = true;
        if matches!(
            self.state,
            Update::Unknown | Update::Current | Update::NoRelease | Update::Unavailable
        ) {
            self.start(ctx, Update::Checking, fetch_update);
        }
    }

    fn start(
        &mut self,
        ctx: &egui::Context,
        pending: Update,
        work: impl FnOnce() -> Update + Send + 'static,
    ) {
        let (send, receive) = mpsc::channel();
        let ctx = ctx.clone();
        if std::thread::Builder::new()
            .name("redbim-updates".into())
            .spawn(move || {
                let _ = send.send(work());
                ctx.request_repaint();
            })
            .is_ok()
        {
            self.worker = Some(receive);
            self.state = pending;
        } else {
            self.state = Update::Unavailable;
        }
    }

    pub(crate) fn poll(&mut self) {
        let Some(worker) = &self.worker else { return };
        self.state = match worker.try_recv() {
            Ok(update) => update,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Update::Unavailable,
        };
        self.worker = None;
        if matches!(self.state, Update::Available(_)) {
            self.visible = true;
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context) -> bool {
        if !self.visible {
            return false;
        }
        let mut retry = false;
        let mut download_request = None;
        let mut install = false;
        let mut close = false;
        egui::Window::new("Actualizaciones de RedBeam")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut self.visible)
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(format!("Versión instalada: {CURRENT_VERSION}"));
                match &self.state {
                    Update::Checking | Update::Downloading => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(if matches!(self.state, Update::Checking) {
                                "Consultando GitHub…"
                            } else {
                                "Descargando y verificando el instalador…"
                            });
                        });
                    }
                    Update::Available(installer) | Update::DownloadFailed(installer) => {
                        ui.label(format!("RedBeam {} está disponible.", installer.version));
                        if matches!(self.state, Update::DownloadFailed(_)) {
                            ui.label("La descarga falló o el archivo no pasó la verificación.");
                        }
                        if self.install_dir.is_some()
                            && ui.button("Descargar actualización").clicked()
                        {
                            download_request = Some(installer.clone());
                        }
                        ui.hyperlink_to(
                            "Ver versión en GitHub",
                            format!("{RELEASES}/tag/v{}", installer.version),
                        );
                    }
                    Update::Ready(installer, _) => {
                        ui.label(format!(
                            "RedBeam {} está listo para instalar.",
                            installer.version
                        ));
                        ui.label("Guarda tus planos. Al abrir el instalador se cerrará RedBeam.");
                        install = ui.button("Abrir instalador").clicked();
                    }
                    Update::Current => {
                        ui.label("Ya tienes la versión más reciente.");
                    }
                    Update::NoRelease => {
                        ui.label("Aún no hay una versión publicada en GitHub.");
                    }
                    Update::Unavailable | Update::Unknown => {
                        ui.label("No se pudo comprobar si hay actualizaciones.");
                        ui.hyperlink_to("Ver versiones en GitHub", RELEASES);
                        retry = ui.button("Volver a intentar").clicked();
                    }
                }
                ui.separator();
                close = ui.button("Cerrar").clicked();
            });
        if let Some(installer) = download_request {
            self.start(ctx, Update::Downloading, move || {
                match download(&installer) {
                    Some(path) => Update::Ready(installer, path),
                    None => Update::DownloadFailed(installer),
                }
            });
        } else if retry {
            self.start(ctx, Update::Checking, fetch_update);
        }
        if close {
            self.visible = false;
        }
        install
    }

    pub(crate) fn launch(&mut self) -> Result<(), &'static str> {
        let (Update::Ready(installer, path), Some(install_dir)) = (&self.state, &self.install_dir)
        else {
            return Err("La actualización aún no está lista.");
        };
        let mut bytes = Vec::new();
        let valid = std::fs::File::open(path)
            .and_then(|file| file.take(installer.size + 1).read_to_end(&mut bytes))
            .is_ok()
            && installer.valid(&bytes);
        if !valid {
            self.state = Update::DownloadFailed(installer.clone());
            return Err("Vuelve a descargar el instalador. No pasó la verificación.");
        }
        if !open_installer(path, install_dir) {
            return Err("No se pudo abrir el instalador de RedBeam.");
        }
        self.visible = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> Release {
        Release {
            tag_name: tag.into(),
            assets: vec![Asset {
                name: INSTALLER_NAME.into(),
                state: "uploaded".into(),
                size: 3,
                digest: Some(format!("sha256:{}", hex::encode(Sha256::digest(b"abc")))),
            }],
        }
    }

    #[test]
    fn verifies_a_newer_stable_installer() {
        let current = Version::new(0, 2, 9);
        let Some(Update::Available(installer)) = version_available(release("v0.2.10"), &current)
        else {
            panic!("expected a newer installer");
        };
        assert_eq!(installer.version, Version::new(0, 2, 10));
        assert!(installer.valid(b"abc"));
        assert!(!installer.valid(b"xyz"));
        assert!(!installer.valid(b"abcd"));
        for tag in ["v0.1.0", "v0.2.9"] {
            assert!(matches!(
                version_available(release(tag), &current),
                Some(Update::Current)
            ));
        }
        for tag in ["v0.2.10-beta.1", "invalid"] {
            assert!(version_available(release(tag), &current).is_none());
        }
    }

    #[test]
    fn rejects_unverified_assets() {
        let current = Version::new(0, 2, 9);
        let mut missing = release("v0.2.10");
        missing.assets[0].digest = None;
        assert!(version_available(missing, &current).is_none());

        let mut oversized = release("v0.2.10");
        oversized.assets[0].size = MAX_SIZE + 1;
        assert!(version_available(oversized, &current).is_none());

        let mut incomplete = release("v0.2.10");
        incomplete.assets[0].state = "starter".into();
        assert!(version_available(incomplete, &current).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn downloaded_file_keeps_its_windows_security_zone_after_rename() {
        let directory = tempfile::tempdir().unwrap();
        let source = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        mark_download(source.path(), RELEASES).unwrap();
        let path = directory.path().join(INSTALLER_NAME);
        source.persist(&path).unwrap();
        let mut zone = path.into_os_string();
        zone.push(":Zone.Identifier");
        assert!(std::fs::read_to_string(zone).unwrap().contains("ZoneId=3"));
    }
}
