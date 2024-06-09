mod genome;
mod decode_genome;
mod play_genes;
mod genome_crosser;

use genome::Genome;
use decode_genome::DecodedGenome;
use play_genes::play_genes;
use genome_crosser::GenomeCrosser;
use rand::Rng;

fn main() {
    // Generate random parent genomes. Randomly assign 0s and 1s to the genome.
    let father_genome = Genome::initialise_random_genome();
    let decoded_father_genome = DecodedGenome::decode(father_genome.get_left_chromosome(), father_genome.get_right_chromosome());
    let mother_genome = Genome::initialise_random_genome();
    let decoded_mother_genome = DecodedGenome::decode(mother_genome.get_left_chromosome(), mother_genome.get_right_chromosome());

    // Crossover to produce child genome
    let (child_left, child_right) = GenomeCrosser::crossover(
        father_genome.get_left_chromosome(),
        mother_genome.get_left_chromosome(),
        father_genome.get_right_chromosome(),
        mother_genome.get_right_chromosome(),
    );

    // Create Genome struct
    let child_genome = Genome::new(child_left, child_right);

    // Decode the genome
    let decoded_genome = DecodedGenome::decode(child_genome.get_left_chromosome(), child_genome.get_right_chromosome());

    println!("Father phenotype:");
    play_genes(&decoded_father_genome).unwrap();
    
    println!("Mother phenotype:");
    play_genes(&decoded_mother_genome).unwrap();
    // Play the decoded child genome
    println!("Child phenotype:");
    play_genes(&decoded_genome).unwrap();
}
