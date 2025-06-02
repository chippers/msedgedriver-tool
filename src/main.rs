use std::{
    fs::File,
    io::{BufWriter, Cursor},
    process::Command,
};

use anyhow::{Context, Result};
use ureq::http::header::USER_AGENT;
use zip::ZipArchive;

/// Grab the url for the win64 Microsoft Edge WebDriver.
fn driver_url(version: &str) -> String {
    format!("https://msedgedriver.microsoft.com/{version}/edgedriver_win64.zip")
}

fn main() -> Result<()> {
    let webview2_version =
        webview2_version()?.context("unable to find an installed WebView2 client")?;

    println!("webview2 version: {}", webview2_version);
    let driver_url = driver_url(&webview2_version);

    println!("downloading win64 driver from {}", driver_url);
    let buf = ureq::get(&driver_url)
        .header(USER_AGENT, concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION")))
        .call()?
        .into_body()
        .with_config()
        .limit(100 * 1024 * 1024) // limit to 100MiB instead of default 10MiB
        .read_to_vec()?;

    println!("extracting msedgedriver.exe from downloaded zip archive");
    let mut archive = ZipArchive::new(Cursor::new(buf))?;
    let mut driver = archive.by_name("msedgedriver.exe")?;
    let mut writer = BufWriter::new(File::create("msedgedriver.exe")?);
    std::io::copy(&mut driver, &mut writer)?;

    Ok(())
}

macro_rules! registry_path {
    ($prefix:literal) => {
        concat!($prefix, "Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}")
    };
}

enum Webview2Install {
    Global64,
    Global32,
    User64,
    User32,
}

impl Webview2Install {
    const ALL: &'static [Self] = &[Self::Global64, Self::Global32, Self::User64, Self::User32];

    fn registry_path(&self) -> &'static str {
        match self {
            Webview2Install::Global64 => registry_path!("HKLM:\\SOFTWARE\\WOW6432Node\\"),
            Webview2Install::Global32 => registry_path!("HKLM:\\SOFTWARE\\"),
            Webview2Install::User64 => registry_path!("HKCU:\\SOFTWARE\\WOW6432Node\\"),
            Webview2Install::User32 => registry_path!("HKCU:\\SOFTWARE\\"),
        }
    }
}

fn webview2_version() -> Result<Option<String>> {
    for install in Webview2Install::ALL {
        if let Some(version) = pwsh_get_webview2_registry(install.registry_path())? {
            return Ok(Some(version));
        }
    }

    Ok(None)
}

fn pwsh_get_webview2_registry(registry_path: &str) -> Result<Option<String>> {
    Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(format!("Get-ItemProperty -Path '{registry_path}' | ForEach-Object {{$_.pv}}"))
        .output()
        .context("unable to run powershell command to grab webview2 version")
        .map(|output| {
            output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        })
}
