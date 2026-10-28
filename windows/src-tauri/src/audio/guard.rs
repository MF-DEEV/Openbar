// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Lower volume when headphones disconnect (AppVolumeMixer
//! `loweringOutputVolumeIfHeadphonesDisconnected`): when the headphones that
//! were playing go away and Windows falls back to speakers, the speakers are
//! set to the chosen level, and muted at 0%, which some speakers still play
//! faintly. When headphones are the output again, the speakers get back the
//! level they had, unless someone changed it since.

/// Endpoint form factors that are worn (EndpointFormFactor Headphones, Headset).
const WORN_FORM_FACTORS: [u32; 2] = [3, 5];

/// MixerRoutingSupport.outputLooksLikeHeadphones, with Windows' own form
/// factor first: Bluetooth and USB headphones report it, many jacks too.
/// The Mac also reads a device's data source to catch a jack that switches
/// inside one device; Windows gives a jack-sensing endpoint its own form
/// factor instead, and a driver that keeps one endpoint for both isn't seen.
pub fn looks_like_headphones(name: &str, form_factor: Option<u32>) -> bool {
    if form_factor.is_some_and(|f| WORN_FORM_FACTORS.contains(&f)) {
        return true;
    }
    let normalized: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    const TERMS: [&str; 19] = [
        "headphone",
        "headphones",
        "headset",
        "earphone",
        "earphones",
        "earbud",
        "earbuds",
        "airpod",
        "airpods",
        "earpod",
        "earpods",
        "galaxy buds",
        "pixel buds",
        "beats",
        "bose qc",
        "sony wh",
        "sony wf",
        "jabra",
        "soundcore",
    ];
    TERMS.iter().any(|t| normalized.contains(t))
}

/// Defaults.sanitizedMixerHeadphonesDisconnectVolumePercent; 25 by default.
pub const DEFAULT_PERCENT: u8 = 25;

pub fn sanitized_percent(percent: i64) -> u8 {
    percent.clamp(0, 100) as u8
}

/// One output as the guard sees it.
pub struct Output<'a> {
    pub id: &'a str,
    pub headphones: bool,
}

/// What the guard lowered, to put back later.
#[derive(Clone, Debug, PartialEq)]
pub struct Lowered {
    pub id: String,
    pub previous: f32,
    pub applied: f32,
    pub muted_by_lowering: bool,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    None,
    /// Set this output to `volume`, and mute it when `mute` (a 0% level).
    Lower {
        id: String,
        volume: f32,
        mute: bool,
    },
    /// Headphones are back: return this output to what it had.
    Restore(Lowered),
}

#[derive(Default)]
pub struct Guard {
    /// The headphones that were the output last, while they still are or
    /// until a fallback from them is handled.
    headphones: Option<String>,
    pub lowered: Option<Lowered>,
}

impl Guard {
    /// Called with every fresh device list. Windows can publish an
    /// in-between list with no default while it moves the output, so the
    /// headphones that were playing are remembered until the fallback lands.
    pub fn observe(
        &mut self,
        outputs: &[Output],
        default: Option<&str>,
        enabled: bool,
        percent: u8,
    ) -> Action {
        let Some(default) = default else {
            return Action::None;
        };
        let Some(current) = outputs.iter().find(|o| o.id == default) else {
            return Action::None;
        };
        if current.headphones {
            self.headphones = Some(current.id.to_string());
            return match self.lowered.take() {
                Some(lowered) if outputs.iter().any(|o| o.id == lowered.id) => {
                    Action::Restore(lowered)
                }
                // Not around to restore right now; it may come back.
                other => {
                    self.lowered = other;
                    Action::None
                }
            };
        }
        let Some(headphones) = self.headphones.take() else {
            return Action::None;
        };
        // Moved off headphones that are still connected: the person's choice.
        if outputs.iter().any(|o| o.id == headphones && o.headphones) {
            return Action::None;
        }
        // Taking the headphones above means one fallback lowers only once.
        if !enabled {
            return Action::None;
        }
        Action::Lower {
            id: default.to_string(),
            volume: f32::from(percent) / 100.0,
            mute: percent == 0,
        }
    }

    /// The lowering was applied: `previous` is the level before it, and
    /// `muted` whether this set a mute the output didn't have. Only a real
    /// drop (or that mute) is owed back.
    pub fn lowered(&mut self, id: &str, previous: Option<f32>, applied: f32, muted: bool) {
        if let Some(previous) = previous
            && (previous > applied || muted)
        {
            self.lowered = Some(Lowered {
                id: id.to_string(),
                previous,
                applied,
                muted_by_lowering: muted,
            });
        }
    }
}

