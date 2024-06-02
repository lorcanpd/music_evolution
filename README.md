# Music Evolution Project

**Status: Under Development**

## Overview

The Music Evolution Project is an experimental Rust application inspired by [DarwinTunes](http://darwintunes.org/) designed to evolve musical compositions using genetic algorithms. The project aims to explore the creation of complex musical compositions through the application of evolutionary principles, allowing for the mutation, crossover, and selection of musical genomes. Initially, this project will emulate the functionality of DarwinTunes, but it is hoped that more features could be developed such as a landscape in which musical compositions compete, mate, and evolve.

## Features

- **Genetic Encoding**: Musical parameters such as frequency, amplitude, duration, and phase are encoded within a binary genome.
- **Mutation Rate**: The mutation rate is encoded in the first 8 bits of the genome, normalized between 0 and 1.
- **Total Song Length**: The total length of the song is encoded in the next 16 bits.
- **Note Encoding**: Each note is encoded in chunks, allowing for varying genome lengths and handling incomplete chunks as non-coding regions.
- **Complex Waveform Generation**: Generates complex waveforms by superimposing multiple harmonics.
- **Dynamic Genome Handling**: Supports genomes of varying lengths, capable of handling insertions and deletions.

## Getting Started

### Prerequisites

- **Rust**: Ensure you have the Rust programming language installed. You can download and install Rust from [rust-lang.org](https://www.rust-lang.org/).

### Installation

1. Clone the repository:

   ```sh
   git clone https://github.com/lorcanpd/music_evolution.git
   cd music_evolution
Build the project:

sh
Copy code
cargo build
Running the Project
To run the project, execute the following command:

sh
Copy code
cargo run
Example Output
```{bash}
Genome: [0, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 0, 0, 0, 0, 1, 1, 0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 1, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 0, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 0, 0, 1, 1, 1, 1, 0, 1, 0, 0, 1, 0, 1, 0, 0, 1, 1, 0, 0]
Total song length: 480ms
Playing note 1: frequency = 912, amplitude = 1.734375, duration = 300ms, phase = 0.41887903
```
## Project Structure
* src/main.rs: The main entry point of the application, containing the logic for reading genomes, processing musical notes, and playing the resulting waveforms.

## Contributing
This project is currently under early development and is not yet open for contributions.


## Acknowledgements
This project uses the rodio crate for audio playback.
Special thanks to the Rust community for their support and contributions.
Inspired by [DarwinTunes](http://darwintunes.org/)