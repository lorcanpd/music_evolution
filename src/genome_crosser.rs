use rand::Rng;
use crate::genome::{Genome, Chromosome};

pub struct GenomeCrosser;

impl GenomeCrosser {
    pub fn crossover(father: &Genome, mother: &Genome) -> Genome {
        let mut rng = rand::thread_rng();

        let mutation_rate_father = Self::decode_mutation_rate(
            &father.mutation_rate.get_left_chromosome()
        );
        let mutation_rate_mother = Self::decode_mutation_rate(
            &mother.mutation_rate.get_left_chromosome()
        );

        let crossed_notes = Self::cross_chromosomes(
            &father.notes, &mother.notes, mutation_rate_father, mutation_rate_mother
        );
        let crossed_effects = Self::cross_chromosomes(
            &father.effects, &mother.effects, mutation_rate_father, mutation_rate_mother
        );
        let crossed_sine_codon = Self::cross_chromosomes(
            &father.sine_codon, &mother.sine_codon, mutation_rate_father, mutation_rate_mother
        );
        let crossed_square_codon = Self::cross_chromosomes(
            &father.square_codon, &mother.square_codon, mutation_rate_father, mutation_rate_mother
        );
        let crossed_custom_codon = Self::cross_chromosomes(
            &father.custom_codon, &mother.custom_codon, mutation_rate_father, mutation_rate_mother
        );
        let crossed_low_pass_codon = Self::cross_chromosomes(
            &father.low_pass_codon, &mother.low_pass_codon, mutation_rate_father,
            mutation_rate_mother
        );
        let crossed_high_pass_codon = Self::cross_chromosomes(
            &father.high_pass_codon, &mother.high_pass_codon, mutation_rate_father,
            mutation_rate_mother
        );
        let crossed_reverb_codon = Self::cross_chromosomes(
            &father.reverb_codon, &mother.reverb_codon, mutation_rate_father, mutation_rate_mother
        );
        let crossed_echo_codon = Self::cross_chromosomes(
            &father.echo_codon, &mother.echo_codon, mutation_rate_father, mutation_rate_mother
        );
        let crossed_mutation_rate = Self::cross_chromosomes(
            &father.mutation_rate, &mother.mutation_rate, mutation_rate_father,
            mutation_rate_mother
        );

        Genome {
            notes: crossed_notes,
            effects: crossed_effects,
            sine_codon: crossed_sine_codon,
            square_codon: crossed_square_codon,
            custom_codon: crossed_custom_codon,
            low_pass_codon: crossed_low_pass_codon,
            high_pass_codon: crossed_high_pass_codon,
            reverb_codon: crossed_reverb_codon,
            echo_codon: crossed_echo_codon,
            mutation_rate: crossed_mutation_rate,
        }
    }

    fn cross_chromosomes(
        father_chromosome: &Chromosome, mother_chromosome: &Chromosome, father_mutation_rate: f64,
        mother_mutation_rate: f64
    ) -> Chromosome {
        let mut rng = rand::thread_rng();

        // Cross over the left and right chromosomes of both parents
        let crossed_father = Self::cross_single_chromosome(
            &father_chromosome.get_left_chromosome(),
            &father_chromosome.get_right_chromosome(),
            father_mutation_rate
        );
        let crossed_mother = Self::cross_single_chromosome(
            &mother_chromosome.get_left_chromosome(),
            &mother_chromosome.get_right_chromosome(),
            mother_mutation_rate
        );

        // Randomly set the crossed-over chromosomes as left and right
        if rng.gen_bool(0.5) {
            Chromosome::new(crossed_father, crossed_mother)
        } else {
            Chromosome::new(crossed_mother, crossed_father)
        }
    }

    fn cross_single_chromosome(
        first: &[u8], second: &[u8], mutation_rate: f64
    ) -> Vec<u8> {
        let mut rng = rand::thread_rng();

        let first_len = first.len();
        let second_len = second.len();
        let mut child = Vec::with_capacity(first_len.max(second_len));

        let num_crossovers = rng.gen_range(1..=4);
        let mut crossover_points = Vec::new();
        for _ in 0..num_crossovers {
            crossover_points.push(rng.gen_range(0.0..1.0));
        }
        crossover_points.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mut current_pos_first = 0;
        let mut current_pos_second = 0;
        let mut in_first = rng.gen_bool(0.5);

        for &crossover_point in &crossover_points {
            let next_pos_first = (crossover_point * first_len as f64).round() as usize;
            let next_pos_second = (crossover_point * second_len as f64).round() as usize;

            if in_first {
                child.extend_from_slice(&first[current_pos_first..next_pos_first]);
            } else {
                child.extend_from_slice(&second[current_pos_second..next_pos_second]);
            }

            in_first = !in_first;
            current_pos_first = next_pos_first;
            current_pos_second = next_pos_second;
        }

        if in_first {
            child.extend_from_slice(&first[current_pos_first..]);
        } else {
            child.extend_from_slice(&second[current_pos_second..]);
        }

        Self::apply_mutation(&mut child, mutation_rate);

        child
    }

    fn decode_mutation_rate(bits: &[u8]) -> f64 {
        let value = bits.iter().rev().enumerate().fold(
            0, |acc, (i, &bit)| acc + (bit as usize * (1 << i))
        );
        value as f64 / (255.0 * 5.0)
    }

    fn apply_mutation(chromosome: &mut Vec<u8>, mutation_rate: f64) {
        let mut rng = rand::thread_rng();
        let substitution_rate = mutation_rate * 0.8;
        let indel_rate = mutation_rate * 0.1;
        for bit in chromosome.iter_mut() {
            if rng.gen_bool(substitution_rate) {
                *bit = 1 - *bit;
            }
        }

        if rng.gen_bool(indel_rate) {
            let pos = rng.gen_range(0..chromosome.len());
            chromosome.insert(pos, rng.gen());
        }

        if rng.gen_bool(indel_rate) {
            let pos = rng.gen_range(0..chromosome.len());
            chromosome.remove(pos);
        }
    }
}