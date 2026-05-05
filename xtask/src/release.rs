use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rand_core::OsRng;
use rsa::{
    BigUint, RsaPrivateKey, RsaPublicKey,
    pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey, EncodeRsaPrivateKey, EncodeRsaPublicKey},
    pkcs1v15,
    pkcs8::LineEnding,
    signature::{SignatureEncoding, Signer, Verifier},
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

type DynResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Default)]
struct WindowsReleaseOptions {
    version: Option<String>,
    output_dir: PathBuf,
    build: bool,
    try_installer: bool,
    code_sign: bool,
    certificate_path: Option<String>,
    certificate_password: Option<String>,
    certificate_base64: Option<String>,
    timestamp_server: String,
    repository: Option<String>,
}

#[derive(Debug)]
struct VerifyOptions {
    file: PathBuf,
    signature: Option<PathBuf>,
    public_key: PathBuf,
}

#[derive(Debug)]
struct MacosOptions {
    version: Option<String>,
    arch: Option<String>,
    app_path: Option<PathBuf>,
    output_pkg: Option<PathBuf>,
    output_dmg: Option<PathBuf>,
    skip_pkg: bool,
    skip_dmg: bool,
}

#[derive(Serialize)]
struct ReleaseAsset {
    name: String,
    url: String,
    signature: String,
    sha256: String,
    size: u64,
}

#[derive(Serialize)]
struct PlatformAssets {
    assets: Vec<ReleaseAsset>,
}

#[derive(Serialize)]
struct SignatureInfo {
    algorithm: &'static str,
    public_key: &'static str,
    format: &'static str,
}

#[derive(Serialize)]
struct LatestJson {
    name: &'static str,
    version: String,
    pub_date: String,
    notes: String,
    update_protocol: u8,
    minimum_supported_version: &'static str,
    platforms: BTreeMap<String, PlatformAssets>,
    signature: SignatureInfo,
}

pub fn run_release(root: &Path, args: &[String]) -> DynResult<()> {
    match args {
        [command, rest @ ..] if command == "windows" => release_windows(root, rest),
        [command, rest @ ..] if command == "verify" => verify_release_signature(root, rest),
        _ => Err("unknown release command".into()),
    }
}

pub fn run_package(root: &Path, args: &[String]) -> DynResult<()> {
    match args {
        [command, rest @ ..] if command == "macos" => package_macos(root, rest),
        _ => Err("unknown package command".into()),
    }
}

