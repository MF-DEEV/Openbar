// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Claude Code's session records on Linux: whose they are, and when their
//! process began. `sessions/registry.rs` and `sessions/service.rs` hand over
//! to this where Windows has its own `pidDomain` and a date for `procStart`.

use super::process;

/// Linux names the machine and the pid namespace, like
/// `linux:8474584911d3444fb483d450f2ab879f:pid:[4026531836]`: another machine
/// sharing the folder, or a container, writes another. None when this machine
/// has no id to compare with.
fn local_pid_domain() -> Option<String> {
    let id = ["/etc/machine-id", "/var/lib/dbus/machine-id"]
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok())?;
    let namespace = std::fs::read_link("/proc/self/ns/pid").ok()?;
    Some(format!(
        "linux:{}:{}",
        id.trim(),
        namespace.to_string_lossy()
    ))
}

/// The `pidDomain`s of records written on this machine.
pub fn pid_domains() -> &'static [&'static str] {
    static DOMAINS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    DOMAINS.get_or_init(|| {
        local_pid_domain()
            .map(|domain| &*Box::leak(domain.into_boxed_str()))
            .into_iter()
            .collect()
    })
}

/// On Linux `procStart` is the clock ticks after boot at which the process
/// began, as `/proc/<pid>/stat` has it, like "2012813".
pub fn proc_start_ticks(text: &str) -> Option<f64> {
    process::epoch_of_ticks(text.parse().ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::registry::{RegistryFile, record, scan};

    // A record as Claude Code writes it on Linux: the domain names the machine
    // and the pid namespace, procStart is the start in clock ticks since boot.
    #[test]
    fn a_linux_record_counts_while_its_process_runs() {
        let pid = std::process::id();
        let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
        let ticks = stat[stat.rfind(')').unwrap() + 1..]
            .split_whitespace()
            .nth(19)
            .unwrap()
            .to_owned();
        let domains = pid_domains();
        assert_eq!(domains.len(), 1, "this machine has an id and a namespace");
        assert!(domains[0].starts_with("linux:") && domains[0].contains(":pid:["));
        let write = |domain: &str| {
            format!(
                r#"{{"pid":{pid},"sessionId":"s","cwd":"/a","procStart":"{ticks}","pidDomain":"{domain}","startedAt":1}}"#
            )
        };
        let own = write(domains[0]);
        let parsed = record(own.as_bytes(), domains).unwrap();
        assert_eq!(parsed.started, process::started_at(pid));
        let files = [RegistryFile {
            name: format!("{pid}.json"),
            data: Some(own.into_bytes()),
        }];
        let found = scan(&files, domains, |p| process::started_at(p as u32));
        assert_eq!(found.records.len(), 1, "the running session is listed");
        let other = write("linux:00000000000000000000000000000000:pid:[1]");
        assert!(
            record(other.as_bytes(), domains).is_none(),
            "another machine or container is left out"
        );
        let reused = write(domains[0]).replace(&format!("\"{ticks}\""), "\"1\"");
        let files = [RegistryFile {
            name: format!("{pid}.json"),
            data: Some(reused.into_bytes()),
        }];
        let found = scan(&files, domains, |p| process::started_at(p as u32));
        assert!(
            found.records.is_empty(),
            "a pid that came back for another process is not the session"
        );
    }
}
