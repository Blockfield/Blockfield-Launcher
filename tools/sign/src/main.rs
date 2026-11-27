use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use minisign::SecretKeyBox;

fn decode_secret_key_box(raw: &str) -> Result<SecretKeyBox, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("TAURI_SIGNING_PRIVATE_KEY is empty".into());
    }
    let text = if trimmed.starts_with("untrusted comment:") {
        trimmed.to_owned()
    } else if Path::new(trimmed).is_file() {
        let content = fs::read_to_string(trimmed)
            .map_err(|e| format!("Cannot read private key file {trimmed}: {e}"))?;
        let content_trimmed = content.trim();
        if content_trimmed.starts_with("untrusted comment:") {
            content_trimmed.to_owned()
        } else {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(content_trimmed)
                .map_err(|e| format!("Invalid base64 private key file: {e}"))?;
            String::from_utf8(decoded).map_err(|_| "Private key is not valid UTF-8".to_string())?
        }
    } else {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(trimmed)
            .map_err(|e| format!("Invalid base64 private key: {e}"))?;
        String::from_utf8(decoded).map_err(|_| "Private key is not valid UTF-8".to_string())?
    };
    SecretKeyBox::from_string(&text).map_err(|e| format!("Invalid minisign secret key: {e}"))
}

fn sign_artifact(
    artifact: &Path,
    app_version: &str,
    raw_key: &str,
    password: Option<String>,
) -> Result<String, String> {
    let file_name = artifact
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("Invalid artifact file name: {}", artifact.display()))?;
    let sk_box = decode_secret_key_box(raw_key)?;
    let sk = match sk_box.into_unencrypted_secret_key() {
        Ok(sk) => sk,
        Err(_) => decode_secret_key_box(raw_key)?
            .into_secret_key(password.or_else(|| Some(String::new())))
            .map_err(|e| format!("Cannot decrypt minisign secret key: {e}"))?,
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("System clock before UNIX epoch: {e}"))?
        .as_secs();
    let trusted_comment = format!("timestamp:{timestamp}\tfile:{file_name}\tversion:{app_version}");
    let reader = BufReader::new(
        File::open(artifact)
            .map_err(|e| format!("Cannot open artifact {}: {e}", artifact.display()))?,
    );
    let sig_box = minisign::sign(
        None,
        &sk,
        reader,
        Some(trusted_comment.as_str()),
        Some("signature from blockfield release key"),
    )
    .map_err(|e| format!("Signing failed: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(sig_box.into_string().as_bytes()))
}

fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<(String, PathBuf), String> {
    let mut app_version = None;
    let mut artifact = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--app-version" => {
                app_version = Some(iter.next().ok_or("--app-version requires a value")?);
            }
            _ if arg.starts_with('-') => return Err(format!("Unknown argument: {arg}")),
            _ => {
                if artifact.is_some() {
                    return Err(format!("Unexpected extra argument: {arg}"));
                }
                artifact = Some(PathBuf::from(arg));
            }
        }
    }
    let version = app_version
        .filter(|v| !v.trim().is_empty())
        .ok_or("Missing required --app-version <VERSION>")?;
    let path = artifact.ok_or("Missing required <ARTIFACT_PATH>")?;
    Ok((version, path))
}