fn release_windows(root: &Path, args: &[String]) -> DynResult<()> {
    let options = parse_windows_options(args)?;
    let version = options.version.clone().unwrap_or(package_version(root)?);
    let release_dir = resolve_from_root(root, &options.output_dir);
    let dist_dir = root.join("dist");
    let tauri_release_dir = root.join("src-tauri/target/release");

    env::set_current_dir(root)?;
    fs::create_dir_all(&release_dir)?;
    clean_release_dir(&release_dir)?;

    if options.build {
        invoke_tauri_build(root, &["--no-bundle", "--no-sign", "--ci"])?;
        let built_exe = get_tauri_windows_executable(&tauri_release_dir)?;
        invoke_optional_code_signing(root, &options, &[built_exe])?;
        invoke_tauri_build(root, &["--bundles", "nsis", "--no-sign", "--ci"])?;
    }

    let tauri_exe = get_tauri_windows_executable(&tauri_release_dir)?;
    if options.code_sign && !options.build {
        invoke_optional_code_signing(root, &options, std::slice::from_ref(&tauri_exe))?;
    }

    let tauri_setup = get_tauri_nsis_installer(&tauri_release_dir)?;
    if options.try_installer && tauri_setup.is_none() {
        return Err(format!(
            "Tauri NSIS installer not found under {}",
            tauri_release_dir.join("bundle/nsis").display()
        )
        .into());
    }

    let private_key = get_or_create_signing_key(root, &release_dir)?;
    let mut windows_assets = Vec::new();

    let portable_zip =
        release_dir.join(format!("CC-Desktop-Switch-v{version}-Windows-Portable.zip"));
    create_windows_portable_zip(root, &release_dir, &tauri_exe, &portable_zip)?;
    windows_assets.push(add_release_asset(
        &portable_zip,
        &private_key,
        &version,
        options.repository.as_deref(),
    )?);

    let release_exe = release_dir.join(format!("CC-Desktop-Switch-v{version}-Windows-x64.exe"));
    copy_file(&tauri_exe, &release_exe)?;
    windows_assets.push(add_release_asset(
        &release_exe,
        &private_key,
        &version,
        options.repository.as_deref(),
    )?);

    if let Some(setup) = tauri_setup {
        let release_setup =
            release_dir.join(format!("CC-Desktop-Switch-v{version}-Windows-Setup.exe"));
        copy_file(&setup, &release_setup)?;
        invoke_optional_code_signing(root, &options, std::slice::from_ref(&release_setup))?;
        windows_assets.push(add_release_asset(
            &release_setup,
            &private_key,
            &version,
            options.repository.as_deref(),
        )?);
    }

    let mut platforms = BTreeMap::new();
    platforms.insert(
        "windows-x64".to_string(),
        PlatformAssets {
            assets: windows_assets,
        },
    );

    add_macos_platform_assets(
        &mut platforms,
        &dist_dir.join("mac"),
        &release_dir,
        "arm64",
        &version,
        &private_key,
        options.repository.as_deref(),
    )?;
    add_macos_platform_assets(
        &mut platforms,
        &dist_dir.join("mac"),
        &release_dir,
        "x64",
        &version,
        &private_key,
        options.repository.as_deref(),
    )?;

    let latest = LatestJson {
        name: "CC Desktop Switch",
        version: version.clone(),
        pub_date: utc_now_rfc3339()?,
        notes: format!("Windows release for CC Desktop Switch v{version}."),
        update_protocol: 1,
        minimum_supported_version: "1.0.0",
        platforms,
        signature: SignatureInfo {
            algorithm: "RSA-PKCS1v15-SHA256",
            public_key: "CC-Desktop-Switch-release-public.pem",
            format: "base64 raw signature over file bytes",
        },
    };

    let latest_path = release_dir.join("latest.json");
    fs::write(&latest_path, serde_json::to_string_pretty(&latest)?)?;
    write_release_sidecars(&latest_path, &private_key)?;

    print_release_dir(&release_dir)?;
    Ok(())
}

fn verify_release_signature(root: &Path, args: &[String]) -> DynResult<()> {
    let options = parse_verify_options(root, args)?;
    let signature = options
        .signature
        .unwrap_or_else(|| PathBuf::from(format!("{}.sig", options.file.display())));
    let public_key = load_public_key(&options.public_key)?;
    let bytes = fs::read(&options.file)?;
    let sig_text = fs::read_to_string(&signature)?;
    let sig_bytes = BASE64.decode(sig_text.trim())?;
    let signature = pkcs1v15::Signature::try_from(sig_bytes.as_slice())?;
    let verifying_key = pkcs1v15::VerifyingKey::<Sha256>::new(public_key);
    verifying_key.verify(&bytes, &signature)?;
    println!("SIGNATURE_OK {}", options.file.display());
    Ok(())
}

