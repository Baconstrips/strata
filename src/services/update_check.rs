// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    sync::mpsc::{self, Receiver},
    time::Duration,
};

use serde::Deserialize;

const RELEASES_URL: &str = "https://api.github.com/repos/lgse/strata/releases/latest";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateCheck {
    UpToDate,
    Available {
        version: String,
        url: String,
        download_url: Option<String>,
    },
    Failed(String),
}

#[derive(Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

/// The asset naming convention published by `.github/workflows/release.yml`.
fn archive_name(version: &str) -> String {
    format!(
        "strata-{version}-{}-unknown-linux-gnu.tar.gz",
        std::env::consts::ARCH
    )
}

/// Queries the latest GitHub release off the GTK thread and reports the outcome once.
pub fn check_for_updates(current_version: &'static str) -> Receiver<UpdateCheck> {
    let (sender, receiver) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("strata-update-check".into())
        .spawn(move || {
            let _sent = sender.send(fetch_latest_release(current_version));
        });
    drop(spawned);
    receiver
}

fn fetch_latest_release(current_version: &str) -> UpdateCheck {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build();
    let agent: ureq::Agent = config.into();
    let release = agent
        .get(RELEASES_URL)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "strata-file-manager")
        .call()
        .and_then(|mut response| response.body_mut().read_json::<ReleaseResponse>());
    match release {
        Ok(release) => {
            let latest = release.tag_name.trim_start_matches('v');
            if is_newer(latest, current_version) {
                let archive_name = archive_name(latest);
                let download_url = release
                    .assets
                    .iter()
                    .find(|asset| asset.name == archive_name)
                    .map(|asset| asset.browser_download_url.clone());
                UpdateCheck::Available {
                    version: latest.to_owned(),
                    url: release.html_url,
                    download_url,
                }
            } else {
                UpdateCheck::UpToDate
            }
        }
        Err(error) => UpdateCheck::Failed(error.to_string()),
    }
}

fn is_newer(candidate: &str, current: &str) -> bool {
    parse_version(candidate) > parse_version(current)
}

fn parse_version(value: &str) -> (u64, u64, u64) {
    let mut parts = value.split('.').map(|part| part.parse().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

#[cfg(test)]
mod tests;
