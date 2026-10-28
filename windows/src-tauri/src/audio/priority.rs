// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Audio priority (AudioPriorityService.swift, MixerRoutingSupport.swift):
//! one ordered list of outputs and one of microphones, each with its own
//! switch. When a higher-priority device connects it becomes the default;
//! when the default one leaves, the next available takes over. A device the
//! lists have never ranked waits until Windows settles on it, then goes
//! first if it is the device in use and below the other hardware if not.
//! The lists and the last-known names live in `audio-priority.json`.

use std::collections::{HashMap, HashSet};

/// Long enough for Windows to switch to headphones it just connected,
/// which can land after the device itself appears.
pub const PLACEMENT_DELAY_MS: u64 = 2000;
/// Connect and disconnect bursts settle into one pass.
pub const ENFORCE_DELAY_MS: u64 = 250;
/// Defaults.audioPriorityMaxListSize.
const MAX_LIST: usize = 64;

/// Where a device goes in a list that has not ranked it yet. Virtual
/// devices play or record nothing on their own, so they never take the
/// place of hardware.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    BuiltIn,
    Hardware,
    Virtual,
}

/// Built in (HD Audio on the board), plugged in hardware (USB, Bluetooth),
/// or a virtual cable or mixer, from the bus that enumerated the device and
/// the names virtual drivers carry.
pub fn tier(enumerator: &str, name: &str) -> Tier {
    let name = name.to_lowercase();
    const VIRTUAL_NAMES: [&str; 6] = [
        "virtual",
        "vb-audio",
        "voicemeeter",
        "cable input",
        "cable output",
        "steam streaming",
    ];
    if VIRTUAL_NAMES.iter().any(|v| name.contains(v)) {
        return Tier::Virtual;
    }
    // A display's speakers ride the same HD Audio bus as the board's, but
    // the Mac ranks HDMI and DisplayPort with plugged-in hardware.
    const DISPLAY_NAMES: [&str; 5] = [
        "hdmi",
        "displayport",
        "display audio",
        "nvidia high definition",
        "amd high definition",
    ];
    let display = DISPLAY_NAMES.iter().any(|d| name.contains(d));
    match enumerator.to_uppercase().as_str() {
        "HDAUDIO" | "INTELAUDIO" | "ACPI" | "PCI" if !display => Tier::BuiltIn,
        "ROOT" | "SW" | "SWD" => Tier::Virtual,
        _ => Tier::Hardware,
    }
}

/// Ids are opaque, but an empty one or one with control characters is no id.
fn sanitized(id: &str) -> Option<&str> {
    let id = id.trim();
    (!id.is_empty() && !id.chars().any(char::is_control)).then_some(id)
}

/// The order a first-time list starts in: the device in use, then built-in
/// devices, then other hardware, and virtual ones last. Within a tier the
/// order Windows lists them in is kept.
pub fn initial_list(
    available: &[String],
    current: Option<&str>,
    tier: impl Fn(&str) -> Tier,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let unique: Vec<&str> = available
        .iter()
        .filter_map(|id| sanitized(id))
        .filter(|id| seen.insert(*id))
        .collect();
    let mut rest: Vec<(usize, &str)> = unique
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, id)| Some(*id) != current)
        .collect();
    rest.sort_by_key(|(offset, id)| (tier(id), *offset));
    unique
        .iter()
        .filter(|id| Some(**id) == current)
        .chain(rest.iter().map(|(_, id)| id))
        .map(|id| id.to_string())
        .collect()
}

/// Where a device the list has never seen joins it. The device in use goes
/// first, since Windows or the person just picked it; any other device goes
/// above the virtual entries, or last when it is one itself.
pub fn placing_new(
    id: &str,
    list: &[String],
    is_current: bool,
    tier: impl Fn(&str) -> Tier,
) -> Vec<String> {
    let Some(id) = sanitized(id) else {
        return list.to_vec();
    };
    if list.iter().any(|d| d == id) {
        return list.to_vec();
    }
    let mut placed = list.to_vec();
    if is_current {
        placed.insert(0, id.to_string());
        return placed;
    }
    match list.iter().position(|d| tier(d) == Tier::Virtual) {
        Some(first_virtual) if tier(id) != Tier::Virtual => {
            placed.insert(first_virtual, id.to_string())
        }
        _ => placed.push(id.to_string()),
    }
    placed
}

/// The first listed device that is connected now.
pub fn first_available<'a>(ordered: &'a [String], available: &HashSet<String>) -> Option<&'a str> {
    let mut seen = HashSet::new();
    ordered
        .iter()
        .filter_map(|id| sanitized(id))
        .filter(|id| seen.insert(*id))
        .find(|id| available.contains(*id))
}