fn package_macos(root: &Path, args: &[String]) -> DynResult<()> {
    let options = parse_macos_options(args)?;
    let version = options.version.clone().unwrap_or(package_version(root)?);
    let arch = options.arch.clone().unwrap_or_else(default_macos_arch);
    let app_path = resolve_from_root(
        root,
        &options
            .app_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("dist/mac/CC Desktop Switch.app")),
    );
    let output_pkg = resolve_from_root(
        root,
        &options.output_pkg.clone().unwrap_or_else(|| {
            PathBuf::from(format!(
                "dist/mac/CC-Desktop-Switch-v{version}-macOS-{arch}.pkg"
            ))
        }),
    );
    let output_dmg = resolve_from_root(
        root,
        &options.output_dmg.clone().unwrap_or_else(|| {
            PathBuf::from(format!(
                "dist/mac/CC-Desktop-Switch-v{version}-macOS-{arch}.dmg"
            ))
        }),
    );

    if !app_path.exists() {
        return Err(format!("App bundle not found: {}", app_path.display()).into());
    }

    if !options.skip_pkg {
        build_macos_pkg(root, &version, &app_path, &output_pkg)?;
    }
    if !options.skip_dmg {
        build_macos_dmg(root, &version, &app_path, &output_dmg)?;
    }
    Ok(())
}

fn parse_windows_options(args: &[String]) -> DynResult<WindowsReleaseOptions> {
    let mut options = WindowsReleaseOptions {
        output_dir: PathBuf::from("release"),
        timestamp_server: "http://timestamp.digicert.com".to_string(),
        repository: env::var("GITHUB_REPOSITORY")
            .ok()
            .filter(|value| !value.is_empty()),
        ..WindowsReleaseOptions::default()
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--version" => options.version = Some(next_value(args, &mut index, "--version")?),
            "--output-dir" => {
                options.output_dir = PathBuf::from(next_value(args, &mut index, "--output-dir")?)
            }
            "--build" => options.build = true,
            "--try-installer" => options.try_installer = true,
            "--code-sign" => options.code_sign = true,
            "--code-signing-certificate-path" => {
                options.certificate_path = Some(next_value(
                    args,
                    &mut index,
                    "--code-signing-certificate-path",
                )?)
            }
            "--code-signing-certificate-password" => {
                options.certificate_password = Some(next_value(
                    args,
                    &mut index,
                    "--code-signing-certificate-password",
                )?)
            }
            "--code-signing-certificate-base64" => {
                options.certificate_base64 = Some(next_value(
                    args,
                    &mut index,
                    "--code-signing-certificate-base64",
                )?)
            }
            "--timestamp-server" => {
                options.timestamp_server = next_value(args, &mut index, "--timestamp-server")?
            }
            "--repository" => {
                options.repository = Some(next_value(args, &mut index, "--repository")?)
            }
            value => return Err(format!("unknown release windows flag: {value}").into()),
        }
        index += 1;
    }
    Ok(options)
}

fn parse_verify_options(root: &Path, args: &[String]) -> DynResult<VerifyOptions> {
    let mut file = None;
    let mut signature = None;
    let mut public_key = PathBuf::from("release/CC-Desktop-Switch-release-public.pem");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--file" => file = Some(PathBuf::from(next_value(args, &mut index, "--file")?)),
            "--signature" => {
                signature = Some(PathBuf::from(next_value(args, &mut index, "--signature")?))
            }
            "--public-key" => {
                public_key = PathBuf::from(next_value(args, &mut index, "--public-key")?)
            }
            value => return Err(format!("unknown release verify flag: {value}").into()),
        }
        index += 1;
    }
    let file = file.ok_or("release verify requires --file <path>")?;
    Ok(VerifyOptions {
        file: resolve_from_root(root, &file),
        signature: signature.map(|path| resolve_from_root(root, &path)),
        public_key: resolve_from_root(root, &public_key),
    })
}

fn parse_macos_options(args: &[String]) -> DynResult<MacosOptions> {
    let mut options = MacosOptions {
        version: None,
        arch: None,
        app_path: None,
        output_pkg: None,
        output_dmg: None,
        skip_pkg: false,
        skip_dmg: false,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--version" => options.version = Some(next_value(args, &mut index, "--version")?),
            "--arch" => options.arch = Some(next_value(args, &mut index, "--arch")?),
            "--app" => {
                options.app_path = Some(PathBuf::from(next_value(args, &mut index, "--app")?))
            }
            "--pkg" => {
                options.output_pkg = Some(PathBuf::from(next_value(args, &mut index, "--pkg")?))
            }
            "--dmg" => {
                options.output_dmg = Some(PathBuf::from(next_value(args, &mut index, "--dmg")?))
            }
            "--skip-pkg" => options.skip_pkg = true,
            "--skip-dmg" => options.skip_dmg = true,
            value => return Err(format!("unknown package macos flag: {value}").into()),
        }
        index += 1;
    }
    if options.skip_pkg && options.skip_dmg {
        return Err("package macos cannot skip both pkg and dmg".into());
    }
    Ok(options)
}

