// SPDX-License-Identifier: GPL-3.0-or-later
//! Software gain for ALSA's WAV-only player, which has no per-stream volume flag.
//! Only samples change; neither ALSA mixer controls nor other applications do.
pub fn scaled_wav(input: &[u8], gain: f64) -> Option<Vec<u8>> {
    if input.get(..4)? != b"RIFF" || input.get(8..12)? != b"WAVE" || !gain.is_finite() {
        return None;
    }
    let mut format = None;
    let mut data = Vec::new();
    let mut offset = 12usize;
    while offset.checked_add(8)? <= input.len() {
        let count =
            u32::from_le_bytes(input.get(offset + 4..offset + 8)?.try_into().ok()?) as usize;
        let start = offset.checked_add(8)?;
        let end = start.checked_add(count)?;
        let chunk = input.get(start..end)?;
        match &input[offset..offset + 4] {
            b"fmt " if count >= 16 => {
                let mut tag = u16::from_le_bytes(chunk[0..2].try_into().ok()?);
                if tag == 0xfffe && count >= 40 {
                    tag = u16::from_le_bytes(chunk[24..26].try_into().ok()?);
                }
                format = Some((tag, u16::from_le_bytes(chunk[14..16].try_into().ok()?)));
            }
            b"data" => data.push(start..end),
            _ => {}
        }
        offset = end.checked_add(count % 2)?;
    }
    let (tag, bits) = format?;
    if !(tag == 1 && [8, 16, 24, 32].contains(&bits) || tag == 3 && bits == 32) || data.is_empty() {
        return None;
    }
    let gain = gain.clamp(0.0, 1.0);
    let size = usize::from(bits / 8);
    let mut output = input.to_vec();
    for range in data {
        if range.len() % size != 0 {
            return None;
        }
        for sample in output[range].chunks_exact_mut(size) {
            if tag == 3 {
                let value = f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]);
                sample.copy_from_slice(
                    &(if value.is_finite() {
                        value * gain as f32
                    } else {
                        0.0
                    })
                    .to_le_bytes(),
                );
            } else if bits == 8 {
                sample[0] = (((f64::from(sample[0]) - 128.0) * gain).round() + 128.0) as u8;
            } else {
                let mut bytes = if sample[size - 1] & 0x80 != 0 {
                    [0xff; 4]
                } else {
                    [0; 4]
                };
                bytes[..size].copy_from_slice(sample);
                let value = (f64::from(i32::from_le_bytes(bytes)) * gain).round() as i32;
                sample.copy_from_slice(&value.to_le_bytes()[..size]);
            }
        }
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn volume_changes_only_samples_and_zero_is_silent() {
        let (_, wav) = crate::sound::builtin_sound(None);
        let half = scaled_wav(wav, 0.5).unwrap();
        let zero = scaled_wav(wav, 0.0).unwrap();
        assert_eq!(&half[..44], &wav[..44]);
        assert!(zero[44..].iter().all(|byte| *byte == 0));
        let peak = |bytes: &[u8]| {
            bytes[44..]
                .chunks_exact(2)
                .map(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs())
                .max()
                .unwrap()
        };
        assert!((f64::from(peak(&half)) / f64::from(peak(wav)) - 0.5).abs() < 0.001);
        assert_eq!(scaled_wav(wav, 1.0).unwrap(), wav);
        assert!(scaled_wav(&wav[..40], 1.0).is_none());
        assert!(scaled_wav(wav, f64::NAN).is_none());
    }
}