fn main() {
    let (version, artifact) = match parse_args(std::env::args().skip(1)) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("Usage: blockfield-sign --app-version <VERSION> <ARTIFACT_PATH>\n{error}");
            std::process::exit(2);
        }
    };
    let raw_key = match std::env::var("TAURI_SIGNING_PRIVATE_KEY") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("TAURI_SIGNING_PRIVATE_KEY environment variable is required");
            std::process::exit(1);
        }
    };
    let password = std::env::var("TAURI_SIGNING_PRIVATE_KEY_PASSWORD").ok();
    let sig_b64 = match sign_artifact(&artifact, &version, &raw_key, password) {
        Ok(sig) => sig,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let mut sig_path = artifact.into_os_string();
    sig_path.push(".sig");
    let sig_path = PathBuf::from(sig_path);
    if let Err(error) = fs::write(&sig_path, format!("{sig_b64}\n")) {
        eprintln!("Cannot write {}: {error}", sig_path.display());
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verify_with_updater_logic(
        data: &[u8],
        sig_b64: &str,
        pubkey_b64: &str,
        expected_version: &str,
        expected_file: &str,
    ) -> Result<(), String> {
        let sig_bytes = base64::engine::general_purpose::STANDARD
            .decode(sig_b64.trim())
            .map_err(|e| e.to_string())?;
        let sig_text = std::str::from_utf8(&sig_bytes).map_err(|e| e.to_string())?;
        let signature = minisign_verify::Signature::decode(sig_text).map_err(|e| e.to_string())?;

        let key_bytes = base64::engine::general_purpose::STANDARD
            .decode(pubkey_b64.trim())
            .map_err(|e| e.to_string())?;
        let key_text = std::str::from_utf8(&key_bytes).map_err(|e| e.to_string())?;
        let public_key = minisign_verify::PublicKey::decode(key_text).map_err(|e| e.to_string())?;

        public_key
            .verify(data, &signature, true)
            .map_err(|e| e.to_string())?;
        let comment = signature.trusted_comment();
        let version = comment
            .split('\t')
            .find_map(|field| field.strip_prefix("version:"));
        let file = comment
            .split('\t')
            .find_map(|field| field.strip_prefix("file:"));
        if version != Some(expected_version) {
            return Err(format!("version mismatch: {version:?}"));
        }
        if file != Some(expected_file) {
            return Err(format!("file mismatch: {file:?}"));
        }
        Ok(())
    }

    #[test]
    fn signs_encrypted_outer_base64_key_for_updater_verification() {
        let tmp = tempfile::tempdir().unwrap();
        let artifact = tmp.path().join("blockfield-launcher_2.0.1_amd64.AppImage");
        let payload = b"native slint appimage payload";
        fs::write(&artifact, payload).unwrap();

        let kp = minisign::KeyPair::generate_encrypted_keypair(Some("test-pass".into())).unwrap();
        let sk_text = kp.sk.to_box(None).unwrap().into_string();
        let pk_text = kp.pk.to_box().unwrap().into_string();
        let sk_b64 = base64::engine::general_purpose::STANDARD.encode(sk_text.as_bytes());
        let pk_b64 = base64::engine::general_purpose::STANDARD.encode(pk_text.as_bytes());

        let sig_b64 = sign_artifact(&artifact, "2.0.1", &sk_b64, Some("test-pass".into())).unwrap();
        verify_with_updater_logic(
            payload,
            &sig_b64,
            &pk_b64,
            "2.0.1",
            "blockfield-launcher_2.0.1_amd64.AppImage",
        )
        .unwrap();

        // Wrong password must fail decryption.
        assert!(sign_artifact(&artifact, "2.0.1", &sk_b64, Some("wrong".into())).is_err());

        // Tampered payload must fail verification.
        let mut tampered = payload.to_vec();
        tampered[0] ^= 1;
        assert!(verify_with_updater_logic(
            &tampered,
            &sig_b64,
            &pk_b64,
            "2.0.1",
            "blockfield-launcher_2.0.1_amd64.AppImage",
        )
        .is_err());
    }

    #[test]
    fn accepts_unencrypted_raw_key_and_key_file_path() {
        let tmp = tempfile::tempdir().unwrap();
        let artifact = tmp.path().join("launcher.deb");
        let payload = b"deb bytes";
        fs::write(&artifact, payload).unwrap();

        let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let sk_text = kp.sk.to_box(None).unwrap().into_string();
        let pk_text = kp.pk.to_box().unwrap().into_string();
        let pk_b64 = base64::engine::general_purpose::STANDARD.encode(pk_text.as_bytes());

        // Raw minisign text works directly.
        let sig_raw = sign_artifact(&artifact, "2.1.0", &sk_text, None).unwrap();
        verify_with_updater_logic(payload, &sig_raw, &pk_b64, "2.1.0", "launcher.deb").unwrap();

        // Key file path also works.
        let key_file = tmp.path().join("release.key");
        fs::write(&key_file, &sk_text).unwrap();
        let sig_file = sign_artifact(&artifact, "2.1.0", key_file.to_str().unwrap(), None).unwrap();
        verify_with_updater_logic(payload, &sig_file, &pk_b64, "2.1.0", "launcher.deb").unwrap();
    }
}