fn next_value(args: &[String], index: &mut usize, flag: &str) -> DynResult<String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value").into())
}

fn package_version(root: &Path) -> DynResult<String> {
    let package_json: Value =
        serde_json::from_str(&fs::read_to_string(root.join("package.json"))?)?;
    package_json
        .get("version")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "version not found in package.json".into())
}

fn resolve_from_root(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn invoke_tauri_build(root: &Path, args: &[&str]) -> DynResult<()> {
    let mut command = Command::new(pnpm_program_name());
    command
        .current_dir(root)
        .arg("tauri")
        .arg("build")
        .args(args);
    run_command(&mut command, "Tauri build")
}

fn pnpm_program_name() -> &'static str {
    if cfg!(windows) { "pnpm.cmd" } else { "pnpm" }
}

fn invoke_optional_code_signing(
    root: &Path,
    options: &WindowsReleaseOptions,
    files: &[PathBuf],
) -> DynResult<()> {
    if !options.code_sign || files.is_empty() {
        return Ok(());
    }

    let mut certificate_base64 = options.certificate_base64.clone();
    if certificate_base64.is_none() {
        certificate_base64 = env::var("WINDOWS_CODESIGN_PFX_BASE64")
            .ok()
            .filter(|value| !value.is_empty());
    }
    let mut certificate_password = options.certificate_password.clone();
    if certificate_password.is_none() {
        certificate_password = env::var("WINDOWS_CODESIGN_PFX_PASSWORD").ok();
    }

    let mut command = Command::new("powershell");
    command
        .current_dir(root)
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(root.join("scripts/Invoke-CodeSigning.ps1"))
        .arg("-Files");
    for file in files {
        command.arg(file);
    }
    command
        .arg("-TimestampServer")
        .arg(&options.timestamp_server);
    if let Some(path) = &options.certificate_path {
        command.arg("-CertificatePath").arg(path);
    }
    if let Some(password) = &certificate_password {
        command.arg("-CertificatePassword").arg(password);
    }
    if let Some(base64) = &certificate_base64 {
        command.arg("-CertificateBase64").arg(base64);
    }
    run_command(&mut command, "Authenticode signing")
}

fn get_tauri_windows_executable(release_dir: &Path) -> DynResult<PathBuf> {
    let candidate = release_dir.join("cc-desktop-switch.exe");
    if candidate.is_file() {
        return Ok(candidate);
    }
    newest_file(release_dir, |path| {
        path.extension().and_then(|value| value.to_str()) == Some("exe")
            && path
                .file_name()
                .and_then(|value| value.to_str())
                .map(|name| {
                    let lower = name.to_lowercase();
                    !lower.contains("setup") && !lower.contains("installer")
                })
                .unwrap_or(false)
    })
    .ok_or_else(|| {
        format!(
            "Tauri Windows executable not found under {}",
            release_dir.display()
        )
        .into()
    })
}

fn get_tauri_nsis_installer(release_dir: &Path) -> DynResult<Option<PathBuf>> {
    let nsis_dir = release_dir.join("bundle/nsis");
    if !nsis_dir.is_dir() {
        return Ok(None);
    }
    Ok(newest_file(&nsis_dir, |path| {
        path.extension().and_then(|value| value.to_str()) == Some("exe")
    }))
}

fn newest_file<F>(dir: &Path, predicate: F) -> Option<PathBuf>
where
    F: Fn(&Path) -> bool,
{
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && predicate(path))
        .max_by_key(|path| {
            path.metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        })
}

