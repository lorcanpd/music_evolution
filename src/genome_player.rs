// use rodio::{OutputStream, Sink};
// use std::error::Error;
// use std::f32::consts::PI;
// use std::time::Duration;
// use hound; // Added for WAV file writing
//
// const PARAMETERS: usize = 4; // Frequency, Amplitude, Duration, Phase
// const BITS_PER_PARAMETER: usize = 8;
// const TOTAL_SONG_LENGTH_BITS: usize = 16; // 16 bits for total song length
// const MUTATION_RATE_BITS: usize = 8; // 8 bits for mutation rate
// const NOTE_START_BITS: usize = 8; // 8 bits for each note start time
// const GENOME_BASE_SIZE: usize = MUTATION_RATE_BITS + TOTAL_SONG_LENGTH_BITS; // 24 bits base size
// // const GENOME_SIZE: usize = 1024 * 1;
//
// // const NOTE_CODON_SIZE: usize = 7; // Size of note codon in bits
// // const EFFECT_CODON_SIZE: usize = 9; // Size of effect codon in bits
//
// // const NOTE_CODON: [u8; NOTE_CODON_SIZE] = [1, 1, 0, 1, 1, 1, 0,0,0,0]; // Example pattern for note codon
// // const SQUARE_WAVE_CODON: [u8; NOTE_CODON_SIZE] = [0, 1, 0, 1, 1, 1, 1]; // Example pattern for square wave codon
// // const CUSTOM_WAVE_CODON: [u8; NOTE_CODON_SIZE] = [1, 0, 1, 0, 0, 1, 1]; // Example pattern for custom waveform codon
// // const LOW_PASS_CODON: [u8; EFFECT_CODON_SIZE] = [1, 0, 1, 0, 1, 0, 1];
// // const HIGH_PASS_CODON: [u8; EFFECT_CODON_SIZE] = [0, 1, 0, 1, 0, 1, 0];
// // const REVERB_CODON: [u8; EFFECT_CODON_SIZE] = [1, 1, 1, 0, 1, 1, 1];
// // const ECHO_CODON: [u8; EFFECT_CODON_SIZE] = [0, 0, 0, 1, 0, 0, 0];
//
// pub struct GenomePlayer {
//     genome: Vec<u8>,
// }
//
// impl GenomePlayer {
//     pub fn new(genome: Vec<u8>) -> Self {
//         GenomePlayer { genome }
//     }
//
//     /// Plays the genome using `rodio` for debugging purposes.
//     pub fn play(&self) -> Result<(), Box<dyn Error>> {
//         let (_stream, stream_handle) = OutputStream::try_default()?;
//         let sink = Sink::try_new(&stream_handle)?;
//
//         let combined_samples = self.generate_samples();
//
//         let sample_rate = 44100;
//         sink.append(rodio::buffer::SamplesBuffer::new(1, sample_rate, combined_samples));
//         sink.sleep_until_end();
//         Ok(())
//     }
//
//     /// Generates a WAV file from the genome and saves it to the specified filename.
//     pub fn generate_wav(&self, filename: &str) -> Result<(), Box<dyn Error>> {
//         let combined_samples = self.generate_samples();
//
//         let sample_rate = 44100;
//         let spec = hound::WavSpec {
//             channels: 1,
//             sample_rate: sample_rate as u32,
//             bits_per_sample: 16,
//             sample_format: hound::SampleFormat::Int,
//         };
//
//         let mut writer = hound::WavWriter::create(filename, spec)?;
//
//         for sample in combined_samples {
//             // Convert f32 samples in [-1.0, 1.0] to i16
//             let amplitude = (sample * i16::MAX as f32) as i16;
//             writer.write_sample(amplitude)?;
//         }
//
//         writer.finalize()?;
//         Ok(())
//     }
//
//     /// Generates WAV data from the genome and returns it as a `Vec<u8>`.
//     /// Useful for streaming the audio data.
//     pub fn generate_wav_data(&self) -> Result<Vec<u8>, Box<dyn Error>> {
//         let combined_samples = self.generate_samples();
//
//         let sample_rate = 44100;
//         let spec = hound::WavSpec {
//             channels: 1,
//             sample_rate: sample_rate as u32,
//             bits_per_sample: 16,
//             sample_format: hound::SampleFormat::Int,
//         };
//
//         let mut cursor = std::io::Cursor::new(Vec::new());
//         let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
//
//         for sample in combined_samples {
//             // Convert f32 samples in [-1.0, 1.0] to i16
//             let amplitude = (sample * i16::MAX as f32) as i16;
//             writer.write_sample(amplitude)?;
//         }
//
//         writer.finalize()?;
//
//         Ok(cursor.into_inner())
//     }
//
//     /// Generates the audio samples from the genome.
//     fn generate_samples(&self) -> Vec<f32> {
//         let total_length = Self::bits_to_duration(&self.genome[MUTATION_RATE_BITS..MUTATION_RATE_BITS + TOTAL_SONG_LENGTH_BITS]);
//         println!("Total song length: {}ms", total_length.as_millis());
//
//         let mut i = GENOME_BASE_SIZE;
//         let sample_rate = 44100;
//         let mut combined_samples = vec![0.0; (total_length.as_secs_f32() * sample_rate as f32) as usize];
//
//         while i < self.genome.len() {
//             if i + NOTE_CODON_SIZE <= self.genome.len() && Self::matches_codon(&self.genome[i..i + NOTE_CODON_SIZE], &NOTE_CODON) {
//                 i += NOTE_CODON_SIZE;
//                 if i + NOTE_START_BITS + PARAMETERS * BITS_PER_PARAMETER <= self.genome.len() {
//                     let start_time = Self::bits_to_duration(&self.genome[i..i + NOTE_START_BITS]);
//                     let (frequency, amplitude, duration, phase) = Self::decode_parameters(&self.genome[i + NOTE_START_BITS..i + NOTE_START_BITS + PARAMETERS * BITS_PER_PARAMETER]);
//                     Self::generate_sine_wave(&mut combined_samples, start_time, frequency, amplitude, duration, phase);
//                     i += NOTE_START_BITS + PARAMETERS * BITS_PER_PARAMETER;
//                 }
//             } else if i + NOTE_CODON_SIZE <= self.genome.len() && Self::matches_codon(&self.genome[i..i + NOTE_CODON_SIZE], &SQUARE_WAVE_CODON) {
//                 i += NOTE_CODON_SIZE;
//                 if i + NOTE_START_BITS + PARAMETERS * BITS_PER_PARAMETER <= self.genome.len() {
//                     let start_time = Self::bits_to_duration(&self.genome[i..i + NOTE_START_BITS]);
//                     let (frequency, amplitude, duration, phase) = Self::decode_parameters(&self.genome[i + NOTE_START_BITS..i + NOTE_START_BITS + PARAMETERS * BITS_PER_PARAMETER]);
//                     Self::generate_square_wave(&mut combined_samples, start_time, frequency, amplitude, duration, phase);
//                     i += NOTE_START_BITS + PARAMETERS * BITS_PER_PARAMETER;
//                 }
//             } else if i + NOTE_CODON_SIZE <= self.genome.len() && Self::matches_codon(&self.genome[i..i + NOTE_CODON_SIZE], &CUSTOM_WAVE_CODON) {
//                 i += NOTE_CODON_SIZE;
//                 if i + NOTE_START_BITS + PARAMETERS * BITS_PER_PARAMETER <= self.genome.len() {
//                     let start_time = Self::bits_to_duration(&self.genome[i..i + NOTE_START_BITS]);
//                     let (frequency, amplitude, duration, phase) = Self::decode_parameters(&self.genome[i + NOTE_START_BITS..i + NOTE_START_BITS + PARAMETERS * BITS_PER_PARAMETER]);
//                     Self::generate_custom_waveform(&mut combined_samples, start_time, frequency, amplitude, duration, phase);
//                     i += NOTE_START_BITS + PARAMETERS * BITS_PER_PARAMETER;
//                 }
//             } else {
//                 i += 1;
//             }
//         }
//
//         // Parse effects in reverse
//         i = self.genome.len();
//         while i > 0 {
//             if i >= EFFECT_CODON_SIZE + BITS_PER_PARAMETER && Self::matches_codon(&self.genome[i - EFFECT_CODON_SIZE..i], &LOW_PASS_CODON) {
//                 i -= EFFECT_CODON_SIZE;
//                 let cutoff = Self::bits_to_amplitude(&self.genome[i - BITS_PER_PARAMETER..i]);
//                 Self::apply_low_pass_filter(&mut combined_samples, cutoff);
//                 i -= BITS_PER_PARAMETER;
//             } else if i >= EFFECT_CODON_SIZE + BITS_PER_PARAMETER && Self::matches_codon(&self.genome[i - EFFECT_CODON_SIZE..i], &HIGH_PASS_CODON) {
//                 i -= EFFECT_CODON_SIZE;
//                 let cutoff = Self::bits_to_amplitude(&self.genome[i - BITS_PER_PARAMETER..i]);
//                 Self::apply_high_pass_filter(&mut combined_samples, cutoff);
//                 i -= BITS_PER_PARAMETER;
//             } else if i >= EFFECT_CODON_SIZE + 2 * BITS_PER_PARAMETER && Self::matches_codon(&self.genome[i - EFFECT_CODON_SIZE..i], &REVERB_CODON) {
//                 i -= EFFECT_CODON_SIZE;
//                 let delay = Self::bits_to_duration(&self.genome[i - 2 * BITS_PER_PARAMETER..i - BITS_PER_PARAMETER]);
//                 let feedback = Self::bits_to_amplitude(&self.genome[i - BITS_PER_PARAMETER..i]);
//                 Self::apply_reverb(&mut combined_samples, delay, feedback);
//                 i -= 2 * BITS_PER_PARAMETER;
//             } else if i >= EFFECT_CODON_SIZE + 2 * BITS_PER_PARAMETER && Self::matches_codon(&self.genome[i - EFFECT_CODON_SIZE..i], &ECHO_CODON) {
//                 i -= EFFECT_CODON_SIZE;
//                 let delay = Self::bits_to_duration(&self.genome[i - 2 * BITS_PER_PARAMETER..i - BITS_PER_PARAMETER]);
//                 let feedback = Self::bits_to_amplitude(&self.genome[i - BITS_PER_PARAMETER..i]);
//                 Self::apply_echo(&mut combined_samples, delay, feedback);
//                 i -= 2 * BITS_PER_PARAMETER;
//             } else {
//                 i -= 1;
//             }
//         }
//
//         combined_samples
//     }
//
//     fn decode_parameters(bits: &[u8]) -> (f32, f32, Duration, f32) {
//         let frequency = Self::bits_to_frequency(&bits[0..8]);
//         let amplitude = Self::bits_to_amplitude(&bits[8..16]);
//         let duration = Self::bits_to_duration(&bits[16..24]);
//         let phase = Self::bits_to_phase(&bits[24..32]);
//         (frequency, amplitude, duration, phase)
//     }
//
//     fn generate_sine_wave(samples: &mut Vec<f32>, start_time: Duration, frequency: f32, amplitude: f32, duration: Duration, phase: f32) {
//         let sample_rate = 44100;
//         let start_sample = (start_time.as_secs_f32() * sample_rate as f32) as usize;
//         let end_sample = (start_sample + (duration.as_secs_f32() * sample_rate as f32) as usize).min(samples.len());
//
//         for sample_index in start_sample..end_sample {
//             let time = (sample_index - start_sample) as f32 / sample_rate as f32;
//             samples[sample_index] += amplitude * (2.0 * PI * frequency * time + phase).sin();
//         }
//     }
//
//     fn generate_square_wave(samples: &mut Vec<f32>, start_time: Duration, frequency: f32, amplitude: f32, duration: Duration, phase: f32) {
//         let sample_rate = 44100;
//         let start_sample = (start_time.as_secs_f32() * sample_rate as f32) as usize;
//         let end_sample = (start_sample + (duration.as_secs_f32() * sample_rate as f32) as usize).min(samples.len());
//
//         for sample_index in start_sample..end_sample {
//             let time = (sample_index - start_sample) as f32 / sample_rate as f32;
//             samples[sample_index] += amplitude * ((2.0 * PI * frequency * time + phase).sin().signum());
//         }
//     }
//
//     fn generate_custom_waveform(samples: &mut Vec<f32>, start_time: Duration, frequency: f32, amplitude: f32, duration: Duration, phase: f32) {
//         let sample_rate = 44100;
//         let start_sample = (start_time.as_secs_f32() * sample_rate as f32) as usize;
//         let end_sample = (start_sample + (duration.as_secs_f32() * sample_rate as f32) as usize).min(samples.len());
//
//         for sample_index in start_sample..end_sample {
//             let time = (sample_index - start_sample) as f32 / sample_rate as f32;
//             let mut sample_value = 0.0;
//             for j in 0..10 { // Number of harmonics
//                 let harmonic_frequency = frequency * (j + 1) as f32;
//                 let harmonic_amplitude = amplitude / (j + 1) as f32;
//                 sample_value += harmonic_amplitude * (2.0 * PI * harmonic_frequency * time + phase).sin();
//             }
//             samples[sample_index] += sample_value;
//         }
//     }
//
//     fn apply_low_pass_filter(samples: &mut Vec<f32>, cutoff: f32) {
//         let mut previous = 0.0;
//         for sample in samples.iter_mut() {
//             previous = previous + cutoff * (*sample - previous);
//             *sample = previous;
//         }
//     }
//
//     fn apply_high_pass_filter(samples: &mut Vec<f32>, cutoff: f32) {
//         let mut previous = 0.0;
//         for sample in samples.iter_mut() {
//             let current = *sample;
//             *sample = current - previous + cutoff * current;
//             previous = current;
//         }
//     }
//
//     fn apply_reverb(samples: &mut Vec<f32>, delay: Duration, feedback: f32) {
//         let delay_samples = (delay.as_secs_f32() * 44100.0) as usize;
//         let mut buffer = vec![0.0; delay_samples];
//         let mut index = 0;
//         for sample in samples.iter_mut() {
//             let delayed_sample = buffer[index];
//             let output = *sample + delayed_sample * feedback;
//             buffer[index] = output;
//             index = (index + 1) % delay_samples;
//             *sample = output;
//         }
//     }
//
//     fn apply_echo(samples: &mut Vec<f32>, delay: Duration, feedback: f32) {
//         let delay_samples = (delay.as_secs_f32() * 44100.0) as usize;
//         let mut buffer = vec![0.0; delay_samples];
//         let mut index = 0;
//         for sample in samples.iter_mut() {
//             let delayed_sample = buffer[index];
//             let output = *sample + delayed_sample * feedback;
//             buffer[index] = *sample;
//             index = (index + 1) % delay_samples;
//             *sample = output;
//         }
//     }
//
//     fn bits_to_frequency(bits: &[u8]) -> f32 {
//         let value = Self::bits_to_value(bits);
//         220.0 + value as f32 * 4.0 // Frequency range from 220Hz to ~1250Hz
//     }
//
//     fn bits_to_amplitude(bits: &[u8]) -> f32 {
//         let value = Self::bits_to_value(bits);
//         value as f32 / 128.0 // Normalized amplitude between 0.0 and 1.0
//     }
//
//     fn bits_to_duration(bits: &[u8]) -> Duration {
//         let value = Self::bits_to_value(bits);
//         Duration::from_millis(100 + value as u64 * 50) // Duration range from 100ms to ~1350ms
//     }
//
//     fn bits_to_phase(bits: &[u8]) -> f32 {
//         let value = Self::bits_to_value(bits);
//         value as f32 * 2.0 * PI / 255.0 // Phase between 0 and 2π
//     }
//
//     fn bits_to_value(bits: &[u8]) -> u8 {
//         bits.iter().rev().enumerate().fold(0, |acc, (i, &bit)| {
//             if i < 8 { // Ensure we do not exceed the u8 size
//                 acc + (bit << i)
//             } else {
//                 acc
//             }
//         })
//     }
//
//     fn matches_codon(segment: &[u8], codon: &[u8]) -> bool {
//         segment.len() == codon.len() && segment.iter().zip(codon.iter()).all(|(&a, &b)| a == b)
//     }
// }
