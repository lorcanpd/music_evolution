use std::time::Duration;
use crate::genome::{Genome, BITS_PER_PARAMETER, PARAMETERS};
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
    pub fn decode(genome: &Genome) -> Self {
        let mut notes = Vec::new();
        let mut effects = Vec::new();

        // Decode the note chromosome.
        let note_chromosome = genome.notes.get_left_chromosome();
        let note_codons = vec![
            genome.sine_codon.get_left_chromosome(),
            genome.square_codon.get_left_chromosome(),
            genome.custom_codon.get_left_chromosome(),
        ];

        notes.extend(decode_chromosome(note_chromosome, &note_codons));

        // Decode the effect chromosome.
        let effect_chromosome = genome.effects.get_left_chromosome();
        let effect_codons = vec![
            (Effect::LowPass(0.0), genome.low_pass_codon.get_left_chromosome()),
            (Effect::HighPass(0.0), genome.high_pass_codon.get_left_chromosome()),
            (Effect::Reverb(Duration::from_secs(0), 0.0), genome.reverb_codon.get_left_chromosome()),
            (Effect::Echo(Duration::from_secs(0), 0.0), genome.echo_codon.get_left_chromosome()),
        ];
        effects.extend(decode_effect_chromosome(effect_chromosome, &effect_codons));

        DecodedGenome { notes, effects }
    }
}

fn decode_chromosome(chromosome: &[u8], codons: &[&[u8]]) -> Vec<DecodedParameters> {
    let mut decoded_params = Vec::new();
    let param_length = PARAMETERS * BITS_PER_PARAMETER;
    let mut i = 0;

    while i < chromosome.len() {
        for codon in codons {
            if i + codon.len() <= chromosome.len() && matches_codon(
                &chromosome[i..i + codon.len()], codon
            ) {
                if i + param_length <= chromosome.len() {
                    i += codon.len();
                    let params = decode_parameters(
                        &chromosome[i..i + param_length]
                    );
                    decoded_params.push(params);
                    i += param_length;
                } else {
                    break;
                }
            }
        }
        i += 1;
    }

    decoded_params
}

fn decode_effect_chromosome(chromosome: &[u8], codons: &[(Effect, &[u8])]) -> Vec<Effect> {
    let mut effects = Vec::new();
    let mut i = 0;

    while i < chromosome.len() {
        if let Some((effect, effect_size)) = decode_effects(&chromosome[i..], codons) {
            effects.push(effect);
            i += effect_size;
        } else {
            i += 1;
        }
    }

    effects
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

fn decode_effects(bits: &[u8], codons: &[(Effect, &[u8])]) -> Option<(Effect, usize)> {
    for (effect, codon) in codons {
        let codon_size = codon.len();
        if bits.len() >= codon_size && matches_codon(&bits[0..codon_size], codon) {
            let total_size = match effect {
                Effect::LowPass(_) | Effect::HighPass(_) => codon_size + BITS_PER_PARAMETER,
                Effect::Reverb(_, _) | Effect::Echo(_, _) => codon_size + 2 * BITS_PER_PARAMETER,
            };
            if bits.len() >= total_size {
                let effect_instance = match effect {
                    Effect::LowPass(_) => Effect::LowPass(
                        bits_to_amplitude(&bits[codon_size..total_size])
                    ),
                    Effect::HighPass(_) => Effect::HighPass(
                        bits_to_amplitude(&bits[codon_size..total_size])
                    ),
                    Effect::Reverb(_, _) => {
                        let delay = bits_to_duration(
                            &bits[codon_size..codon_size + BITS_PER_PARAMETER]
                        );
                        let feedback = bits_to_amplitude(
                            &bits[codon_size + BITS_PER_PARAMETER..total_size]
                        );
                        Effect::Reverb(delay, feedback)
                    }
                    Effect::Echo(_, _) => {
                        let delay = bits_to_duration(
                            &bits[codon_size..codon_size + BITS_PER_PARAMETER]
                        );
                        let feedback = bits_to_amplitude(
                            &bits[codon_size + BITS_PER_PARAMETER..total_size]
                        );
                        Effect::Echo(delay, feedback)
                    }
                };
                return Some((effect_instance, total_size));
            }
        }
    }
    None
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