fn clean_release_dir(release_dir: &Path) -> DynResult<()> {
    for entry in fs::read_dir(release_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let should_remove = (name.starts_with("CC-Desktop-Switch-v")
            && (name.contains("-Windows-") || name.contains("-macOS-")))
            || matches!(
                name.as_ref(),
                "CC-Desktop-Switch-release-public.pem"
                    | "latest.json"
                    | "latest.json.sha256"
                    | "latest.json.sig"
            );
        if should_remove {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn get_or_create_signing_key(root: &Path, release_dir: &Path) -> DynResult<RsaPrivateKey> {
    let key_dir = root.join(".release-signing");
    fs::create_dir_all(&key_dir)?;
    let private_path = key_dir.join("release-private-key.pem");
    let public_path = key_dir.join("release-public-key.pem");

    let private_key = if private_path.is_file() {
        load_private_key(&private_path)?
    } else {
        let mut rng = OsRng;
        let private_key = RsaPrivateKey::new(&mut rng, 3072)?;
        let private_pem = private_key.to_pkcs1_pem(LineEnding::LF)?;
        let public_pem = RsaPublicKey::from(&private_key).to_pkcs1_pem(LineEnding::LF)?;
        fs::write(&private_path, private_pem.as_bytes())?;
        fs::write(&public_path, public_pem.as_bytes())?;
        println!(
            "Created local release signing key: {}",
            private_path.display()
        );
        private_key
    };

    if !public_path.is_file() {
        let public_pem = RsaPublicKey::from(&private_key).to_pkcs1_pem(LineEnding::LF)?;
        fs::write(&public_path, public_pem.as_bytes())?;
    }
    copy_file(
        &public_path,
        &release_dir.join("CC-Desktop-Switch-release-public.pem"),
    )?;
    Ok(private_key)
}

fn load_private_key(path: &Path) -> DynResult<RsaPrivateKey> {
    let text = fs::read_to_string(path)?;
    if text.contains("RSA PRIVATE KEY BLOB") {
        return parse_csp_private_key(&text);
    }
    Ok(RsaPrivateKey::from_pkcs1_pem(&text)?)
}

fn load_public_key(path: &Path) -> DynResult<RsaPublicKey> {
    let text = fs::read_to_string(path)?;
    if text.contains("RSA PUBLIC KEY BLOB") {
        return parse_csp_public_key(&text);
    }
    Ok(RsaPublicKey::from_pkcs1_pem(&text)?)
}

fn parse_csp_private_key(text: &str) -> DynResult<RsaPrivateKey> {
    let bytes = decode_legacy_csp_blob(text)?;
    ensure_min_len(&bytes, 20, "legacy private key blob")?;
    if bytes[0] != 0x07 {
        return Err("legacy private key blob has an unexpected blob type".into());
    }
    let magic = read_u32_le(&bytes, 8)?;
    if magic != 0x3241_5352 {
        return Err("legacy private key blob has an unexpected RSA magic".into());
    }
    let bit_len = read_u32_le(&bytes, 12)? as usize;
    let key_len = bit_len / 8;
    let half_len = key_len / 2;
    let e = BigUint::from(read_u32_le(&bytes, 16)?);
    let mut offset = 20;
    let n = read_biguint_le(&bytes, &mut offset, key_len)?;
    let p = read_biguint_le(&bytes, &mut offset, half_len)?;
    let q = read_biguint_le(&bytes, &mut offset, half_len)?;
    offset += half_len * 3;
    ensure_min_len(&bytes, offset + key_len, "legacy private key blob")?;
    let d = read_biguint_le(&bytes, &mut offset, key_len)?;
    Ok(RsaPrivateKey::from_components(n, e, d, vec![p, q])?)
}

fn parse_csp_public_key(text: &str) -> DynResult<RsaPublicKey> {
    let bytes = decode_legacy_csp_blob(text)?;
    ensure_min_len(&bytes, 20, "legacy public key blob")?;
    if bytes[0] != 0x06 {
        return Err("legacy public key blob has an unexpected blob type".into());
    }
    let magic = read_u32_le(&bytes, 8)?;
    if magic != 0x3141_5352 {
        return Err("legacy public key blob has an unexpected RSA magic".into());
    }
    let bit_len = read_u32_le(&bytes, 12)? as usize;
    let key_len = bit_len / 8;
    let e = BigUint::from(read_u32_le(&bytes, 16)?);
    let mut offset = 20;
    let n = read_biguint_le(&bytes, &mut offset, key_len)?;
    Ok(RsaPublicKey::new(n, e)?)
}

fn decode_legacy_csp_blob(text: &str) -> DynResult<Vec<u8>> {
    let body = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("-----"))
        .collect::<String>();
    Ok(BASE64.decode(body)?)
}

fn read_biguint_le(bytes: &[u8], offset: &mut usize, len: usize) -> DynResult<BigUint> {
    ensure_min_len(bytes, *offset + len, "legacy RSA key blob")?;
    let value = BigUint::from_bytes_le(&bytes[*offset..*offset + len]);
    *offset += len;
    Ok(value)
}

fn read_u32_le(bytes: &[u8], offset: usize) -> DynResult<u32> {
    ensure_min_len(bytes, offset + 4, "legacy RSA key blob")?;
    Ok(u32::from_le_bytes(bytes[offset..offset + 4].try_into()?))
}

fn ensure_min_len(bytes: &[u8], len: usize, label: &str) -> DynResult<()> {
    if bytes.len() < len {
        Err(format!("{label} is truncated").into())
    } else {
        Ok(())
    }
}

fn create_windows_portable_zip(
    root: &Path,
    release_dir: &Path,
    tauri_exe: &Path,
    portable_zip: &Path,
) -> DynResult<()> {
    remove_file_if_exists(portable_zip)?;
    let portable_stage = release_dir.join("portable-windows-x64");
    remove_dir_if_exists(&portable_stage)?;
    fs::create_dir_all(&portable_stage)?;
    copy_file(tauri_exe, &portable_stage.join("CC Desktop Switch.exe"))?;
    let license = root.join("LICENSE.txt");
    if license.is_file() {
        copy_file(&license, &portable_stage.join("LICENSE.txt"))?;
    }
    zip_directory_contents(&portable_stage, portable_zip)?;
    remove_dir_if_exists(&portable_stage)?;
    Ok(())
}

fn zip_directory_contents(source_dir: &Path, zip_path: &Path) -> DynResult<()> {
    let file = fs::File::create(zip_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o755);
    add_directory_to_zip(&mut zip, source_dir, source_dir, options)?;
    zip.finish()?;
    Ok(())
}

fn add_directory_to_zip(
    zip: &mut ZipWriter<fs::File>,
    root: &Path,
    current: &Path,
    options: SimpleFileOptions,
) -> DynResult<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root)?;
        let name = relative.to_string_lossy().replace('\\', "/");
        if path.is_dir() {
            zip.add_directory(format!("{name}/"), options)?;
            add_directory_to_zip(zip, root, &path, options)?;
        } else {
            zip.start_file(name, options)?;
            let mut file = fs::File::open(&path)?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
        }
    }
    Ok(())
}