/// MixerRoutingSupport.shouldRestoreOutputVolume: only while the level is
/// still the one the guard set; anything else was someone's choice.
pub fn should_restore(lowered: &Lowered, current: Option<f32>) -> bool {
    current.is_some_and(|c| (c - lowered.applied).abs() < 0.005)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outs<'a>(list: &[(&'a str, bool)]) -> Vec<Output<'a>> {
        list.iter()
            .map(|(id, headphones)| Output {
                id,
                headphones: *headphones,
            })
            .collect()
    }

    #[test]
    fn headphones_are_told_by_form_factor_or_name() {
        assert!(looks_like_headphones("Speakers", Some(3)));
        assert!(looks_like_headphones(
            "Headset Earphone (Jabra Evolve2)",
            None
        ));
        assert!(looks_like_headphones("Słuchawki (AirPods Pro)", Some(1)));
        assert!(looks_like_headphones(
            "Headphones (WH-1000XM5 Stereo)",
            None
        ));
        assert!(!looks_like_headphones(
            "Speakers (Realtek(R) Audio)",
            Some(1)
        ));
        assert!(!looks_like_headphones(
            "Monitor (NVIDIA High Definition Audio)",
            None
        ));
    }

    #[test]
    fn a_disconnect_lowers_the_fallback_once() {
        let mut g = Guard::default();
        let both = outs(&[("hp", true), ("spk", false)]);
        assert_eq!(g.observe(&both, Some("hp"), true, 25), Action::None);
        // Windows passes through a list with no default.
        let alone = outs(&[("spk", false)]);
        assert_eq!(g.observe(&alone, None, true, 25), Action::None);
        let action = g.observe(&alone, Some("spk"), true, 25);
        assert_eq!(
            action,
            Action::Lower {
                id: "spk".into(),
                volume: 0.25,
                mute: false
            }
        );
        g.lowered("spk", Some(0.8), 0.25, false);
        assert_eq!(g.observe(&alone, Some("spk"), true, 25), Action::None);
    }

    #[test]
    fn switching_away_from_connected_headphones_lowers_nothing() {
        let mut g = Guard::default();
        let both = outs(&[("hp", true), ("spk", false)]);
        g.observe(&both, Some("hp"), true, 25);
        assert_eq!(g.observe(&both, Some("spk"), true, 25), Action::None);
        // Later unplugging them while the speakers play changes nothing.
        assert_eq!(
            g.observe(&outs(&[("spk", false)]), Some("spk"), true, 25),
            Action::None
        );
    }

    #[test]
    fn turned_off_it_never_lowers() {
        let mut g = Guard::default();
        g.observe(
            &outs(&[("hp", true), ("spk", false)]),
            Some("hp"),
            false,
            25,
        );
        assert_eq!(
            g.observe(&outs(&[("spk", false)]), Some("spk"), false, 25),
            Action::None
        );
    }

    #[test]
    fn zero_percent_also_mutes() {
        let mut g = Guard::default();
        g.observe(&outs(&[("hp", true), ("spk", false)]), Some("hp"), true, 0);
        assert_eq!(
            g.observe(&outs(&[("spk", false)]), Some("spk"), true, 0),
            Action::Lower {
                id: "spk".into(),
                volume: 0.0,
                mute: true
            }
        );
    }

    #[test]
    fn headphones_coming_back_restore_the_speakers() {
        let mut g = Guard::default();
        let both = outs(&[("hp", true), ("spk", false)]);
        g.observe(&both, Some("hp"), true, 25);
        g.observe(&outs(&[("spk", false)]), Some("spk"), true, 25);
        g.lowered("spk", Some(0.8), 0.25, false);
        let Action::Restore(lowered) = g.observe(&both, Some("hp"), true, 25) else {
            panic!("expected a restore");
        };
        assert_eq!(lowered.previous, 0.8);
        assert!(should_restore(&lowered, Some(0.25)));
        assert!(!should_restore(&lowered, Some(0.4)));
        assert!(!should_restore(&lowered, None));
        assert!(g.lowered.is_none());
        // Unplugged again: the speakers are lowered again.
        assert!(matches!(
            g.observe(&outs(&[("spk", false)]), Some("spk"), true, 25),
            Action::Lower { .. }
        ));
    }

    #[test]
    fn a_level_already_low_is_not_owed_back() {
        let mut g = Guard::default();
        g.lowered("spk", Some(0.1), 0.25, false);
        assert!(g.lowered.is_none());
        g.lowered("spk", Some(0.3), 0.0, true);
        assert_eq!(g.lowered.as_ref().map(|l| l.muted_by_lowering), Some(true));
    }

    #[test]
    fn headphones_still_named_the_default_after_leaving_are_remembered() {
        let mut g = Guard::default();
        g.observe(&outs(&[("hp", true), ("spk", false)]), Some("hp"), true, 25);
        // The list no longer has them, but Windows hasn't moved yet.
        let alone = outs(&[("spk", false)]);
        assert_eq!(g.observe(&alone, Some("hp"), true, 25), Action::None);
        assert!(matches!(
            g.observe(&alone, Some("spk"), true, 25),
            Action::Lower { .. }
        ));
    }

    #[test]
    fn a_lowered_output_away_when_headphones_return_is_restored_later() {
        let mut g = Guard::default();
        g.observe(&outs(&[("hp", true), ("spk", false)]), Some("hp"), true, 25);
        g.observe(&outs(&[("spk", false)]), Some("spk"), true, 25);
        g.lowered("spk", Some(0.8), 0.25, false);
        // The speakers are unplugged too before the headphones come back.
        assert_eq!(
            g.observe(&outs(&[("hp", true)]), Some("hp"), true, 25),
            Action::None
        );
        assert!(g.lowered.is_some());
        let Action::Restore(lowered) =
            g.observe(&outs(&[("hp", true), ("spk", false)]), Some("hp"), true, 25)
        else {
            panic!("expected a restore");
        };
        assert_eq!(lowered.id, "spk");
    }

    #[test]
    fn an_unknown_earlier_level_is_not_owed_back() {
        let mut g = Guard::default();
        g.lowered("spk", None, 0.25, false);
        assert!(g.lowered.is_none());
        // Nothing to restore when the headphones return.
        assert_eq!(
            g.observe(&outs(&[("hp", true), ("spk", false)]), Some("hp"), true, 25),
            Action::None
        );
    }

    #[test]
    fn percents_are_clamped() {
        assert_eq!(sanitized_percent(-5), 0);
        assert_eq!(sanitized_percent(250), 100);
        assert_eq!(sanitized_percent(40), 40);
    }
}
