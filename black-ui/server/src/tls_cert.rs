use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use rcgen::{CertificateParams, DnType, KeyPair};
use time::{Duration, OffsetDateTime};

use crate::models::{TlsSelfSignedInput, TlsSelfSignedResult};

const DEFAULT_CERT_DIR: &str = "/etc/blackwire/certs";
const MAX_DAYS: u16 = 3650;

pub fn generate_self_signed(input: TlsSelfSignedInput) -> Result<TlsSelfSignedResult, String> {
    generate_self_signed_inner(input).map_err(|error| error.to_string())
}

fn generate_self_signed_inner(input: TlsSelfSignedInput) -> Result<TlsSelfSignedResult> {
    let server_name = normalize_server_name(&input.server_name)?;
    let days = normalize_days(input.days);
    let cert_dir = cert_dir();
    fs::create_dir_all(&cert_dir).with_context(|| {
        format!(
            "failed to create certificate directory {}",
            cert_dir.display()
        )
    })?;
    set_dir_permissions(&cert_dir)?;

    let file_base = sanitize_file_name(&server_name);
    let certificate_file = cert_dir.join(format!("{file_base}.crt"));
    let key_file = cert_dir.join(format!("{file_base}.key"));

    let signing_key =
        KeyPair::generate().context("failed to generate self-signed TLS private key")?;
    let mut params = CertificateParams::new(vec![server_name.clone()])
        .context("failed to prepare self-signed TLS certificate parameters")?;
    params.not_before = OffsetDateTime::now_utc() - Duration::minutes(5);
    params.not_after = params.not_before + Duration::days(i64::from(days));
    params
        .distinguished_name
        .push(DnType::CommonName, &server_name);
    let cert = params
        .self_signed(&signing_key)
        .context("failed to generate self-signed TLS certificate")?;
    write_secret_file(&key_file, signing_key.serialize_pem().as_bytes())?;
    write_cert_file(&certificate_file, cert.pem().as_bytes())?;

    Ok(TlsSelfSignedResult {
        server_name,
        certificate_file: certificate_file.display().to_string(),
        key_file: key_file.display().to_string(),
        days,
    })
}

fn normalize_server_name(value: &str) -> Result<String> {
    let value = value.trim().trim_start_matches('[').trim_end_matches(']');
    if value.is_empty() {
        return Err(anyhow!("serverName is required"));
    }
    if value.contains('*') {
        return Err(anyhow!(
            "wildcard serverName is not supported for generated self-signed certificates"
        ));
    }
    if value.contains('/') || value.contains('\\') || value.contains('\0') {
        return Err(anyhow!(
            "serverName must be a DNS name or IP address, not a path"
        ));
    }
    Ok(value.to_string())
}

fn normalize_days(days: i64) -> u16 {
    days.clamp(1, i64::from(MAX_DAYS)) as u16
}

fn cert_dir() -> PathBuf {
    std::env::var("BLACK_UI_CERT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CERT_DIR))
}

fn sanitize_file_name(value: &str) -> String {
    let sanitized = value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "tls-self-signed".into()
    } else {
        sanitized
    }
}

fn write_secret_file(path: &Path, bytes: &[u8]) -> Result<()> {
    write_file(path, bytes, 0o640)
}

fn write_cert_file(path: &Path, bytes: &[u8]) -> Result<()> {
    write_file(path, bytes, 0o640)
}

fn write_file(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let tmp_path = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .with_context(|| format!("failed to create {}", tmp_path.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("failed to write {}", tmp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", tmp_path.display()))?;
    }
    set_file_permissions(&tmp_path, mode)?;
    fs::rename(&tmp_path, path).with_context(|| format!("failed to install {}", path.display()))?;
    Ok(())
}

#[cfg(unix)]
fn set_dir_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match fs::set_permissions(path, fs::Permissions::from_mode(0o770)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to protect {}", path.display())),
    }
}

#[cfg(not(unix))]
fn set_dir_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_file_permissions(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("failed to protect {}", path.display()))
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_certificate_and_key_under_configured_directory() {
        let dir = std::env::temp_dir().join(format!("black-ui-cert-test-{}", uuid::Uuid::new_v4()));
        std::env::set_var("BLACK_UI_CERT_DIR", &dir);

        let generated = generate_self_signed_inner(TlsSelfSignedInput {
            server_name: "203.0.113.10".into(),
            days: 999999,
        })
        .unwrap();

        assert_eq!(generated.server_name, "203.0.113.10");
        assert_eq!(generated.days, MAX_DAYS);
        assert!(generated
            .certificate_file
            .starts_with(&dir.display().to_string()));
        assert!(generated.key_file.starts_with(&dir.display().to_string()));
        assert!(fs::read_to_string(&generated.certificate_file)
            .unwrap()
            .contains("BEGIN CERTIFICATE"));
        assert!(fs::read_to_string(&generated.key_file)
            .unwrap()
            .contains("BEGIN PRIVATE KEY"));
        assert_unix_mode(&dir, 0o770);
        assert_unix_mode(Path::new(&generated.certificate_file), 0o640);
        assert_unix_mode(Path::new(&generated.key_file), 0o640);

        let _ = fs::remove_dir_all(&dir);
        std::env::remove_var("BLACK_UI_CERT_DIR");
    }

    #[test]
    fn rejects_wildcards_and_paths() {
        assert!(generate_self_signed_inner(TlsSelfSignedInput {
            server_name: "*.example.com".into(),
            days: 365,
        })
        .is_err());
        assert!(generate_self_signed_inner(TlsSelfSignedInput {
            server_name: "../bad".into(),
            days: 365,
        })
        .is_err());
    }

    #[test]
    fn clamps_non_positive_validity_days() {
        assert_eq!(normalize_days(0), 1);
        assert_eq!(normalize_days(-42), 1);
    }

    #[cfg(unix)]
    fn assert_unix_mode(path: &Path, expected: u32) {
        use std::os::unix::fs::PermissionsExt;
        let actual = fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(actual, expected, "unexpected mode for {}", path.display());
    }

    #[cfg(not(unix))]
    fn assert_unix_mode(_path: &Path, _expected: u32) {}
}