fn add_release_asset(
    path: &Path,
    private_key: &RsaPrivateKey,
    version: &str,
    repository: Option<&str>,
) -> DynResult<ReleaseAsset> {
    let (sha256, signature) = write_release_sidecars(path, private_key)?;
    let name = file_name(path)?;
    Ok(ReleaseAsset {
        url: asset_url(repository, version, &name),
        signature,
        sha256,
        size: fs::metadata(path)?.len(),
        name,
    })
}

fn write_release_sidecars(path: &Path, private_key: &RsaPrivateKey) -> DynResult<(String, String)> {
    let bytes = fs::read(path)?;
    let sha256 = sha256_hex(&bytes);
    let name = file_name(path)?;
    let sha_path = PathBuf::from(format!("{}.sha256", path.display()));
    fs::write(sha_path, format!("{sha256}  {name}\n"))?;

    let signing_key = pkcs1v15::SigningKey::<Sha256>::new(private_key.clone());
    let signature = signing_key.sign(&bytes);
    let sig_path = PathBuf::from(format!("{}.sig", path.display()));
    fs::write(&sig_path, BASE64.encode(signature.to_bytes()))?;
    Ok((sha256, file_name(&sig_path)?))
}

fn add_macos_platform_assets(
    platforms: &mut BTreeMap<String, PlatformAssets>,
    mac_dir: &Path,
    release_dir: &Path,
    arch: &str,
    version: &str,
    private_key: &RsaPrivateKey,
    repository: Option<&str>,
) -> DynResult<()> {
    if !mac_dir.is_dir() {
        return Ok(());
    }
    let mut assets = Vec::new();
    for extension in ["pkg", "dmg"] {
        let asset_path = mac_dir.join(format!(
            "CC-Desktop-Switch-v{version}-macOS-{arch}.{extension}"
        ));
        if asset_path.is_file() {
            let release_asset_path = release_dir.join(file_name(&asset_path)?);
            copy_file(&asset_path, &release_asset_path)?;
            assets.push(add_release_asset(
                &release_asset_path,
                private_key,
                version,
                repository,
            )?);
        }
    }
    if !assets.is_empty() {
        platforms.insert(format!("macos-{arch}"), PlatformAssets { assets });
    }
    Ok(())
}

