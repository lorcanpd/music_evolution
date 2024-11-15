use rand::Rng;

pub const LEFT_CHROMOSOME_SIZE: usize = 512;
pub const RIGHT_CHROMOSOME_SIZE: usize = 512;
pub const PARAMETERS: usize = 5; // Frequency, Amplitude, Duration, Phase
pub const BITS_PER_PARAMETER: usize = 8;
pub const TOTAL_SONG_LENGTH_BITS: usize = 16; // 16 bits for total song length
pub const MUTATION_RATE_BITS: usize = 8; // 8 bits for mutation rate
pub const NOTE_START_BITS: usize = 8; // 8 bits for each note start time
pub const GENOME_SIZE: usize = LEFT_CHROMOSOME_SIZE + RIGHT_CHROMOSOME_SIZE;
pub const NOTE_CODON_SIZE: usize = 8; // Size of note codon in bits
pub const EFFECT_CODON_SIZE: usize = 9; // Size of effect codon in bits

pub const NOTE_CODON: [u8; NOTE_CODON_SIZE] = [1, 1, 0, 1, 1, 1, 0, 0]; // Example pattern for note codon
pub const SQUARE_WAVE_CODON: [u8; NOTE_CODON_SIZE] = [0, 1, 0, 1, 1, 1, 1, 1]; // Example pattern for square wave codon
pub const CUSTOM_WAVE_CODON: [u8; NOTE_CODON_SIZE] = [1, 0, 1, 0, 0, 1, 1, 0]; // Example pattern for custom waveform codon
pub const LOW_PASS_CODON: [u8; EFFECT_CODON_SIZE] = [1, 0, 1, 0, 1, 0, 1, 1, 0];
pub const HIGH_PASS_CODON: [u8; EFFECT_CODON_SIZE] = [0, 1, 0, 1, 0, 1, 0, 0, 0];
pub const REVERB_CODON: [u8; EFFECT_CODON_SIZE] = [1, 1, 1, 0, 1, 1, 1, 1, 0];
pub const ECHO_CODON: [u8; EFFECT_CODON_SIZE] = [0, 0, 0, 1, 0, 0, 0, 0, 0];

pub struct Genome {
    left_chromosome: Vec<u8>,
    right_chromosome: Vec<u8>,
}

impl Chromosome {
    pub fn new(left_chromosome: Vec<u8>, right_chromosome: Vec<u8>) -> Self {
        Genome {
            left_chromosome,
            right_chromosome,
        }
    }

    pub fn get_left_chromosome(&self) -> &[u8] {
        &self.left_chromosome
    }

    pub fn get_right_chromosome(&self) -> &[u8] {
        &self.right_chromosome
    }

    pub fn initialise_random_genome() -> Self {
        let mut rng = rand::thread_rng();
        let left_chromosome = (
            0..LEFT_CHROMOSOME_SIZE).map(|_| rng.gen_range(0..=1)).collect();
        let right_chromosome = (
            0..RIGHT_CHROMOSOME_SIZE).map(|_| rng.gen_range(0..=1)).collect();
        Genome {
            left_chromosome,
            right_chromosome,
        }
    }
}
