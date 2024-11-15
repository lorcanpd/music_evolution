use std::time::Duration;
use crate::genome::{
    BITS_PER_PARAMETER, PARAMETERS, NOTE_CODON, LOW_PASS_CODON, HIGH_PASS_CODON,
    REVERB_CODON, ECHO_CODON, NOTE_CODON_SIZE, EFFECT_CODON_SIZE};
use std::f32::consts::PI;

pub struct DecodedParameters {
    pub start_time: Duration,
    pub frequency: f32,
    pub amplitude: f32,
    pub duration: Duration,
    pub phase: f32,
}

pub struct DecodedGenome {
    pub notes: Vec<DecodedParameters>,
    pub effects: Vec<Effect>,
}

pub enum Effect {
    LowPass(f32),
    HighPass(f32),
    Reverb(Duration, f32),
    Echo(Duration, f32),
}

impl DecodedGenome {
    pub fn decode(left_chromosome: &[u8], right_chromosome: &[u8]) -> Self {
        let mut notes = Vec::new();
        let mut effects = Vec::new();

        // Decode notes from the left chromosome
        let mut i = 0;
        while i < left_chromosome.len() {
            if i + NOTE_CODON_SIZE <= left_chromosome.len() && matches_codon(
                &left_chromosome[i..i + NOTE_CODON_SIZE], &NOTE_CODON) {
                if i + PARAMETERS * BITS_PER_PARAMETER <= left_chromosome.len() {
                    i += NOTE_CODON_SIZE;
                    let params = decode_parameters(
                        &left_chromosome[i..i + PARAMETERS * BITS_PER_PARAMETER]);
                    notes.push(params);
                    i += PARAMETERS * BITS_PER_PARAMETER;
                } else {
                    break
                }
            } else {
                i += 1;
            }
        }

        // Decode effects from the right chromosome
        let mut j = 0;
        while j < right_chromosome.len() {
            if let Some((effect, effect_size)) = decode_effects(&right_chromosome[j..]) {
                effects.push(effect);
                j += effect_size; // Advance by the size of the effect codon and parameters
            } else {
                j += 1;
            }
        }

        DecodedGenome { notes, effects }
    }
}

fn decode_parameters(bits: &[u8]) -> DecodedParameters {
    let start_time = bits_to_duration(&bits[0..8]);
    let frequency = bits_to_frequency(&bits[8..16]);
    let amplitude = bits_to_amplitude(&bits[16..24]);
    let duration = bits_to_duration(&bits[24..32]);
    let phase = bits_to_phase(&bits[32..40]);
    DecodedParameters {
        start_time,
        frequency,
        amplitude,
        duration,
        phase,
    }
}

// fn decode_effects(bits: &[u8]) -> Option<Effect> {
//     if matches_codon(bits, &LOW_PASS_CODON) {
//         Some(Effect::LowPass(bits_to_amplitude(&bits[9..17])))
//     } else if matches_codon(bits, &HIGH_PASS_CODON) {
//         Some(Effect::HighPass(bits_to_amplitude(&bits[9..17])))
//     } else if matches_codon(bits, &REVERB_CODON) {
//         Some(Effect::Reverb(
//             bits_to_duration(&bits[9..17]),
//             bits_to_amplitude(&bits[17..25]),
//         ))
//     } else if matches_codon(bits, &ECHO_CODON) {
//         Some(Effect::Echo(
//             bits_to_duration(&bits[9..17]),
//             bits_to_amplitude(&bits[17..25]),
//         ))
//     } else {
//         None
//     }
// }

fn decode_effects(bits: &[u8]) -> Option<(Effect, usize)> {
    if bits.len() >= EFFECT_CODON_SIZE {
        if matches_codon(&bits[0..EFFECT_CODON_SIZE], &LOW_PASS_CODON) {
            let total_size = EFFECT_CODON_SIZE + BITS_PER_PARAMETER;
            if bits.len() >= total_size {
                let cutoff = bits_to_amplitude(&bits[EFFECT_CODON_SIZE..total_size]);
                Some((Effect::LowPass(cutoff), total_size))
            } else {
                None
            }
        } else if matches_codon(&bits[0..EFFECT_CODON_SIZE], &HIGH_PASS_CODON) {
            let total_size = EFFECT_CODON_SIZE + BITS_PER_PARAMETER;
            if bits.len() >= total_size {
                let cutoff = bits_to_amplitude(&bits[EFFECT_CODON_SIZE..total_size]);
                Some((Effect::HighPass(cutoff), total_size))
            } else {
                None
            }
        } else if matches_codon(&bits[0..EFFECT_CODON_SIZE], &REVERB_CODON) {
            let total_size = EFFECT_CODON_SIZE + 2 * BITS_PER_PARAMETER;
            if bits.len() >= total_size {
                let delay = bits_to_duration(&bits[EFFECT_CODON_SIZE..EFFECT_CODON_SIZE + BITS_PER_PARAMETER]);
                let feedback = bits_to_amplitude(&bits[EFFECT_CODON_SIZE + BITS_PER_PARAMETER..total_size]);
                Some((Effect::Reverb(delay, feedback), total_size))
            } else {
                None
            }
        } else if matches_codon(&bits[0..EFFECT_CODON_SIZE], &ECHO_CODON) {
            let total_size = EFFECT_CODON_SIZE + 2 * BITS_PER_PARAMETER;
            if bits.len() >= total_size {
                let delay = bits_to_duration(&bits[EFFECT_CODON_SIZE..EFFECT_CODON_SIZE + BITS_PER_PARAMETER]);
                let feedback = bits_to_amplitude(&bits[EFFECT_CODON_SIZE + BITS_PER_PARAMETER..total_size]);
                Some((Effect::Echo(delay, feedback), total_size))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}


    fn bits_to_frequency(bits: &[u8]) -> f32 {
    let value = bits_to_value(bits);
    value as f32 * 5.0 // Frequency range from 0 to 1275 Hz
}

fn bits_to_amplitude(bits: &[u8]) -> f32 {
    let value = bits_to_value(bits);
    value as f32 / 128.0 // Normalized amplitude between 0.0 and 1.0
}

fn bits_to_duration(bits: &[u8]) -> Duration {
    let value = bits_to_value(bits);
    // Duration range from 0 to 20400 ms
    Duration::from_millis(value as u64 * 20)
}

fn bits_to_phase(bits: &[u8]) -> f32 {
    let value = bits_to_value(bits);
    value as f32 * 2.0 * PI / 255.0 // Phase between 0 and 2π
}

// fn bits_to_value(bits: &[u8]) -> u8 {
//     bits.iter().rev().enumerate().fold(0, |acc, (i, &bit)| {
//         if i < 8 { // Ensure we do not exceed the u8 size
//             acc + (bit << i)
//         } else {
//             acc
//         }
//     })
// }

fn bits_to_value(bits: &[u8]) -> u32 {
    bits.iter().rev().enumerate().fold(0u32, |acc, (i, &bit)| {
        acc + ((bit as u32) << i)
    })
}

fn matches_codon(segment: &[u8], codon: &[u8]) -> bool {
    segment.len() == codon.len() && segment.iter().zip(codon.iter()).all(|(&a, &b)| a == b)
}
