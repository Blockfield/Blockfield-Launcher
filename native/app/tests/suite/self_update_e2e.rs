//! End-to-end signed self-update and rollback tests for the native launcher.
//!
//! Exercises the real packaging (`scripts/package-native-deb.py`), manifest
//! generation (`scripts/prepare-updater.py`), minisign signing/verify with
//! version pinning, HTTP fetch, tamper rejection, `.deb` extraction, AppImage
//! in-place replacement, atomic `.previous` backup, and rollback.

use std::path::{Path, PathBuf};
use std::process::Command;

use blockfield_native::update::self_update;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("native/app sits two levels under repo root")
        .to_path_buf()
}

fn python_bin() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}

#[test]
fn prepare_updater_manifest_parses_in_check_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let release_dir = tmp.path().join("release");
    std::fs::create_dir_all(&release_dir).unwrap();
    let version = "2.0.99";
    for name in [
        format!("blockfield-launcher_{version}_amd64.AppImage"),
        format!("blockfield-launcher_{version}_amd64.deb"),
        format!("blockfield-launcher_{version}_x64_en-US.msi"),
        format!("blockfield-launcher_{version}_x64-setup.exe"),
    ] {
        std::fs::write(release_dir.join(&name), b"artifact").unwrap();
        std::fs::write(release_dir.join(format!("{name}.sig")), "c2ln\n").unwrap();
    }

    let script = repo_root().join("scripts").join("prepare-updater.py");
    let out = Command::new(python_bin())
        .arg(&script)
        .arg(&release_dir)
        .arg(version)
        .output()
        .expect("run prepare-updater.py");
    assert!(
        out.status.success(),
        "prepare-updater.py failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let manifest = std::fs::read_to_string(release_dir.join("latest.json")).unwrap();
    let linux_appimage = self_update::check_manifest(
        &manifest,
        &["linux-x86_64-appimage".into(), "linux-x86_64".into()],
        "2.0.0",
    )
    .unwrap()
    .expect("linux AppImage release available");
    assert_eq!(linux_appimage.version, version);
    assert!(linux_appimage.url.ends_with(&format!(
        "/launcher-v{version}/blockfield-launcher_{version}_amd64.AppImage"
    )));

    let linux_deb = self_update::check_manifest(&manifest, &["linux-x86_64-deb".into()], "2.0.0")
        .unwrap()
        .expect("linux deb release available");
    assert_eq!(linux_deb.version, version);
    assert!(linux_deb.url.ends_with(&format!(
        "/launcher-v{version}/blockfield-launcher_{version}_amd64.deb"
    )));

    let windows = self_update::check_manifest(
        &manifest,
        &["windows-x86_64".into(), "windows-x86_64-nsis".into()],
        "2.0.0",
    )
    .unwrap()
    .expect("windows release available");
    assert_eq!(windows.version, version);
    assert!(windows.url.ends_with(&format!(
        "/launcher-v{version}/blockfield-launcher_{version}_x64_en-US.msi"
    )));
}

