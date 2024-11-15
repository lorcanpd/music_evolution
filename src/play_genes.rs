use rodio::{OutputStream, Sink};
use std::error::Error;
use std::f32::consts::PI;
use std::time::Duration;
use hound; // Added for WAV file writing

use crate::decode_genome::{DecodedGenome, Effect};

/// Plays the decoded genome using `rodio` for debugging purposes.
pub fn play_genes(decoded: &DecodedGenome) -> Result<(), Box<dyn Error>> {
    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&stream_handle)?;

    let combined_samples = generate_samples(decoded);

    let sample_rate = 44100;
    sink.append(rodio::buffer::SamplesBuffer::new(1, sample_rate, combined_samples));
    sink.sleep_until_end();
    Ok(())
}

/// Generates a WAV file from the decoded genome and saves it to the specified filename.
pub fn generate_wav(decoded: &DecodedGenome, filename: &str) -> Result<(), Box<dyn Error>> {
    let combined_samples = generate_samples(decoded);

    let sample_rate = 44100;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sample_rate as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(filename, spec)?;

    for sample in combined_samples {
        // Convert f32 samples in [-1.0, 1.0] to i16
        let amplitude = (sample * i16::MAX as f32) as i16;
        writer.write_sample(amplitude)?;
    }

    writer.finalize()?;
    Ok(())
}

/// Generates WAV data from the decoded genome and returns it as a `Vec<u8>`.
/// Useful for streaming the audio data.
pub fn generate_wav_data(decoded: &DecodedGenome) -> Result<Vec<u8>, Box<dyn Error>> {
    let combined_samples = generate_samples(decoded);

    let sample_rate = 44100;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sample_rate as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut cursor, spec)?;

    for sample in combined_samples {
        // Convert f32 samples in [-1.0, 1.0] to i16
        let amplitude = (sample * i16::MAX as f32) as i16;
        writer.write_sample(amplitude)?;
    }

    writer.finalize()?;

    Ok(cursor.into_inner())
}

/// Generates the audio samples from the decoded genome.
fn generate_samples(decoded: &DecodedGenome) -> Vec<f32> {
    // Calculate the total length considering the start times and durations of all notes
    let total_length = decoded
        .notes
        .iter()
        .map(|n| n.start_time + n.duration)
        .max()
        .unwrap_or_default();
    println!("Total song length: {}ms", total_length.as_millis());

    let sample_rate = 44100;
    let mut combined_samples = vec![0.0; (total_length.as_secs_f32() * sample_rate as f32) as usize];

    for note in &decoded.notes {
        generate_wave(
            &mut combined_samples,
            note.start_time,
            note.frequency,
            note.amplitude,
            note.duration,
            note.phase,
        );
    }

    for effect in &decoded.effects {
        match effect {
            Effect::LowPass(cutoff) => apply_low_pass_filter(&mut combined_samples, *cutoff),
            Effect::HighPass(cutoff) => apply_high_pass_filter(&mut combined_samples, *cutoff),
            Effect::Reverb(delay, feedback) => apply_reverb(&mut combined_samples, *delay, *feedback),
            Effect::Echo(delay, feedback) => apply_echo(&mut combined_samples, *delay, *feedback),
        }
    }

    combined_samples
}

fn generate_wave(
    samples: &mut Vec<f32>,
    start_time: Duration,
    frequency: f32,
    amplitude: f32,
    duration: Duration,
    phase: f32,
) {
    let sample_rate = 44100;
    let start_sample = (start_time.as_secs_f32() * sample_rate as f32) as usize;
    let end_sample = (start_sample
        + (duration.as_secs_f32() * sample_rate as f32) as usize)
        .min(samples.len());

    for sample_index in start_sample..end_sample {
        let time = (sample_index - start_sample) as f32 / sample_rate as f32;
        samples[sample_index] += amplitude * (2.0 * PI * frequency * time + phase).sin();
    }
}

fn apply_low_pass_filter(samples: &mut Vec<f32>, cutoff: f32) {
    let mut previous = 0.0;
    for sample in samples.iter_mut() {
        previous = previous + cutoff * (*sample - previous);
        *sample = previous;
    }
}

fn apply_high_pass_filter(samples: &mut Vec<f32>, cutoff: f32) {
    let mut previous = 0.0;
    for sample in samples.iter_mut() {
        let current = *sample;
        *sample = current - previous + cutoff * current;
        previous = current;
    }
}

fn apply_reverb(samples: &mut Vec<f32>, delay: Duration, feedback: f32) {
    let delay_samples = (delay.as_secs_f32() * 44100.0) as usize;
    if delay_samples == 0 {
        return; // Avoid division by zero
    }
    let mut buffer = vec![0.0; delay_samples];
    let mut index = 0;
    for sample in samples.iter_mut() {
        let delayed_sample = buffer[index];
        let output = *sample + delayed_sample * feedback;
        buffer[index] = output;
        index = (index + 1) % delay_samples;
        *sample = output;
    }
}

fn apply_echo(samples: &mut Vec<f32>, delay: Duration, feedback: f32) {
    let delay_samples = (delay.as_secs_f32() * 44100.0) as usize;
    if delay_samples == 0 {
        return; // Avoid division by zero
    }
    let mut buffer = vec![0.0; delay_samples];
    let mut index = 0;
    for sample in samples.iter_mut() {
        let delayed_sample = buffer[index];
        let output = *sample + delayed_sample * feedback;
        buffer[index] = *sample;
        index = (index + 1) % delay_samples;
        *sample = output;
    }
}
