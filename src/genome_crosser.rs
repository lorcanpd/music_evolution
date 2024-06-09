use rand::Rng;

const LEFT_CHROMOSOME_SIZE: usize = 512;
const RIGHT_CHROMOSOME_SIZE: usize = 512;
const MUTATION_RATE_BITS: usize = 8;

pub struct GenomeCrosser;

impl GenomeCrosser {

    pub fn crossover(father_left: &[u8], mother_left: &[u8], father_right: &[u8], mother_right: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut rng = rand::thread_rng();

        let crossed_father_left = Self::cross_chromosomes(father_left, father_right);
        let crossed_mother_left = Self::cross_chromosomes(mother_left, mother_right);

        let final_left = if rng.gen_bool(0.5) { crossed_father_left.clone() } else { crossed_mother_left.clone() };
        let final_right = if rng.gen_bool(0.5) { crossed_mother_left } else { crossed_father_left };


        (final_left, final_right)
    }


    fn cross_chromosomes(first_chromosome: &[u8], second_chromosome: &[u8]) -> Vec<u8> {
        assert_eq!(first_chromosome.len(), second_chromosome.len());

        let genome_length = first_chromosome.len();
        let mut rng = rand::thread_rng();
        let mut child_chromosome = Vec::with_capacity(genome_length);

        let num_crossovers = rng.gen_range(1..=4); // Randomly choose 1 to 4 crossover points

        let mut crossover_points = Vec::new();
        for _ in 0..num_crossovers {
            crossover_points.push(rng.gen_range(0..genome_length));
        }
        crossover_points.sort_unstable();

        let mut current_pos = 0;
        let mut in_first = rng.gen_bool(0.5); // Randomly decide whether to start with first or second chromosome

        for &crossover_point in &crossover_points {
            if in_first {
                child_chromosome.extend_from_slice(&first_chromosome[current_pos..crossover_point]);
            } else {
                child_chromosome.extend_from_slice(&second_chromosome[current_pos..crossover_point]);
            }
            in_first = !in_first;
            current_pos = crossover_point;
        }

        if in_first {
            child_chromosome.extend_from_slice(&first_chromosome[current_pos..]);
        } else {
            child_chromosome.extend_from_slice(&second_chromosome[current_pos..]);
        }

        let mutation_rate = Self::decode_mutation_rate(&first_chromosome[..MUTATION_RATE_BITS]);

        Self::apply_mutation(&child_chromosome, mutation_rate);

        child_chromosome
    }

    fn decode_mutation_rate(bits: &[u8]) -> f64 {
        let value = bits.iter().rev().enumerate().fold(0, |acc, (i, &bit)| acc + (bit as usize * (1 << i)));
        value as f64 / (255.0 * 5.0) // Mutation rate between 0 and 0.2
    }

    fn apply_mutation(genome: &[u8], mutation_rate: f64) -> Vec<u8> {
        let mut rng = rand::thread_rng();
        genome.iter().map(|&bit| {
            if rng.gen_bool(mutation_rate) {
                1 - bit // Flip the bit
            } else {
                bit
            }
        }).collect()
    }
}
