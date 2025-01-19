use postgres::types::{FromSql, ToSql, Type, IsNull, accepts, to_sql_checked};
// use postgres::error::Error;
use std::fmt;
use std::fmt::{Debug, Formatter};
use postgres::types::private::BytesMut;
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
    pub song_id: Option<i32>,
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
            song_id: None,
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
            song_id: None,
        }
    }

    // method to allow the genome to be cloned
    pub fn clone_genome(&self) -> Genome {
        Genome {
            notes: Chromosome::new(
                self.notes.left_chromosome.clone(),
                self.notes.right_chromosome.clone()
            ),
            effects: Chromosome::new(
                self.effects.left_chromosome.clone(),
                self.effects.right_chromosome.clone()
            ),
            sine_codon: Chromosome::new(
                self.sine_codon.left_chromosome.clone(),
                self.sine_codon.right_chromosome.clone()
            ),
            square_codon: Chromosome::new(
                self.square_codon.left_chromosome.clone(),
                self.square_codon.right_chromosome.clone()
            ),
            custom_codon: Chromosome::new(
                self.custom_codon.left_chromosome.clone(),
                self.custom_codon.right_chromosome.clone()
            ),
            low_pass_codon: Chromosome::new(
                self.low_pass_codon.left_chromosome.clone(),
                self.low_pass_codon.right_chromosome.clone()
            ),
            high_pass_codon: Chromosome::new(
                self.high_pass_codon.left_chromosome.clone(),
                self.high_pass_codon.right_chromosome.clone()
            ),
            reverb_codon: Chromosome::new(
                self.reverb_codon.left_chromosome.clone(),
                self.reverb_codon.right_chromosome.clone()
            ),
            echo_codon: Chromosome::new(
                self.echo_codon.left_chromosome.clone(),
                self.echo_codon.right_chromosome.clone()
            ),
            mutation_rate: Chromosome::new(
                self.mutation_rate.left_chromosome.clone(),
                self.mutation_rate.right_chromosome.clone()
            ),
            song_id: self.song_id,
        }
    }

    // method for assigning a song_id to the genome
    // Question: Is using Some() the best way to assign a song_id to the genome?
    // Answer: Yes, using Some() is the best way to assign a song_id to the genome.
    pub fn assign_song_id(&mut self, song_id: i32) {
        self.song_id = Some(song_id);
    }
}

impl Debug for Genome {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

// function to parse the raw bytes of the genome os it can be efficiently stored in the database
impl ToSql for Genome {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        let mut bytes = Vec::new();

        // Helper function to extend bytes with chromosome data
        fn extend_with_chromosome(bytes: &mut Vec<u8>, chromosome: &Chromosome) {
            bytes.extend(&(chromosome.left_chromosome.len() as u32).to_be_bytes());
            bytes.extend(&chromosome.left_chromosome);
            bytes.extend(&(chromosome.right_chromosome.len() as u32).to_be_bytes());
            bytes.extend(&chromosome.right_chromosome);
        }

        extend_with_chromosome(&mut bytes, &self.notes);
        extend_with_chromosome(&mut bytes, &self.effects);
        extend_with_chromosome(&mut bytes, &self.sine_codon);
        extend_with_chromosome(&mut bytes, &self.square_codon);
        extend_with_chromosome(&mut bytes, &self.custom_codon);
        extend_with_chromosome(&mut bytes, &self.low_pass_codon);
        extend_with_chromosome(&mut bytes, &self.high_pass_codon);
        extend_with_chromosome(&mut bytes, &self.reverb_codon);
        extend_with_chromosome(&mut bytes, &self.echo_codon);
        extend_with_chromosome(&mut bytes, &self.mutation_rate);

        out.extend(bytes);
        Ok(IsNull::No)
    }

    accepts!(BYTEA);
    to_sql_checked!();
}

// function to restore the genome from the raw bytes stored in the database
impl FromSql<'_> for Genome {
    fn from_sql(ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let mut offset = 0;

        // Helper function to extract chromosome data
        fn extract_chromosome(raw: &[u8], offset: &mut usize) -> Chromosome {
            let left_len = u32::from_be_bytes(raw[*offset..*offset + 4].try_into().unwrap()) as usize;
            *offset += 4;
            let left_chromosome = raw[*offset..*offset + left_len].to_vec();
            *offset += left_len;

            let right_len = u32::from_be_bytes(raw[*offset..*offset + 4].try_into().unwrap()) as usize;
            *offset += 4;
            let right_chromosome = raw[*offset..*offset + right_len].to_vec();
            *offset += right_len;

            Chromosome::new(left_chromosome, right_chromosome)
        }

        let notes = extract_chromosome(raw, &mut offset);
        let effects = extract_chromosome(raw, &mut offset);
        let sine_codon = extract_chromosome(raw, &mut offset);
        let square_codon = extract_chromosome(raw, &mut offset);
        let custom_codon = extract_chromosome(raw, &mut offset);
        let low_pass_codon = extract_chromosome(raw, &mut offset);
        let high_pass_codon = extract_chromosome(raw, &mut offset);
        let reverb_codon = extract_chromosome(raw, &mut offset);
        let echo_codon = extract_chromosome(raw, &mut offset);
        let mutation_rate = extract_chromosome(raw, &mut offset);

        Ok(Genome {
            notes,
            effects,
            sine_codon,
            square_codon,
            custom_codon,
            low_pass_codon,
            high_pass_codon,
            reverb_codon,
            echo_codon,
            mutation_rate,
            song_id: None, // song_id is not stored in the raw bytes
        })
    }

    accepts!(BYTEA);
}