fn build_macos_pkg(
    root: &Path,
    version: &str,
    app_path: &Path,
    output_pkg: &Path,
) -> DynResult<()> {
    require_command(
        "pkgbuild",
        "pkgbuild is required to create a macOS installer package.",
    )?;
    let pkg_root = root.join(".tmp/pkg-root");
    remove_dir_if_exists(&pkg_root)?;
    fs::create_dir_all(pkg_root.join("Applications"))?;
    fs::create_dir_all(parent_dir(output_pkg)?)?;
    let app_target = pkg_root.join("Applications").join(file_name(app_path)?);
    run_command(
        Command::new("ditto").arg(app_path).arg(&app_target),
        "copy macOS app bundle",
    )?;
    remove_file_if_exists(output_pkg)?;
    run_command(
        Command::new("pkgbuild")
            .arg("--root")
            .arg(&pkg_root)
            .arg("--install-location")
            .arg("/")
            .arg("--identifier")
            .arg("io.github.lonr6.ccdesktopswitch")
            .arg("--version")
            .arg(version)
            .arg("--scripts")
            .arg(root.join("macos/pkg-scripts"))
            .arg(output_pkg),
        "pkgbuild",
    )?;
    Ok(())
}

fn build_macos_dmg(
    root: &Path,
    version: &str,
    source_path: &Path,
    output_dmg: &Path,
) -> DynResult<()> {
    require_command("hdiutil", "hdiutil is required to create a DMG.")?;
    fs::create_dir_all(parent_dir(output_dmg)?)?;
    let staging = root
        .join(".tmp")
        .join(format!("dmg-staging-{}", std::process::id()));
    remove_dir_if_exists(&staging)?;
    fs::create_dir_all(&staging)?;

    let result = (|| -> DynResult<()> {
        let staged_source = staging.join(file_name(source_path)?);
        if command_exists("ditto") {
            run_command(
                Command::new("ditto").arg(source_path).arg(&staged_source),
                "copy DMG source",
            )?;
        } else {
            copy_dir_recursive(source_path, &staged_source)?;
        }
        if source_path.extension().and_then(|value| value.to_str()) == Some("app") {
            create_applications_symlink(&staging.join("Applications"))?;
        }
        remove_file_if_exists(output_dmg)?;
        run_command(
            Command::new("hdiutil")
                .arg("create")
                .arg("-volname")
                .arg(format!("CC Desktop Switch {version}"))
                .arg("-srcfolder")
                .arg(&staging)
                .arg("-ov")
                .arg("-format")
                .arg("UDZO")
                .arg(output_dmg),
            "hdiutil create",
        )
    })();

    remove_dir_if_exists(&staging)?;
    result
}

