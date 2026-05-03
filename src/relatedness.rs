use crate::genome::{Chromosome, Genome};

fn needleman_wunsch_identity(a: &[u8], b: &[u8]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }

    let rows = a.len() + 1;
    let cols = b.len() + 1;
    let mut scores = vec![vec![0i32; cols]; rows];
    let mut trace = vec![vec![0u8; cols]; rows];

    for i in 1..rows {
        scores[i][0] = scores[i - 1][0] - 1;
        trace[i][0] = 1;
    }
    for j in 1..cols {
        scores[0][j] = scores[0][j - 1] - 1;
        trace[0][j] = 2;
    }

    for i in 1..rows {
        for j in 1..cols {
            let diag_score = scores[i - 1][j - 1] + if a[i - 1] == b[j - 1] { 1 } else { -1 };
            let up_score = scores[i - 1][j] - 1;
            let left_score = scores[i][j - 1] - 1;

            let (score, dir) = if diag_score >= up_score && diag_score >= left_score {
                (diag_score, 0)
            } else if up_score >= left_score {
                (up_score, 1)
            } else {
                (left_score, 2)
            };

            scores[i][j] = score;
            trace[i][j] = dir;
        }
    }

    let mut i = a.len();
    let mut j = b.len();
    let mut aligned = 0usize;
    let mut matches = 0usize;

    while i > 0 || j > 0 {
        match trace[i][j] {
            0 => {
                aligned += 1;
                if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
                    matches += 1;
                }
                i = i.saturating_sub(1);
                j = j.saturating_sub(1);
            }
            1 => {
                aligned += 1;
                i = i.saturating_sub(1);
            }
            2 => {
                aligned += 1;
                j = j.saturating_sub(1);
            }
            _ => break,
        }
    }

    if aligned == 0 {
        0.0
    } else {
        matches as f32 / aligned as f32
    }
}

fn chromosome_similarity(chromosome_a: &Chromosome, chromosome_b: &Chromosome) -> (f32, usize) {
    let a_left = chromosome_a.get_left_chromosome();
    let a_right = chromosome_a.get_right_chromosome();
    let b_left = chromosome_b.get_left_chromosome();
    let b_right = chromosome_b.get_right_chromosome();

    let straight = (
        needleman_wunsch_identity(a_left, b_left),
        needleman_wunsch_identity(a_right, b_right),
    );
    let crossed = (
        needleman_wunsch_identity(a_left, b_right),
        needleman_wunsch_identity(a_right, b_left),
    );

    let straight_avg = (straight.0 + straight.1) / 2.0;
    let crossed_avg = (crossed.0 + crossed.1) / 2.0;
    let weight = a_left.len() + a_right.len() + b_left.len() + b_right.len();

    (straight_avg.max(crossed_avg), weight.max(1))
}

pub fn genome_similarity(a: &Genome, b: &Genome) -> f32 {
    let chromosomes = [
        (&a.notes, &b.notes),
        (&a.effects, &b.effects),
        (&a.sine_codon, &b.sine_codon),
        (&a.square_codon, &b.square_codon),
        (&a.custom_codon, &b.custom_codon),
        (&a.low_pass_codon, &b.low_pass_codon),
        (&a.high_pass_codon, &b.high_pass_codon),
        (&a.reverb_codon, &b.reverb_codon),
        (&a.echo_codon, &b.echo_codon),
        (&a.mutation_rate, &b.mutation_rate),
    ];

    let mut weighted_sum = 0.0f32;
    let mut total_weight = 0usize;

    for (left, right) in chromosomes {
        let (similarity, weight) = chromosome_similarity(left, right);
        weighted_sum += similarity * weight as f32;
        total_weight += weight;
    }

    if total_weight == 0 {
        0.0
    } else {
        weighted_sum / total_weight as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn genome_with_bits(
        notes_left: &[u8],
        notes_right: &[u8],
        effects_left: &[u8],
        effects_right: &[u8],
    ) -> Genome {
        Genome::new(
            notes_left.to_vec(),
            notes_right.to_vec(),
            effects_left.to_vec(),
            effects_right.to_vec(),
            vec![1, 0, 1, 0],
            vec![1, 0, 1, 0],
            vec![0, 1, 0, 1],
            vec![0, 1, 0, 1],
            vec![1, 1, 0, 0],
            vec![1, 1, 0, 0],
            vec![0, 0, 1, 1],
            vec![0, 0, 1, 1],
            vec![1, 0, 0, 1],
            vec![1, 0, 0, 1],
            vec![0, 1, 1, 0],
            vec![0, 1, 1, 0],
            vec![1, 1, 1, 0],
            vec![1, 1, 1, 0],
            vec![0, 0, 0, 1],
            vec![0, 0, 0, 1],
        )
    }

    #[test]
    fn identical_genomes_have_full_similarity() {
        let genome = genome_with_bits(&[1, 0, 1, 0], &[0, 1, 0, 1], &[1, 1, 0, 0], &[0, 0, 1, 1]);
        let similarity = genome_similarity(&genome, &genome);
        assert!((similarity - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn swapped_haplotypes_score_as_identical() {
        let genome_a = genome_with_bits(&[1, 0, 1, 0], &[0, 1, 0, 1], &[1, 1, 0, 0], &[0, 0, 1, 1]);
        let genome_b = genome_with_bits(&[0, 1, 0, 1], &[1, 0, 1, 0], &[0, 0, 1, 1], &[1, 1, 0, 0]);

        let similarity = genome_similarity(&genome_a, &genome_b);
        assert!((similarity - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn divergent_genomes_score_lower_than_close_relatives() {
        let close_a = genome_with_bits(&[1, 1, 1, 1], &[1, 1, 1, 0], &[0, 0, 0, 0], &[0, 0, 0, 1]);
        let close_b = genome_with_bits(&[1, 1, 1, 1], &[1, 1, 0, 0], &[0, 0, 0, 0], &[0, 0, 1, 1]);
        let distant = genome_with_bits(&[0, 0, 0, 0], &[0, 0, 0, 0], &[1, 1, 1, 1], &[1, 1, 1, 1]);

        let close_similarity = genome_similarity(&close_a, &close_b);
        let distant_similarity = genome_similarity(&close_a, &distant);

        assert!(close_similarity > distant_similarity);
        assert!(distant_similarity < 1.0);
    }
}
