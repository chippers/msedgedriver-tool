use std::{
    env::consts::{ARCH, OS},
    fmt,
    fs::File,
    io,
    io::{BufWriter, Cursor},
    process::{Command, Output},
};

use ureq::http::header::USER_AGENT;
use zip::{result::ZipError, ZipArchive};

const NAME_VERSION: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));

#[derive(Debug)]
enum Error {
    Powershell(io::Error),
    NoInstallFound,
    Platform { arch: &'static str, os: &'static str },
    Unsupported,
    Io(io::Error),
    UReq(ureq::Error),
    Zip(ZipError),
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Powershell(err) => {
                f.write_fmt(format_args!("unable to run command to find WebView2 version: {err}"))
            }
            Self::NoInstallFound => f.write_str("no WebView2 installation found"),
            Self::Platform { arch, os } => {
                f.write_fmt(format_args!("{os}({arch}) platform not supported by msedgedriver"))
            }
            Self::Unsupported => f.write_fmt(format_args!(
                "{NAME_VERSION} currently only supports finding webview2 installs on Windows"
            )),
            Self::Io(err) => f.write_fmt(format_args!("I/O error: {err}")),
            Self::UReq(err) => f.write_fmt(format_args!("http request failed: {err}")),
            Self::Zip(err) => f.write_fmt(format_args!("unzipping archive failed: {err}")),
        }
    }
}

impl From<ureq::Error> for Error {
    fn from(err: ureq::Error) -> Self {
        Self::UReq(err)
    }
}

impl From<ZipError> for Error {
    fn from(err: ZipError) -> Self {
        Self::Zip(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// Grab the url for the win64 Microsoft Edge WebDriver.
fn driver_url(version: &Version, platform: &Platform) -> String {
    format!("https://msedgedriver.microsoft.com/{version}/edgedriver_{platform}.zip")
}

fn main() -> Result<(), Error> {
    if !cfg!(windows) {
        return Err(Error::Unsupported);
    }

    let webview2_version = webview2_version()?;
    println!("found webview2 version: {webview2_version}");

    let platform = Platform::current()?;
    println!("current platform: {platform}");

    let driver_url = driver_url(&webview2_version, &platform);
    println!("downloading {platform} driver from {driver_url}");
    let archive = fetch(&driver_url)?;

    let filename = if OS == "windows" { "msedgedriver.exe" } else { "msedgedriver" };
    println!("extracting {filename} from downloaded zip archive");
    extract(archive, filename)?;

    Ok(())
}

fn fetch(driver_url: &str) -> Result<Vec<u8>, Error> {
    Ok(ureq::get(driver_url)
        .header(USER_AGENT, NAME_VERSION)
        .call()?
        .into_body()
        .with_config()
        .limit(100 * 1024 * 1024) // limit to 100MiB instead of default 10MiB
        .read_to_vec()?)
}

fn extract(archive: Vec<u8>, filename: &str) -> Result<(), Error> {
    let mut archive = ZipArchive::new(Cursor::new(archive))?;
    let mut driver = archive.by_name(filename)?;
    let mut writer = BufWriter::new(File::create(filename)?);
    std::io::copy(&mut driver, &mut writer)?;
    Ok(())
}

/// How Microsoft labels platforms for Microsoft Edge WebDriver distributions.
struct Platform(&'static str);

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Platform {
    fn current() -> Result<Self, Error> {
        match (OS, ARCH) {
            ("windows", "x86_64") => Ok("win64"),
            ("windows", "aarch64") => Ok("arm64"),
            ("windows", "x86") => Ok("win32"),
            ("macos", "x86_64") => Ok("mac64"),
            ("macos", "aarch64") => Ok("mac64_m1"),
            ("linux", "x86_64") => Ok("linux64"),
            (os, arch) => Err(Error::Platform { os, arch }),
        }
            .map(Self)
    }
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
}

impl fmt::Display for Webview2Install {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Webview2Install::Global64 => registry_path!("HKLM:\\SOFTWARE\\WOW6432Node\\"),
            Webview2Install::Global32 => registry_path!("HKLM:\\SOFTWARE\\"),
            Webview2Install::User64 => registry_path!("HKCU:\\SOFTWARE\\WOW6432Node\\"),
            Webview2Install::User32 => registry_path!("HKCU:\\SOFTWARE\\"),
        })
    }
}

/// A WebView2 version consisting of 4 parts: `MAJOR.MINOR.BUILD.PATCH`.
struct Version(String);

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Version {
    fn from_output(output: Output) -> Option<Self> {
        output.status.success().then(|| {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stdout = stdout.trim();
            Self(stdout.to_string())
        })
    }
}

fn webview2_version() -> Result<Version, Error> {
    for install in Webview2Install::ALL {
        if let Some(version) = pwsh_get_webview2_registry(install)? {
            return Ok(version);
        }
    }

    Err(Error::NoInstallFound)
}

fn pwsh_get_webview2_registry(install: &Webview2Install) -> Result<Option<Version>, Error> {
    Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(format!("Get-ItemProperty -Path '{install}' | ForEach-Object {{$_.pv}}"))
        .output()
        .map(Version::from_output)
        .map_err(Error::Powershell)
}
