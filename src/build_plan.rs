// SPDX-FileCopyrightText: 2026 Theofilos Augoustis
//
// SPDX-License-Identifier: MPL-2.0

//! `--build-per-system`: build each system's profiles once.
//!
//! Profiles for the deploying machine's own system build locally, one after
//! another, so the local store shares everything they have in common. Each
//! other system builds on one target host of that system, the least loaded
//! one that answers. The results are then copied from there to the others.
//!
//! Every target host is asked first. Hosts that don't answer are left out,
//! since copying to them would hang the deploy.

use std::collections::{BTreeMap, HashMap};
use std::process::Stdio;
use std::time::Duration;

use log::{info, warn};
use thiserror::Error;
use tokio::process::Command;
use tokio::task::JoinSet;

use crate::push::{BuildHost, PushProfileData};

/// How long a candidate builder gets to answer.
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Error, Debug)]
pub enum BuildPlanError {
    #[error("Failed to ask Nix for the local system: {0}")]
    LocalSystem(std::io::Error),
    #[error("`nix config show system` failed with status {0:?}")]
    LocalSystemExit(Option<i32>),
}

/// The system this machine builds for.
pub async fn local_system() -> Result<String, BuildPlanError> {
    let output = Command::new("nix")
        .args(["--extra-experimental-features", "nix-command"])
        .args(["config", "show", "system"])
        .stderr(Stdio::inherit())
        .output()
        .await
        .map_err(BuildPlanError::LocalSystem)?;
    if !output.status.success() {
        return Err(BuildPlanError::LocalSystemExit(output.status.code()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Sets where each profile builds. Returns the nodes that didn't answer, in
/// the order they appear. Their profiles are left as they were.
pub async fn assign_build_hosts(
    profiles: &mut [PushProfileData],
) -> Result<Vec<String>, BuildPlanError> {
    let local = local_system().await?;
    let loads = probe_all(profiles.iter().map(BuildHost::of_node)).await;

    let mut unreachable: Vec<String> = Vec::new();
    for profile in profiles.iter() {
        let node = &profile.deploy_data.node_name;
        if loads[&BuildHost::of_node(profile)].is_none() && !unreachable.contains(node) {
            unreachable.push(node.clone());
        }
    }

    let mut foreign: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, profile) in profiles.iter_mut().enumerate() {
        if unreachable.contains(&profile.deploy_data.node_name) {
            continue;
        }
        match profile.deploy_data.profile.profile_settings.system.clone() {
            Some(system) if system == local => {
                profile.build_host = None;
                profile.deploy_data.merged_settings.remote_build = Some(false);
            }
            Some(system) => foreign.entry(system).or_default().push(i),
            // A hand-written store path has no known system. Keep its settings.
            None => (),
        }
    }

    for (system, indices) in foreign {
        let mut candidates: Vec<BuildHost> = Vec::new();
        for &i in &indices {
            let host = BuildHost::of_node(&profiles[i]);
            if !candidates.contains(&host) {
                candidates.push(host);
            }
        }

        let answers = candidates.into_iter().map(|host| {
            let load = loads[&host];
            (host, load)
        });
        match least_loaded(answers) {
            Some(builder) => {
                info!(
                    "Building {} profiles for {} on {}",
                    indices.len(),
                    system,
                    builder.hostname
                );
                for i in indices {
                    profiles[i].build_host = Some(builder.clone());
                }
            }
            // Every candidate answered, so this can't happen. Keep the
            // node's own settings rather than panic.
            None => warn!("No {} host answered", system),
        }
    }

    Ok(unreachable)
}

/// Load per core of each distinct host, asked in parallel. None means the
/// host didn't answer.
async fn probe_all(hosts: impl IntoIterator<Item = BuildHost>) -> HashMap<BuildHost, Option<f64>> {
    let mut probes = JoinSet::new();
    let mut seen = Vec::new();
    for host in hosts {
        if seen.contains(&host) {
            continue;
        }
        seen.push(host.clone());
        probes.spawn(async move {
            let load = probe_load(&host).await;
            (host, load)
        });
    }
    probes.join_all().await.into_iter().collect()
}

/// Load per core, or none when the host doesn't answer in time.
async fn probe_load(host: &BuildHost) -> Option<f64> {
    let mut command = Command::new("ssh");
    command
        .args(&host.ssh_opts)
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10"])
        .arg(format!("{}@{}", host.ssh_user, host.hostname))
        .arg("echo deploy-rs-load $(nproc) $(cut -d' ' -f1 /proc/loadavg)")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(PROBE_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_load(&String::from_utf8_lossy(&output.stdout))
}

/// Reads `deploy-rs-load <cores> <load>`, ignoring anything a login shell
/// printed before it.
fn parse_load(output: &str) -> Option<f64> {
    let line = output
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("deploy-rs-load "))?;
    let mut fields = line.split_whitespace();
    let cores: f64 = fields.next()?.parse().ok()?;
    let load: f64 = fields.next()?.parse().ok()?;
    (cores > 0.0).then(|| load / cores)
}

/// First host with the lowest load among those that answered.
fn least_loaded<T>(answers: impl IntoIterator<Item = (T, Option<f64>)>) -> Option<T> {
    let mut best: Option<(T, f64)> = None;
    for (host, load) in answers {
        let Some(load) = load else { continue };
        if best.as_ref().is_none_or(|(_, b)| load < *b) {
            best = Some((host, load));
        }
    }
    best.map(|(host, _)| host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_load_divides_by_cores() {
        assert_eq!(parse_load("deploy-rs-load 8 4.00\n"), Some(0.5));
    }

    #[test]
    fn parse_load_skips_login_noise() {
        let output = "motd line\nzsh: some warning\ndeploy-rs-load 4 1.0\n";
        assert_eq!(parse_load(output), Some(0.25));
    }

    #[test]
    fn parse_load_rejects_garbage() {
        assert_eq!(parse_load(""), None);
        assert_eq!(parse_load("deploy-rs-load x 1"), None);
        assert_eq!(parse_load("deploy-rs-load 0 1"), None);
    }

    #[test]
    fn least_loaded_skips_silent_hosts() {
        let answers = vec![("hogged", Some(3.0)), ("down", None), ("idle", Some(0.1))];
        assert_eq!(least_loaded(answers), Some("idle"));
    }

    #[test]
    fn least_loaded_keeps_the_first_on_a_tie() {
        let answers = vec![("a", Some(0.5)), ("b", Some(0.5))];
        assert_eq!(least_loaded(answers), Some("a"));
    }

    #[test]
    fn least_loaded_without_answers_is_none() {
        assert_eq!(least_loaded(vec![("down", None::<f64>)]), None);
    }
}
