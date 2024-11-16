use rand::Rng;

pub const PARAMETERS: usize = 5; // Frequency, Amplitude, Duration, Phase
pub const BITS_PER_PARAMETER: usize = 8;

pub struct Chromosome {
    left_chromosome: Vec<u8>,
    right_chromosome: Vec<u8>,
}

impl Chromosome {
    pub fn new(left_chromosome: Vec<u8>, right_chromosome: Vec<u8>) -> Self {
        Chromosome {
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

    pub fn initialise_random_chromosome(min_start_len: i32, max_start_len: i32) -> Self {
        let mut rng = rand::thread_rng();
        let chr_length: i32 = rng.gen_range(min_start_len..=max_start_len);
        let left_chromosome = (0..chr_length).map(
            |_| rng.gen_range(0..=1)
        ).collect::<Vec<u8>>();
        // right should be a copy of the left
        let right_chromosome = left_chromosome.clone();
        Chromosome {
            left_chromosome,
            right_chromosome,
        }
    }
}

pub struct Genome {
    pub notes: Chromosome,
    pub effects: Chromosome,
    pub sine_codon: Chromosome,
    pub square_codon: Chromosome,
    pub custom_codon: Chromosome,
    pub low_pass_codon: Chromosome,
    pub high_pass_codon: Chromosome,
    pub reverb_codon: Chromosome,
    pub echo_codon: Chromosome,
    pub mutation_rate: Chromosome,
}

impl Genome {
    pub fn new(
        notes_left: Vec<u8>, notes_right: Vec<u8>,
        effects_left: Vec<u8>, effects_right: Vec<u8>,
        sine_codon_left: Vec<u8>, sine_codon_right: Vec<u8>,
        square_codon_left: Vec<u8>, square_codon_right: Vec<u8>,
        custom_codon_left: Vec<u8>, custom_codon_right: Vec<u8>,
        low_pass_codon_left: Vec<u8>, low_pass_codon_right: Vec<u8>,
        high_pass_codon_left: Vec<u8>, high_pass_codon_right: Vec<u8>,
        reverb_codon_left: Vec<u8>, reverb_codon_right: Vec<u8>,
        echo_codon_left: Vec<u8>, echo_codon_right: Vec<u8>,
        mutation_rate_left: Vec<u8>, mutation_rate_right: Vec<u8>
    ) -> Self {
        Genome {
            notes: Chromosome::new(notes_left, notes_right),
            effects: Chromosome::new(effects_left, effects_right),
            sine_codon: Chromosome::new(sine_codon_left, sine_codon_right),
            square_codon: Chromosome::new(square_codon_left, square_codon_right),
            custom_codon: Chromosome::new(custom_codon_left, custom_codon_right),
            low_pass_codon: Chromosome::new(low_pass_codon_left, low_pass_codon_right),
            high_pass_codon: Chromosome::new(high_pass_codon_left, high_pass_codon_right),
            reverb_codon: Chromosome::new(reverb_codon_left, reverb_codon_right),
            echo_codon: Chromosome::new(echo_codon_left, echo_codon_right),
            mutation_rate: Chromosome::new(mutation_rate_left, mutation_rate_right),
        }
    }

    pub fn initialise_random_genome(
        large_chr_min: i32, large_chr_max: i32, small_chr_min: i32, small_chr_max: i32
    ) -> Self {
        Genome {
            notes: Chromosome::initialise_random_chromosome(large_chr_min, large_chr_max),
            effects: Chromosome::initialise_random_chromosome(large_chr_min, large_chr_max),
            sine_codon: Chromosome::initialise_random_chromosome(small_chr_min, small_chr_max),
            square_codon: Chromosome::initialise_random_chromosome(small_chr_min, small_chr_max),
            custom_codon: Chromosome::initialise_random_chromosome(small_chr_min, small_chr_max),
            low_pass_codon: Chromosome::initialise_random_chromosome(small_chr_min, small_chr_max),
            high_pass_codon: Chromosome::initialise_random_chromosome(small_chr_min, small_chr_max),
            reverb_codon: Chromosome::initialise_random_chromosome(small_chr_min, small_chr_max),
            echo_codon: Chromosome::initialise_random_chromosome(small_chr_min, small_chr_max),
            mutation_rate: Chromosome::initialise_random_chromosome(8, 8),
        }
    }
}