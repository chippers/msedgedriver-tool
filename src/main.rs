use std::{
    fs::File,
    io::{BufWriter, Cursor},
    process::{exit, Command},
};

use anyhow::Result;
use quick_xml::de::from_str;
use serde::Deserialize;
use zip::ZipArchive;

const MANIFEST_URL: &str = "https://msedgedriver.azureedge.net";

#[derive(Debug, Default, Deserialize)]
struct EnumerationResults {
    #[serde(rename = "Blobs", default)]
    blobs: Blobs,
}

#[derive(Debug, Default, Deserialize)]

struct Blobs {
    #[serde(rename = "Blob", default)]
    blobs: Vec<Blob>,
}

#[derive(Debug, Default, Deserialize)]

struct Blob {
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "Url", default)]
    url: String,
}

fn main() -> Result<()> {
    let webview2_version = match webview2_version() {
        Ok(Some(w2v)) => w2v,
        Ok(None) => {
            eprintln!("unable to find webview2_version");
            exit(1);
        }
        Err(e) => {
            eprintln!(
                "unable to find webview2_version due to underlying error: {}",
                e
            );
            exit(1);
        }
    };

    println!("webview2 version: {}", webview2_version);

    let manifest = ureq::get(MANIFEST_URL)
        .set(
            "User-Agent",
            dbg!(concat!(
                env!("CARGO_PKG_NAME"),
                " ",
                env!("CARGO_PKG_VERSION")
            )),
        )
        .call()?
        .into_string()?;

    println!("writing manifest file to ./msedgedriver-manifest.xml");
    std::fs::write("msedgedriver-manifest.xml", manifest.as_bytes())?;

    let results: EnumerationResults = from_str(&manifest)?;
    let name_to_find = format!("{}/edgedriver_win64.zip", webview2_version.trim());

    println!("searching manifest for {}", name_to_find);
    let blob = results
        .blobs
        .blobs
        .into_iter()
        .find(|b| b.name == name_to_find)
        .expect("could not find matching edgedriver version");

    println!("downloading found zip file");
    let mut buf = Vec::new();
    ureq::get(&blob.url)
        .set(
            "User-Agent",
            dbg!(concat!(
                env!("CARGO_PKG_NAME"),
                " ",
                env!("CARGO_PKG_VERSION")
            )),
        )
        .call()?
        .into_reader()
        .read_to_end(&mut buf)?;

    println!("extracting msedgedriver.exe from downloaded zip archive");
    let mut archive = ZipArchive::new(Cursor::new(buf))?;
    let mut driver = archive.by_name("msedgedriver.exe")?;
    let mut writer = BufWriter::new(File::create("msedgedriver.exe")?);
    std::io::copy(&mut driver, &mut writer)?;

    Ok(())
}

// taken from tauri-cli
fn webview2_version() -> Result<Option<String>> {
    // check 64bit machine-wide installation
    let output = Command::new("powershell")
        .args(&["-NoProfile", "-Command"])
        .arg("Get-ItemProperty -Path 'HKLM:\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' | ForEach-Object {$_.pv}")
        .output()?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).replace('\n', ""),
        ));
    }
    // check 32bit machine-wide installation
    let output = Command::new("powershell")
          .args(&["-NoProfile", "-Command"])
          .arg("Get-ItemProperty -Path 'HKLM:\\SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' | ForEach-Object {$_.pv}")
          .output()?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).replace('\n', ""),
        ));
    }
    // check user-wide installation
    let output = Command::new("powershell")
        .args(&["-NoProfile", "-Command"])
        .arg("Get-ItemProperty -Path 'HKCU:\\SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' | ForEach-Object {$_.pv}")
        .output()?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).replace('\n', ""),
        ));
    }

    Ok(None)
}
