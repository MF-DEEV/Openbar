// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Bounded, in-memory Hann-windowed FFT. No audio leaves the capture thread.
pub const BANDS: usize = 7;
const N: usize = 2048;
const EDGES: [f32; 8] = [50., 120., 250., 500., 1000., 2000., 4500., 12000.];
// Leave room for a player's volume attenuation before the signal becomes
// inaudible to the meter. Digital silence still maps to exactly zero.
const FLOOR_DB: f32 = 72.;

pub struct Spectrum {
    ring: [f32; N],
    cursor: usize,
    window: [f32; N],
    roots: [(f32, f32); N / 2],
    rate: f32,
}

impl Spectrum {
    pub fn new(rate: u32) -> Self {
        Self {
            ring: [0.; N],
            cursor: 0,
            rate: rate as f32,
            window: std::array::from_fn(|i| {
                0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()
            }),
            roots: std::array::from_fn(|i| {
                let phase = -std::f32::consts::TAU * i as f32 / N as f32;
                (phase.cos(), phase.sin())
            }),
        }
    }

    pub fn push(&mut self, sample: f32) {
        self.ring[self.cursor] = if sample.is_finite() {
            sample.clamp(-1., 1.)
        } else {
            0.
        };
        self.cursor = (self.cursor + 1) % N;
    }

    pub fn clear(&mut self) {
        self.ring.fill(0.);
    }

    pub fn levels(&self) -> [f32; BANDS] {
        let mut re = [0.; N];
        let mut im = [0.; N];
        for i in 0..N {
            let reversed = i.reverse_bits() >> (usize::BITS - N.ilog2());
            re[reversed] = self.ring[(self.cursor + i) % N] * self.window[i];
        }
        let mut size = 2;
        while size <= N {
            for base in (0..N).step_by(size) {
                for j in 0..size / 2 {
                    let (wr, wi) = self.roots[j * N / size];
                    let a = base + j;
                    let b = a + size / 2;
                    let tr = wr * re[b] - wi * im[b];
                    let ti = wr * im[b] + wi * re[b];
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            size *= 2;
        }
        std::array::from_fn(|band| {
            let lo = ((EDGES[band] * N as f32 / self.rate) as usize).clamp(1, N / 2 - 1);
            let hi = ((EDGES[band + 1] * N as f32 / self.rate) as usize).clamp(lo + 1, N / 2);
            // Integrate band energy instead of averaging magnitudes. Averaging
            // hundreds of treble bins diluted claps/cymbals compared with the
            // few bass bins. Hann power compensation yields full-scale RMS.
            let power = (lo..hi).map(|i| re[i] * re[i] + im[i] * im[i]).sum::<f32>();
            let rms = (power * 16. / (3. * (N * N) as f32)).sqrt();
            // Fixed floor preserves the difference between quiet and loud music.
            if rms > 0. {
                ((20. * rms.log10() + FLOOR_DB) / FLOOR_DB).clamp(0., 1.)
            } else {
                0.
            }
        })
    }
}

#[derive(Default)]
pub struct Smoother {
    levels: [f32; BANDS],
    previous: [f32; BANDS],
}
impl Smoother {
    pub fn next(&mut self, raw: [f32; BANDS]) -> [f32; BANDS] {
        for (index, raw) in raw.into_iter().enumerate() {
            let raw = if raw.is_finite() {
                raw.clamp(0., 1.)
            } else {
                0.
            };
            // Positive spectral change makes real hits jump above the sustained
            // level. No clock, random motion, or automatic gain on silent audio.
            let onset = (raw - self.previous[index] - 0.025).max(0.);
            self.previous[index] = raw;
            // Lift quiet music rather than suppressing it with an exponent > 1.
            // This stays monotonic: louder audio still gives taller bars.
            let target = (raw.powf(0.85) + onset * 1.2).min(1.);
            let value = &mut self.levels[index];
            *value += (target - *value) * if target > *value { 0.9 } else { 0.28 };
            if *value < 0.001 {
                *value = 0.;
            }
        }
        self.levels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tone(hz: f32, amplitude: f32) -> [f32; BANDS] {
        let mut spectrum = Spectrum::new(48000);
        for i in 0..N {
            spectrum.push(amplitude * (std::f32::consts::TAU * hz * i as f32 / 48000.).sin());
        }
        spectrum.levels()
    }
    #[test]
    fn tones_land_in_their_frequency_bands() {
        for (hz, expected) in [
            (90., 0),
            (180., 1),
            (375., 2),
            (750., 3),
            (1500., 4),
            (3000., 5),
            (7000., 6),
        ] {
            let levels = tone(hz, 0.8);
            let strongest = (0..BANDS)
                .max_by(|&a, &b| levels[a].total_cmp(&levels[b]))
                .unwrap();
            assert_eq!(strongest, expected, "{hz}: {levels:?}");
        }
    }
    #[test]
    fn volume_silence_and_invalid_samples() {
        assert!(tone(750., 0.8)[3] > tone(750., 0.02)[3]);
        let mut spectrum = Spectrum::new(48000);
        for _ in 0..N {
            spectrum.push(f32::NAN);
        }
        assert_eq!(spectrum.levels(), [0.; BANDS]);
        spectrum.push(1.);
        spectrum.clear();
        assert_eq!(spectrum.levels(), [0.; BANDS]);
    }
    #[test]
    fn attack_is_fast_and_silence_decays_to_zero() {
        let mut smoother = Smoother::default();
        assert_eq!(smoother.next([1.; BANDS]), [0.9; BANDS]);
        assert!(smoother.next([0.; BANDS])[0] < 0.9);
        for _ in 0..90 {
            smoother.next([0.; BANDS]);
        }
        assert_eq!(smoother.levels, [0.; BANDS]);
    }

    #[test]
    fn treble_energy_is_not_diluted_by_the_number_of_bins() {
        let bass = tone(90., 0.2)[0];
        let treble = tone(7000., 0.2)[6];
        assert!((bass - treble).abs() < 0.06, "bass={bass}, treble={treble}");
    }

    #[test]
    fn quiet_player_volume_keeps_visible_motion_without_animating_silence() {
        let mut smoother = Smoother::default();
        let quiet = tone(750., 0.003);
        for _ in 0..60 {
            smoother.next(quiet);
        }
        let quiet_height = smoother.levels[3];
        assert!(
            quiet_height > 0.25,
            "quiet music was barely visible: {quiet_height}"
        );
        let loud = tone(750., 0.3);
        for _ in 0..60 {
            smoother.next(loud);
        }
        assert!(smoother.levels[3] > quiet_height + 0.3);
        for _ in 0..40 {
            smoother.next([0.; BANDS]);
        }
        assert_eq!(smoother.levels, [0.; BANDS]);
        assert_eq!(
            tone(750., 0.00001),
            [0.; BANDS],
            "sub-floor noise must stay still"
        );
    }

    #[test]
    fn real_hits_jump_and_steady_audio_does_not_keep_bouncing() {
        let mut smoother = Smoother::default();
        let hit = smoother.next([0.5; BANDS])[0];
        for _ in 0..30 {
            smoother.next([0.5; BANDS]);
        }
        let steady = smoother.next([0.5; BANDS])[0];
        assert!(hit > steady + 0.3);
        assert!((smoother.next([0.5; BANDS])[0] - steady).abs() < 0.001);
        for _ in 0..30 {
            smoother.next([0.; BANDS]);
        }
        assert_eq!(smoother.next([0.; BANDS]), [0.; BANDS]);
    }
}
