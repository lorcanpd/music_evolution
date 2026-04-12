# Music Evolution Project

**Status:** Live for preliminary public testing at [music-evo.com](https://music-evo.com)

## Overview

Music Evolution is an experimental Rust project inspired by [DarwinTunes](http://darwintunes.org/). It evolves short musical compositions using mutation, crossover, migration between habitat nodes, and listener ratings collected through a web interface.

The project is now running in a live test deployment:

- Website: [music-evo.com](https://music-evo.com)
- Public rating page: [music-evo.com/rate_songs](https://music-evo.com/rate_songs)
- Greatest hits page: [music-evo.com/greatest_hits](https://music-evo.com/greatest_hits)

This deployment is still experimental. Expect ongoing changes to the evolutionary logic, infrastructure, and user experience.

The public deployment keeps bootstrap and admin routes behind deployment-side authentication. The repo includes the route wiring and nginx protection, but anyone reimplementing or self-hosting this project will need to provide their own secrets and admin credentials such as environment variables and `htpasswd` material.

If you want to support the project: [Buy me a coffee](https://buymeacoffee.com/lorcanpd)

## Features

- Genetic encoding of musical parameters in a binary genome
- Listener-driven selection through a public web interface
- Reproduction with crossover and mutation
- Habitat structure with node capacities and migration between islands
- Greatest-hits archive generated from genomes stored in Postgres
- Production deployment with Nginx, Postgres, and Slurm-backed reproduction jobs

## Getting Started

### Prerequisites

- Rust
- Cargo
- PostgreSQL

### Installation

1. Clone the repository:
```sh
git clone https://github.com/lorcanpd/music_evolution.git
cd music_evolution
```

2. Build the project:
```sh
cargo build
```

3. Run the web application locally:
```sh
cargo run --bin web_server
```

4. Or run the reproduction binary directly:
```sh
cargo run --bin reproduce -- --current-gen 1 --next-gen 2 --wav-dir ./current_generation
```

## Deployment Notes

- Local development and production deployment use different compose overrides
- Production currently runs on a Raspberry Pi cluster
- Public access is provided through Cloudflare Tunnel
- Reproduction jobs run through Slurm on worker nodes

Relevant docs:

- [docs/public-access.md](docs/public-access.md)
- [docs/deploy-martin.md](docs/deploy-martin.md)
- [docs/slurm-offload.md](docs/slurm-offload.md)

## Project Structure

- `src/web_interface.rs`: web routes, landing page, health endpoint, greatest-hits page
- `src/user_interaction.rs`: rating flow and form handling
- `src/reproduction.rs`: evolutionary reproduction logic
- `src/greatest_hits.rs`: greatest-hits archive generation and serving
- `src/database.rs`: schema creation and habitat population
- `src/genome.rs`: genome representation and SQL serialization
- `src/decode_genome.rs`: genome decoding into musical parameters
- `src/play_genes.rs`: WAV generation and playback-related helpers
- `habitat_config.json`: habitat nodes, capacities, and migration probabilities

## Contributing

The project is still evolving quickly and is not yet set up for general external contributions.

## Acknowledgements

- Inspired by [DarwinTunes](http://darwintunes.org/)
- Uses Rust audio and web tooling, including `rodio` and `rocket`