#[cfg(unix)]
fn create_applications_symlink(path: &Path) -> DynResult<()> {
    std::os::unix::fs::symlink("/Applications", path)?;
    Ok(())
}

#[cfg(not(unix))]
fn create_applications_symlink(_path: &Path) -> DynResult<()> {
    Err("Applications symlink creation is only supported on Unix-like systems".into())
}

fn run_command(command: &mut Command, label: &str) -> DynResult<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} failed with exit code {status}").into())
    }
}

fn require_command(command: &str, message: &str) -> DynResult<()> {
    if command_exists(command) {
        Ok(())
    } else {
        Err(message.to_string().into())
    }
}

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success() || status.code().is_some())
        .unwrap_or(false)
}

fn print_release_dir(release_dir: &Path) -> DynResult<()> {
    let mut entries = fs::read_dir(release_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        println!("{} {}", file_name(&path)?, fs::metadata(&path)?.len());
    }
    Ok(())
}

fn asset_url(repository: Option<&str>, version: &str, file_name: &str) -> String {
    repository
        .filter(|value| !value.is_empty())
        .map(|repo| format!("https://github.com/{repo}/releases/download/v{version}/{file_name}"))
        .unwrap_or_else(|| file_name.to_string())
}

fn utc_now_rfc3339() -> DynResult<String> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn copy_file(source: &Path, target: &Path) -> DynResult<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, target)?;
    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> DynResult<()> {
    if source.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_dir_recursive(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else {
        copy_file(source, target)?;
    }
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> DynResult<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> DynResult<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn parent_dir(path: &Path) -> DynResult<&Path> {
    path.parent()
        .ok_or_else(|| format!("path has no parent directory: {}", path.display()).into())
}

fn file_name(path: &Path) -> DynResult<String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("path has no file name: {}", path.display()).into())
}

fn default_macos_arch() -> String {
    match env::consts::ARCH {
        "aarch64" => "arm64".to_string(),
        "x86_64" => "x64".to_string(),
        value => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_signature_round_trips_with_generated_key() {
        let root = unique_temp_dir("ccds-xtask-signature");
        let release_dir = root.join("release");
        fs::create_dir_all(&release_dir).unwrap();

        let mut rng = OsRng;
        let private_key = RsaPrivateKey::new(&mut rng, 1024).unwrap();
        let public_pem = RsaPublicKey::from(&private_key)
            .to_pkcs1_pem(LineEnding::LF)
            .unwrap();
        fs::write(
            release_dir.join("CC-Desktop-Switch-release-public.pem"),
            public_pem.as_bytes(),
        )
        .unwrap();
        let asset = release_dir.join("asset.bin");
        fs::write(&asset, b"release asset").unwrap();
        write_release_sidecars(&asset, &private_key).unwrap();

        let public_key =
            load_public_key(&release_dir.join("CC-Desktop-Switch-release-public.pem")).unwrap();
        let bytes = fs::read(&asset).unwrap();
        let sig_text = fs::read_to_string(format!("{}.sig", asset.display())).unwrap();
        let sig_bytes = BASE64.decode(sig_text.trim()).unwrap();
        let signature = pkcs1v15::Signature::try_from(sig_bytes.as_slice()).unwrap();
        pkcs1v15::VerifyingKey::<Sha256>::new(public_key)
            .verify(&bytes, &signature)
            .unwrap();

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pnpm_program_uses_windows_command_shim() {
        if cfg!(windows) {
            assert_eq!(pnpm_program_name(), "pnpm.cmd");
        } else {
            assert_eq!(pnpm_program_name(), "pnpm");
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }
}