#[cfg(target_os = "linux")]
mod linux_e2e {
    use super::*;
    use std::collections::HashMap;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    /// Generate an ephemeral Ed25519 keypair via `openssl` and sign `artifact`
    /// in Tauri's prehashed (`ED`) outer-base64 minisign format with a pinned
    /// `version:<ver>` trusted comment. Nothing secret is committed to git.
    fn sign_artifact_ephemeral(
        work_dir: &Path,
        artifact: &Path,
        version: &str,
    ) -> (String, String) {
        let helper = r#"
import base64, hashlib, subprocess, sys
from pathlib import Path

work = Path(sys.argv[1])
artifact = Path(sys.argv[2])
version = sys.argv[3]

priv = work / "ephemeral.pem"
subprocess.run(["openssl", "genpkey", "-algorithm", "ED25519", "-out", str(priv)], check=True)
pub_der = subprocess.check_output(["openssl", "pkey", "-in", str(priv), "-pubout", "-outform", "DER"])
pub_raw = pub_der[-32:]
key_id = b"\x11\x22\x33\x44\x55\x66\x77\x88"

pub_line = base64.b64encode(b"Ed" + key_id + pub_raw).decode("ascii")
pub_text = f"untrusted comment: minisign public key: 8877665544332211\n{pub_line}\n"
pub_b64 = base64.b64encode(pub_text.encode("utf-8")).decode("ascii")

prehash = hashlib.blake2b(artifact.read_bytes(), digest_size=64).digest()
prehash_file = work / "prehash.bin"
prehash_file.write_bytes(prehash)
sig1 = subprocess.check_output([
    "openssl", "pkeyutl", "-sign", "-inkey", str(priv), "-rawin", "-in", str(prehash_file)
])

trusted = f"timestamp:1790000000\tfile:{artifact.name}\tversion:{version}"
global_input = work / "global.bin"
global_input.write_bytes(sig1 + trusted.encode("utf-8"))
global_sig = subprocess.check_output([
    "openssl", "pkeyutl", "-sign", "-inkey", str(priv), "-rawin", "-in", str(global_input)
])

sig_text = (
    "untrusted comment: signature from ephemeral test key\n"
    + base64.b64encode(b"ED" + key_id + sig1).decode("ascii")
    + "\n"
    + f"trusted comment: {trusted}\n"
    + base64.b64encode(global_sig).decode("ascii")
    + "\n"
)
sig_b64 = base64.b64encode(sig_text.encode("utf-8")).decode("ascii")
print(pub_b64)
print(sig_b64)
"#;
        let out = Command::new("python3")
            .arg("-c")
            .arg(helper)
            .arg(work_dir)
            .arg(artifact)
            .arg(version)
            .output()
            .expect("sign artifact with ephemeral openssl key");
        assert!(
            out.status.success(),
            "ephemeral signer failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8(out.stdout).unwrap();
        let mut lines = stdout.lines();
        let pubkey = lines.next().expect("pubkey line").trim().to_string();
        let sig = lines.next().expect("sig line").trim().to_string();
        (pubkey, sig)
    }

    struct StubServer {
        base_url: String,
        stop: Arc<AtomicBool>,
        addr: std::net::SocketAddr,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl StubServer {
        fn start(build_routes: impl FnOnce(&str) -> HashMap<String, Vec<u8>>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let addr = listener.local_addr().unwrap();
            let base_url = format!("http://{addr}");
            let routes = build_routes(&base_url);
            let stop = Arc::new(AtomicBool::new(false));
            let stop_flag = stop.clone();
            let thread = std::thread::spawn(move || {
                for incoming in listener.incoming() {
                    if stop_flag.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(mut stream) = incoming else { break };
                    let mut buf = [0u8; 4096];
                    let Ok(n) = stream.read(&mut buf) else {
                        continue;
                    };
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let path = req
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/");
                    let (status, body) = match routes.get(path) {
                        Some(data) => ("200 OK", data.as_slice()),
                        None => ("404 Not Found", b"".as_slice()),
                    };
                    let head = format!(
                        "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(body);
                }
            });
            Self {
                base_url,
                stop,
                addr,
                thread: Some(thread),
            }
        }
    }

    impl Drop for StubServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            let _ = std::net::TcpStream::connect(self.addr);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
            .block_on(future)
    }

    fn fake_appimage(tag: &[u8]) -> Vec<u8> {
        let machine: u16 = if cfg!(target_arch = "aarch64") {
            183
        } else {
            62
        };
        let mut bytes = vec![0u8; 64];
        bytes[..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[8..11].copy_from_slice(b"AI\x02");
        bytes[18..20].copy_from_slice(&machine.to_le_bytes());
        bytes.extend_from_slice(tag);
        bytes
    }

    #[test]
    fn signed_deb_self_update_tamper_reject_and_rollback_e2e() {
        let tmp = tempfile::tempdir().unwrap();
        let install_dir = tmp.path().join("install");
        let release_dir = tmp.path().join("release");
        let keys_dir = tmp.path().join("keys");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(&release_dir).unwrap();
        std::fs::create_dir_all(&keys_dir).unwrap();

        let v1_bytes = std::fs::read("/bin/true").unwrap();
        let v2_bytes = std::fs::read("/bin/false").unwrap();
        assert_ne!(v1_bytes, v2_bytes);

        let installed_exe = install_dir.join("blockfield-launcher");
        std::fs::write(&installed_exe, &v1_bytes).unwrap();
        std::fs::set_permissions(&installed_exe, std::fs::Permissions::from_mode(0o755)).unwrap();

        // 1. Package real .deb containing v2 via scripts/package-native-deb.py.
        let version = "2.0.1";
        let deb_out = Command::new("python3")
            .arg(repo_root().join("scripts/package-native-deb.py"))
            .args(["--binary", "/bin/false", "--version", version, "--output"])
            .arg(&release_dir)
            .output()
            .expect("run package-native-deb.py");
        assert!(
            deb_out.status.success(),
            "package-native-deb.py failed: {}",
            String::from_utf8_lossy(&deb_out.stderr)
        );
        let deb_name = format!("blockfield-launcher_{version}_amd64.deb");
        let deb_path = release_dir.join(&deb_name);
        assert!(deb_path.is_file(), "expected packaged deb at {deb_path:?}");
        let deb_bytes = std::fs::read(&deb_path).unwrap();

        // 2. Sign the real .deb with an ephemeral Ed25519 key (prehashed ED + version pin).
        let (pubkey_b64, sig_b64) = sign_artifact_ephemeral(&keys_dir, &deb_path, version);
        std::fs::write(
            release_dir.join(format!("{deb_name}.sig")),
            format!("{sig_b64}\n"),
        )
        .unwrap();

        // Stub sibling assets so prepare-updater.py validates the complete 4-asset set.
        for sibling in [
            format!("blockfield-launcher_{version}_amd64.AppImage"),
            format!("blockfield-launcher_{version}_x64_en-US.msi"),
            format!("blockfield-launcher_{version}_x64-setup.exe"),
        ] {
            std::fs::write(release_dir.join(&sibling), b"stub-sibling").unwrap();
            std::fs::write(release_dir.join(format!("{sibling}.sig")), "c3R1Yg==\n").unwrap();
        }

        // 3. Build latest.json via scripts/prepare-updater.py.
        let prep_out = Command::new("python3")
            .arg(repo_root().join("scripts/prepare-updater.py"))
            .arg(&release_dir)
            .arg(version)
            .output()
            .expect("run prepare-updater.py");
        assert!(
            prep_out.status.success(),
            "prepare-updater.py failed: {}",
            String::from_utf8_lossy(&prep_out.stderr)
        );
        let mut manifest_json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(release_dir.join("latest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest_json["version"], version);
        assert_eq!(
            manifest_json["platforms"]["linux-x86_64-deb"]["url"],
            format!(
                "https://github.com/Blockfield/Blockfield-Launcher/releases/download/launcher-v{version}/{deb_name}"
            )
        );

        // 4. Serve manifest, genuine .deb and tampered .deb over local HTTP.
        let mut tampered_deb = deb_bytes.clone();
        let last = tampered_deb.len() - 1;
        tampered_deb[last] ^= 0xff;

        let manifest_path = "/latest.json";
        let deb_route = format!("/{deb_name}");
        let tampered_route = "/tampered.deb";

        let server = StubServer::start(|base| {
            manifest_json["platforms"]["linux-x86_64-deb"]["url"] =
                serde_json::Value::String(format!("{base}{deb_route}"));
            HashMap::from([
                (
                    manifest_path.to_string(),
                    serde_json::to_vec(&manifest_json).unwrap(),
                ),
                (deb_route.clone(), deb_bytes.clone()),
                (tampered_route.to_string(), tampered_deb),
            ])
        });
        let manifest_url = format!("{}{manifest_path}", server.base_url);
        let tampered_url = format!("{}{tampered_route}", server.base_url);

        // 5. Without `.blockfield-native-install`, update check is refused.
        assert!(
            block_on(self_update::check_for_update_at(
                &manifest_url,
                &installed_exe,
                "2.0.0"
            ))
            .is_err()
        );

        // 6. Add marker -> check_for_update_at discovers 2.0.1 (and 2.0.1 sees None).
        std::fs::write(install_dir.join(".blockfield-native-install"), b"1\n").unwrap();
        let release = block_on(self_update::check_for_update_at(
            &manifest_url,
            &installed_exe,
            "2.0.0",
        ))
        .unwrap()
        .expect("update 2.0.1 must be offered");
        assert_eq!(release.version, version);
        assert!(
            block_on(self_update::check_for_update_at(
                &manifest_url,
                &installed_exe,
                version
            ))
            .unwrap()
            .is_none()
        );

        // 7. Tampered .deb must fail signature verification without touching binary or backup.
        let mut tampered_release = release.clone();
        tampered_release.url = tampered_url;
        let err = block_on(self_update::install_release_with(
            &tampered_release,
            &pubkey_b64,
            installed_exe.clone(),
            |_, _| {},
            |_| {},
        ))
        .unwrap_err();
        assert!(
            err.contains("signature"),
            "expected signature rejection, got: {err}"
        );
        assert_eq!(std::fs::read(&installed_exe).unwrap(), v1_bytes);
        assert!(!self_update::backup_path(&installed_exe).unwrap().exists());

        // 8. Repaired manifest with wrong version must fail signed version pin.
        let mut wrong_version = release.clone();
        wrong_version.version = "2.0.2".into();
        let err = block_on(self_update::install_release_with(
            &wrong_version,
            &pubkey_b64,
            installed_exe.clone(),
            |_, _| {},
            |_| {},
        ))
        .unwrap_err();
        assert!(
            err.contains("different version"),
            "expected version-pin rejection, got: {err}"
        );
        assert_eq!(std::fs::read(&installed_exe).unwrap(), v1_bytes);

        // 9. Full signed self-update: HTTP download → verify → extract .deb → atomic replace.
        let mut max_downloaded = 0u64;
        let mut stages = Vec::new();
        block_on(self_update::install_release_with(
            &release,
            &pubkey_b64,
            installed_exe.clone(),
            |downloaded, _total| max_downloaded = max_downloaded.max(downloaded),
            |stage| {
                stages.push(match stage {
                    self_update::InstallStage::Verifying => "verifying",
                    self_update::InstallStage::Installing => "installing",
                });
            },
        ))
        .unwrap();

        assert_eq!(max_downloaded, deb_bytes.len() as u64);
        assert_eq!(stages, vec!["verifying", "installing"]);
        assert_eq!(std::fs::read(&installed_exe).unwrap(), v2_bytes);
        let mode = std::fs::metadata(&installed_exe)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o755);

        let backup = self_update::backup_path(&installed_exe).unwrap();
        assert_eq!(backup.file_name().unwrap(), "blockfield-launcher.previous");
        assert_eq!(std::fs::read(&backup).unwrap(), v1_bytes);

        // 10. Rollback restores original v1 binary atomically.
        self_update::rollback(&installed_exe).unwrap();
        assert_eq!(std::fs::read(&installed_exe).unwrap(), v1_bytes);
    }

    #[test]
    fn signed_appimage_self_update_tamper_reject_and_rollback_e2e() {
        let tmp = tempfile::tempdir().unwrap();
        let install_dir = tmp.path().join("Applications");
        let release_dir = tmp.path().join("release");
        let keys_dir = tmp.path().join("keys");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(&release_dir).unwrap();
        std::fs::create_dir_all(&keys_dir).unwrap();

        let v1_bytes = fake_appimage(b"SQUASHFS_V1");
        let v2_bytes = fake_appimage(b"SQUASHFS_V2");

        let installed_appimage = install_dir.join("blockfieldlauncher.appimage");
        std::fs::write(&installed_appimage, &v1_bytes).unwrap();
        std::fs::set_permissions(&installed_appimage, std::fs::Permissions::from_mode(0o755))
            .unwrap();

        let version = "2.0.1";
        let appimage_name = format!("blockfield-launcher_{version}_amd64.AppImage");
        let appimage_path = release_dir.join(&appimage_name);
        std::fs::write(&appimage_path, &v2_bytes).unwrap();
        let (pubkey_b64, sig_b64) = sign_artifact_ephemeral(&keys_dir, &appimage_path, version);
        std::fs::write(
            release_dir.join(format!("{appimage_name}.sig")),
            format!("{sig_b64}\n"),
        )
        .unwrap();

        for sibling in [
            format!("blockfield-launcher_{version}_amd64.deb"),
            format!("blockfield-launcher_{version}_x64_en-US.msi"),
            format!("blockfield-launcher_{version}_x64-setup.exe"),
        ] {
            std::fs::write(release_dir.join(&sibling), b"stub-sibling").unwrap();
            std::fs::write(release_dir.join(format!("{sibling}.sig")), "c3R1Yg==\n").unwrap();
        }

        let prep_out = Command::new("python3")
            .arg(repo_root().join("scripts/prepare-updater.py"))
            .arg(&release_dir)
            .arg(version)
            .output()
            .expect("run prepare-updater.py");
        assert!(
            prep_out.status.success(),
            "prepare-updater.py failed: {}",
            String::from_utf8_lossy(&prep_out.stderr)
        );
        let mut manifest_json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(release_dir.join("latest.json")).unwrap(),
        )
        .unwrap();

        let mut tampered = v2_bytes.clone();
        *tampered.last_mut().unwrap() ^= 0xff;
        let appimage_route = format!("/{appimage_name}");
        let tampered_route = "/tampered.AppImage";
        let server = StubServer::start(|base| {
            let url = serde_json::Value::String(format!("{base}{appimage_route}"));
            manifest_json["platforms"]["linux-x86_64"]["url"] = url.clone();
            manifest_json["platforms"]["linux-x86_64-appimage"]["url"] = url;
            HashMap::from([
                (
                    "/latest.json".to_string(),
                    serde_json::to_vec(&manifest_json).unwrap(),
                ),
                (appimage_route.clone(), v2_bytes.clone()),
                (tampered_route.to_string(), tampered),
            ])
        });
        let manifest_url = format!("{}/latest.json", server.base_url);

        // AppImage needs no .blockfield-native-install marker.
        let release = block_on(self_update::check_for_update_at(
            &manifest_url,
            &installed_appimage,
            "2.0.0",
        ))
        .unwrap()
        .expect("AppImage update 2.0.1 must be offered");
        assert_eq!(release.version, version);

        // Tampered AppImage rejected cleanly.
        let mut bad = release.clone();
        bad.url = format!("{}{tampered_route}", server.base_url);
        assert!(
            block_on(self_update::install_release_with(
                &bad,
                &pubkey_b64,
                installed_appimage.clone(),
                |_, _| {},
                |_| {},
            ))
            .is_err()
        );
        assert_eq!(std::fs::read(&installed_appimage).unwrap(), v1_bytes);

        // Valid signed AppImage replaces in place with backup and supports rollback.
        block_on(self_update::install_release_with(
            &release,
            &pubkey_b64,
            installed_appimage.clone(),
            |_, _| {},
            |_| {},
        ))
        .unwrap();
        assert_eq!(std::fs::read(&installed_appimage).unwrap(), v2_bytes);
        let backup = self_update::backup_path(&installed_appimage).unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), v1_bytes);

        self_update::rollback(&installed_appimage).unwrap();
        assert_eq!(std::fs::read(&installed_appimage).unwrap(), v1_bytes);
    }
}