/// Unique, valid and at most 64 long, as the Mac keeps its lists.
pub fn sanitized_list(ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.iter()
        .filter_map(|id| sanitized(id))
        .filter(|id| seen.insert(*id))
        .take(MAX_LIST)
        .map(str::to_string)
        .collect()
}

/// What is saved: both switches, both lists, and the last-known names of
/// listed devices, so an unplugged one still reads as itself.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub output_enabled: bool,
    pub input_enabled: bool,
    pub outputs: Vec<String>,
    pub inputs: Vec<String>,
    pub names: HashMap<String, String>,
}

impl Saved {
    /// Reads the saved file field by field, so one damaged field doesn't
    /// throw away the rest, and cleans the lists as the Mac does on load.
    pub fn from_value(value: &serde_json::Value) -> Self {
        let flag = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        };
        let list = |key: &str| {
            let ids: Vec<String> = value
                .get(key)
                .and_then(serde_json::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            sanitized_list(&ids)
        };
        let names = value
            .get("names")
            .and_then(serde_json::Value::as_object)
            .map(|o| {
                o.iter()
                    .filter_map(|(id, name)| Some((id.clone(), name.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        Saved {
            output_enabled: flag("outputEnabled"),
            input_enabled: flag("inputEnabled"),
            outputs: list("outputs"),
            inputs: list("inputs"),
            names,
        }
    }
}

/// One list's live side: the availability the last pass settled on, the
/// newest one waiting for the next pass, the devices still waiting for their
/// place, and whether a pass is owed.
#[derive(Default)]
pub struct Tracker {
    pub settled: Option<HashSet<String>>,
    pending: Option<HashSet<String>>,
    pub unplaced: HashSet<String>,
    /// The list or its switch changed.
    pub due: bool,
}

impl Tracker {
    /// Takes a fresh inventory. The first one that isn't empty is the
    /// baseline, not every device connecting at launch: Windows can answer
    /// with no devices while its audio service starts. Returns true when a
    /// pass should look at it.
    pub fn observe(&mut self, available: &HashSet<String>) -> bool {
        let Some(settled) = &self.settled else {
            if !available.is_empty() {
                self.settled = Some(available.clone());
            }
            return false;
        };
        // Back to where it settled before the pass still owes that pass,
        // which then finds nothing changed; an unchanged list owes none.
        let differs = settled != available || self.pending.as_ref().is_some_and(|p| p != available);
        self.pending = Some(available.clone());
        differs
    }

    /// At the pass: only the settled set against the final one counts, so a
    /// device that drops out and comes back inside the pass's delay isn't a
    /// hardware change (AudioPriorityService.enforceAvailabilityChanges).
    /// Returns whether this list owes a pass, and clears it.
    pub fn settle(&mut self) -> bool {
        let changed = match (self.pending.take(), &self.settled) {
            (Some(pending), Some(settled)) if pending != *settled => {
                self.settled = Some(pending);
                true
            }
            _ => false,
        };
        std::mem::take(&mut self.due) || changed
    }
}

/// Every connected device gets a place: a list that is still empty starts
/// in its initial order, and newcomers wait in `unplaced`. Returns true when
/// some newcomer now waits.
pub fn merge(
    list: &mut Vec<String>,
    tracker: &mut Tracker,
    available: &[String],
    current: Option<&str>,
    tier: impl Fn(&str) -> Tier,
) -> bool {
    if list.is_empty() {
        *list = sanitized_list(&initial_list(available, current, tier));
        return false;
    }
    let mut waiting = false;
    for id in available {
        if !list.contains(id) && tracker.unplaced.insert(id.clone()) {
            waiting = true;
        }
    }
    waiting
}

/// Gives each waiting device that is still connected its place.
pub fn place(
    list: &mut Vec<String>,
    tracker: &mut Tracker,
    available: &[String],
    current: Option<&str>,
    tier: impl Fn(&str) -> Tier,
) {
    let mut waiting: Vec<String> = tracker.unplaced.drain().collect();
    waiting.sort();
    for id in waiting {
        if !available.contains(&id) || list.contains(&id) {
            continue;
        }
        // A full list makes room by forgetting its lowest device that isn't
        // connected, so a newcomer is never cut off again on every pass.
        if list.len() >= MAX_LIST
            && let Some(gone) = list.iter().rposition(|d| !available.contains(d))
        {
            list.remove(gone);
        }
        *list = placing_new(&id, list, current == Some(id.as_str()), &tier);
    }
    *list = sanitized_list(list);
}

/// The device a due pass switches to, if any: none while Windows has just
/// moved to a device the list hasn't ranked yet, or when the best one is
/// already the default.
pub fn target<'a>(
    list: &'a [String],
    tracker: &Tracker,
    available: &HashSet<String>,
    current: Option<&str>,
) -> Option<&'a str> {
    if current.is_some_and(|c| tracker.unplaced.contains(c)) {
        return None;
    }
    first_available(list, available).filter(|t| Some(*t) != current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn tiers(id: &str) -> Tier {
        match id {
            "speakers" => Tier::BuiltIn,
            "cable" => Tier::Virtual,
            _ => Tier::Hardware,
        }
    }

    #[test]
    fn a_first_list_leads_with_the_device_in_use_then_built_in_hardware_virtual() {
        let list = initial_list(
            &ids(&["cable", "usb", "speakers", "bt", "usb"]),
            Some("bt"),
            tiers,
        );
        assert_eq!(list, ids(&["bt", "speakers", "usb", "cable"]));
        assert_eq!(
            initial_list(&ids(&["usb", "speakers"]), None, tiers),
            ids(&["speakers", "usb"])
        );
    }

    #[test]
    fn a_new_device_goes_first_when_in_use_and_above_virtual_ones_otherwise() {
        let list = ids(&["speakers", "cable"]);
        assert_eq!(
            placing_new("bt", &list, true, tiers),
            ids(&["bt", "speakers", "cable"])
        );
        assert_eq!(
            placing_new("bt", &list, false, tiers),
            ids(&["speakers", "bt", "cable"])
        );
        assert_eq!(
            placing_new("cable2", &ids(&["speakers"]), false, |_| Tier::Virtual),
            ids(&["speakers", "cable2"])
        );
        assert_eq!(placing_new("speakers", &list, true, tiers), list);
        assert_eq!(placing_new("  ", &list, true, tiers), list);
    }

    #[test]
    fn the_first_connected_device_of_the_list_wins() {
        let available: HashSet<String> = ids(&["speakers", "usb"]).into_iter().collect();
        assert_eq!(
            first_available(&ids(&["bt", "usb", "speakers"]), &available),
            Some("usb")
        );
        assert_eq!(first_available(&ids(&["bt"]), &available), None);
        assert_eq!(first_available(&[], &available), None);
    }

    fn set(v: &[&str]) -> HashSet<String> {
        ids(v).into_iter().collect()
    }

    #[test]
    fn the_first_inventory_is_a_baseline_and_later_changes_are_due() {
        let mut t = Tracker::default();
        assert!(!t.observe(&set(&["speakers"])));
        assert!(!t.settle());
        assert!(!t.observe(&set(&["speakers"])));
        assert!(t.observe(&set(&["speakers", "bt"])));
        assert!(t.settle());
        assert!(!t.settle());
    }

    #[test]
    fn an_empty_first_inventory_is_no_baseline() {
        let mut t = Tracker::default();
        assert!(!t.observe(&set(&[])));
        assert!(!t.observe(&set(&["speakers", "bt"])));
        assert!(!t.settle());
    }

    #[test]
    fn an_unchanged_inventory_owes_no_pass() {
        let mut t = Tracker::default();
        t.observe(&set(&["a"]));
        assert!(!t.observe(&set(&["a"])));
        assert!(!t.observe(&set(&["a"])));
    }

    #[test]
    fn a_device_back_before_the_pass_is_no_change() {
        let mut t = Tracker::default();
        t.observe(&set(&["a", "b"]));
        assert!(t.observe(&set(&["b"])));
        assert!(t.observe(&set(&["a", "b"])));
        assert!(!t.settle());
        // A list change still owes its pass.
        t.due = true;
        assert!(t.settle());
    }

    #[test]
    fn a_full_list_forgets_a_gone_device_for_a_newcomer() {
        let mut list: Vec<String> = (0..64).map(|i| format!("d{i}")).collect();
        let mut t = Tracker::default();
        let available: Vec<String> = ids(&["d0", "new"]);
        merge(&mut list, &mut t, &available, Some("d0"), |_| {
            Tier::Hardware
        });
        place(&mut list, &mut t, &available, Some("d0"), |_| {
            Tier::Hardware
        });
        assert_eq!(list.len(), 64);
        assert_eq!(list.last().map(String::as_str), Some("new"));
        assert!(!list.contains(&"d63".to_string()));
    }

    #[test]
    fn a_damaged_saved_field_keeps_the_others() {
        let saved = Saved::from_value(&serde_json::json!({
            "outputEnabled": true,
            "outputs": ["a", "a", " ", 3, "b"],
            "inputs": "x",
            "names": {"a": "Speakers", "b": 7}
        }));
        assert!(saved.output_enabled && !saved.input_enabled);
        assert_eq!(saved.outputs, ids(&["a", "b"]));
        assert!(saved.inputs.is_empty());
        assert_eq!(saved.names.len(), 1);
    }

    #[test]
    fn a_device_windows_just_moved_to_is_left_alone_until_placed() {
        let mut list = ids(&["speakers", "usb"]);
        let mut t = Tracker::default();
        let available_ids = ids(&["speakers", "usb", "bt"]);
        assert!(merge(&mut list, &mut t, &available_ids, Some("bt"), tiers));
        let available: HashSet<String> = available_ids.iter().cloned().collect();
        assert_eq!(target(&list, &t, &available, Some("bt")), None);
        place(&mut list, &mut t, &available_ids, Some("bt"), tiers);
        assert_eq!(list, ids(&["bt", "speakers", "usb"]));
        assert_eq!(target(&list, &t, &available, Some("bt")), None);
        // The person moves the speakers up; the next pass switches to them.
        list = ids(&["speakers", "bt", "usb"]);
        assert_eq!(target(&list, &t, &available, Some("bt")), Some("speakers"));
    }

    #[test]
    fn a_device_that_left_before_its_place_is_not_listed() {
        let mut list = ids(&["speakers"]);
        let mut t = Tracker::default();
        merge(
            &mut list,
            &mut t,
            &ids(&["speakers", "usb"]),
            Some("speakers"),
            tiers,
        );
        place(
            &mut list,
            &mut t,
            &ids(&["speakers"]),
            Some("speakers"),
            tiers,
        );
        assert_eq!(list, ids(&["speakers"]));
        assert!(t.unplaced.is_empty());
    }

    #[test]
    fn an_empty_list_starts_from_what_is_connected() {
        let mut list = Vec::new();
        let mut t = Tracker::default();
        assert!(!merge(
            &mut list,
            &mut t,
            &ids(&["usb", "speakers"]),
            Some("usb"),
            tiers
        ));
        assert_eq!(list, ids(&["usb", "speakers"]));
    }

    #[test]
    fn several_newcomers_are_placed_in_a_stable_order() {
        let mut list = ids(&["speakers", "cable"]);
        let mut t = Tracker::default();
        let available = ids(&["speakers", "cable", "usb2", "usb1", "bt"]);
        assert!(merge(&mut list, &mut t, &available, Some("bt"), tiers));
        // Seen again before placement: nothing new waits.
        assert!(!merge(&mut list, &mut t, &available, Some("bt"), tiers));
        place(&mut list, &mut t, &available, Some("bt"), tiers);
        assert_eq!(list, ids(&["bt", "speakers", "usb1", "usb2", "cable"]));
        assert!(t.unplaced.is_empty());
    }

    #[test]
    fn nothing_listed_and_connected_is_no_target() {
        let t = Tracker::default();
        let available: HashSet<String> = ids(&["speakers"]).into_iter().collect();
        assert_eq!(target(&[], &t, &available, Some("speakers")), None);
        assert_eq!(
            target(&ids(&["bt"]), &t, &available, Some("speakers")),
            None
        );
        assert_eq!(
            target(&ids(&["bt", "speakers"]), &t, &available, None),
            Some("speakers")
        );
    }

    #[test]
    fn a_full_list_keeps_its_length() {
        let mut list: Vec<String> = (0..64).map(|i| format!("d{i}")).collect();
        let mut t = Tracker::default();
        let available = ids(&["new"]);
        merge(&mut list, &mut t, &available, Some("new"), |_| {
            Tier::Hardware
        });
        place(&mut list, &mut t, &available, Some("new"), |_| {
            Tier::Hardware
        });
        assert_eq!(list.len(), 64);
        assert_eq!(list[0], "new");
        assert!(!list.contains(&"d63".to_string()));
    }

    #[test]
    fn tiers_follow_the_bus_and_virtual_names() {
        assert_eq!(
            tier("HDAUDIO", "Speakers (Realtek(R) Audio)"),
            Tier::BuiltIn
        );
        assert_eq!(tier("USB", "Headset (Jabra)"), Tier::Hardware);
        assert_eq!(tier("BTHENUM", "Headphones (WH-1000XM5)"), Tier::Hardware);
        assert_eq!(tier("ROOT", "Speakers"), Tier::Virtual);
        assert_eq!(
            tier("HDAUDIO", "LG ULTRAGEAR (NVIDIA High Definition Audio)"),
            Tier::Hardware
        );
        assert_eq!(
            tier("INTELAUDIO", "Monitor (Intel(R) Display Audio)"),
            Tier::Hardware
        );
        assert_eq!(
            tier("", "CABLE Input (VB-Audio Virtual Cable)"),
            Tier::Virtual
        );
        assert_eq!(tier("", "Speakers"), Tier::Hardware);
    }

    #[test]
    fn saved_lists_are_unique_and_bounded() {
        let many: Vec<String> = (0..80).map(|i| format!("d{i}")).collect();
        assert_eq!(sanitized_list(&many).len(), 64);
        assert_eq!(
            sanitized_list(&ids(&["a", "a", " ", "b"])),
            ids(&["a", "b"])
        );
        let saved = Saved::from_value(&serde_json::json!({"outputEnabled": true}));
        assert!(saved.output_enabled && !saved.input_enabled && saved.outputs.is_empty());
    }
}
