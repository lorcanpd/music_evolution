use rand::Rng;
use rodio::{OutputStream, Sink, Source};
use std::error::Error;
use std::f32::consts::PI;
use std::time::Duration;

const PARAMETERS: usize = 4; // Frequency, Amplitude, Duration, Phase
const BITS_PER_PARAMETER: usize = 8;
const TOTAL_SONG_LENGTH_BITS: usize = 16; // 16 bits for total song length
const MUTATION_RATE_BITS: usize = 8; // 8 bits for mutation rate
const NOTE_START_BITS: usize = 8; // 8 bits for each note start time
// 24 bits base size
const GENOME_BASE_SIZE: usize = MUTATION_RATE_BITS + TOTAL_SONG_LENGTH_BITS; 
const GENOME_SIZE: usize = 1024 * 4; // 4096 bits

const NOTE_CODON_SIZE: usize = 7; // Size of note codon in bits
const TRACK_CODON_SIZE: usize = 8; // Size of track codon in bits

// Example pattern for note codon
const NOTE_CODON: [u8; NOTE_CODON_SIZE] = [0, 1, 0, 1, 1, 1, 0]; 
// Example pattern for track codon
const TRACK_CODON: [u8; TRACK_CODON_SIZE] = [1, 0, 1, 0, 1, 1, 0, 1]; 
struct GenomePlayer {
    genome: Vec<u8>,
}

impl GenomePlayer {
    pub fn new(genome: Vec<u8>) -> Self {
        GenomePlayer { genome }
    }

    pub fn play(&self) -> Result<(), Box<dyn Error>> {
        let (_stream, stream_handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&stream_handle)?;

        // Extract total song length from genome
        let total_length = Self::bits_to_duration(
            &self.genome[
                MUTATION_RATE_BITS..MUTATION_RATE_BITS + TOTAL_SONG_LENGTH_BITS
            ]
        );
        println!("Total song length: {}ms", total_length.as_millis());

        // Extract track and note codons and parameters
        let mut tracks = Vec::new();
        let mut current_track = Vec::new();
        let mut inside_track = false;
        let mut i = GENOME_BASE_SIZE;

        while i < self.genome.len() {
            if i + TRACK_CODON_SIZE <= self.genome.len() && &self.genome[
                i..i + TRACK_CODON_SIZE
            ] == TRACK_CODON {
                if inside_track {
                    tracks.push(current_track);
                    current_track = Vec::new();
                }
                inside_track = true;
                // Currently we skip track codon. but 
                //  ideally I want overlap between notes etc.
                i += TRACK_CODON_SIZE; 
            } else if i + NOTE_CODON_SIZE <= self.genome.len() && &self.genome[
                i..i + NOTE_CODON_SIZE
            ] == NOTE_CODON {
                i += NOTE_CODON_SIZE;
                if i + NOTE_START_BITS + PARAMETERS * BITS_PER_PARAMETER <= self.genome.len() {
                    let start_time = Self::bits_to_duration(
                        &self.genome[i..i + NOTE_START_BITS]
                    );
                    let note = (
                        Self::bits_to_frequency(
                            &self.genome[
                                    i + NOTE_START_BITS..i + NOTE_START_BITS + 8
                                ]
                            ),
                        Self::bits_to_amplitude(
                            &self.genome[
                                i + NOTE_START_BITS + 8..i + NOTE_START_BITS + 16
                            ]
                        ),
                        Self::bits_to_duration(
                            &self.genome[
                                i + NOTE_START_BITS + 16..i + NOTE_START_BITS + 24
                            ]
                        ),
                        Self::bits_to_phase(
                            &self.genome[
                                i + NOTE_START_BITS + 24..i + NOTE_START_BITS + 32
                            ]
                        ),
                    );
                    current_track.push((start_time, note));
                    // Currently we skip note codon. but ideally I want 
                    // overlap between notes etc.
                    i += NOTE_START_BITS + PARAMETERS * BITS_PER_PARAMETER;
                }
            } else {
                i += 1;
            }
        }

        if inside_track {
            tracks.push(current_track);
        }

        let sample_rate = 44100;
        let sample_count = (total_length.as_secs_f32() * sample_rate as f32) as usize;
        let mut combined_samples = vec![0.0; sample_count];

        for (track_index, track) in tracks.iter().enumerate() {
            println!("Processing track {}", track_index + 1);
            for (start_time, (frequency, amplitude, duration, phase)) in track {
                let start_sample = (
                    start_time.as_secs_f32() * sample_rate as f32
                ) as usize;
                let end_sample = (
                    start_sample + (
                        duration.as_secs_f32() * sample_rate as f32
                    ) as usize
                ).min(sample_count);

                for sample_index in start_sample..end_sample {
                    let time = (
                        sample_index - start_sample
                    ) as f32 / sample_rate as f32;
                    let mut sample_value = 0.0;

                    for j in 0..10 { // Number of harmonics
                        let harmonic_frequency = frequency * (j + 1) as f32;
                        let harmonic_amplitude = amplitude / (j + 1) as f32;
                        sample_value += harmonic_amplitude * (2
                            .0 * PI * harmonic_frequency * time + phase
                        ).sin();
                    }

                    combined_samples[sample_index] += sample_value;
                }
            }
        }

        sink.append(rodio::buffer::SamplesBuffer::new(1, sample_rate, combined_samples));
        sink.sleep_until_end();
        Ok(())
    }

    fn bits_to_frequency(bits: &[u8]) -> f32 {
        let value = Self::bits_to_value(bits);
        220.0 + value as f32 * 4.0 // Frequency range from 220Hz to ~1250Hz
    }

    fn bits_to_amplitude(bits: &[u8]) -> f32 {
        let value = Self::bits_to_value(bits);
        // Normalized amplitude between 0.0 and 1.0
        value as f32 / 128.0 
    }

    fn bits_to_duration(bits: &[u8]) -> Duration {
        let value = Self::bits_to_value(bits);
        // Duration range from 100ms to ~1350ms
        Duration::from_millis(100 + value as u64 * 50) 
    }

    fn bits_to_phase(bits: &[u8]) -> f32 {
        let value = Self::bits_to_value(bits);
        // Phase between 0 and 2π
        value as f32 * 2.0 * PI / 255.0 
    }

    fn bits_to_value(bits: &[u8]) -> u8 {
        bits.iter().rev().enumerate().fold(0, |acc, (i, &bit)| {
            if i < 8 { // Ensure we do not exceed the u8 size
                acc + (bit << i)
            } else {
                acc
            }
        })
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // Create a random genome
    let mut rng = rand::thread_rng();
    let genome: Vec<u8> = (
        0..GENOME_SIZE
    ).map(|_| rng.gen_range(0..=1)).collect();
    println!("Genome: {:?}", genome);

    let player = GenomePlayer::new(genome);
    player.play()?;

    Ok(())
}
