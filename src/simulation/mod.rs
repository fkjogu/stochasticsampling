//! Module that defines data structures and algorithms for the integration of
//! the simulation.

pub mod distribution;
pub mod flowfield;
pub mod integrators;
pub mod magnetic_interaction;
pub mod mesh;
pub mod output;
pub mod particle;
pub mod polarization;
pub mod settings;
pub mod vector;

use self::distribution::Distribution;
use self::flowfield::spectral_solver::SpectralSolver;
use self::flowfield::FlowField3D;
use self::integrators::langevin::{IntegrationParameter, Integrator, RandomVector};
use self::magnetic_interaction::magnetic_solver::MagneticSolver;
use self::particle::Particle;
use self::settings::{Settings, StressPrefactors};
use consts::TWOPI;
use extprim;
use ndarray::{Array, Ix4};
use pcg_rand::Pcg64;
use rand::distributions::normal::StandardNormal;
use rand::distributions::{IndependentSample, Range};
use rand::{Rand, SeedableRng};
use rayon;
use rayon::prelude::*;
use std::env;
use std::str::FromStr;
use fftw3::fft;

struct ValueCache {
    rot_diff: f64,
}

/// Main data structure representing the simulation.
pub struct Simulation {
    integrator: Integrator,
    spectral_solver: SpectralSolver,
    magnetic_solver: MagneticSolver,
    settings: Settings,
    state: SimulationState,
    vcache: ValueCache,
}

/// Holds the current state of the simulation.
struct SimulationState {
    distribution: Distribution,
    particles: Vec<Particle>,
    random_samples: Vec<RandomVector>,
    rng: Vec<Pcg64>,
    /// count timesteps
    timestep: usize,
}

/// Seed of PCG PRNG
type Pcg64Seed = [u64; 4];

/// Captures the full state of the simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    particles: Vec<Particle>,
    rng_seed: Vec<Pcg64Seed>,
    /// current timestep number
    timestep: usize,
}

impl Simulation {
    /// Return a new simulation data structure, holding the state of the
    /// simulation.
    pub fn new(settings: Settings) -> Simulation {
        // helper bindings for brevity
        let sim = settings.simulation;
        let param = settings.parameters;

        let scaled_stress_prefactors = StressPrefactors {
            active: param.stress.active,
            magnetic: 0.5 * param.stress.magnetic,
        };

        let int_param = IntegrationParameter {
            timestep: sim.timestep,
            // see documentation of `integrator.evolve_particle_inplace` for a rational
            trans_diffusion: (2. * param.diffusion.translational * sim.timestep).sqrt(),
            rot_diffusion: (2. * param.diffusion.rotational * sim.timestep).sqrt(),
            magnetic_reorientation: param.magnetic_reorientation,
            drag: param.drag,
            magnetic_dipole_dipole: param.magnetic_dipole.magnetic_dipole_dipole,
        };

        let integrator = Integrator::new(sim.grid_size, sim.box_size, int_param);

        let spectral_solver =
            SpectralSolver::new(sim.grid_size, sim.box_size, scaled_stress_prefactors);

        let magnetic_solver = MagneticSolver::new(sim.grid_size, sim.box_size);

        // normal distribution with variance timestep
        let seed = [sim.seed[0], sim.seed[1]];

        let num_threads = env::var("RAYON_NUM_THREADS")
            .ok()
            .and_then(|s| usize::from_str(&s).ok())
            .expect("No environment variable 'RAYON_NUM_THREADS' set.");

        rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global()
            .unwrap();

        // Initialze threads of FFTW
        fft::fftw_init(Some(num_threads)).unwrap();

        let rng = (0..num_threads)
            .into_iter()
            .map(|i| SeedableRng::from_seed([seed[0] + i as u64, seed[1] + i as u64]))
            .collect();

        // initialize state with zeros
        let state = SimulationState {
            distribution: Distribution::new(sim.grid_size, sim.box_size),
            particles: Vec::with_capacity(sim.number_of_particles),
            random_samples: vec![
                RandomVector {
                    x: 0.,
                    y: 0.,
                    z: 0.,
                    axis_angle: 0.,
                    rotate_angle: 0.,
                };
                sim.number_of_particles
            ],
            rng: rng,
            timestep: 0,
        };

        Simulation {
            integrator: integrator,
            spectral_solver: spectral_solver,
            magnetic_solver: magnetic_solver,
            settings: settings,
            state: state,
            vcache: ValueCache {
                rot_diff: (2. * param.diffusion.rotational * sim.timestep).sqrt(),
            },
        }
    }

    /// Initialize the state of the simulation
    pub fn init(&mut self, mut particles: Vec<Particle>) {
        assert!(
            particles.len() == self.settings.simulation.number_of_particles,
            "Given initial condition has not the same number of particles ({}) as given in \
             the parameter file ({}).",
            particles.len(),
            self.settings.simulation.number_of_particles
        );

        let bs = self.settings.simulation.box_size;

        // IMPORTANT: Set also the modulo quotiont for every particle, since it is not
        // provided for user given input.
        for p in &mut particles {
            // this makes sure, the input is sanitized
            *p = Particle::new(
                p.position.x,
                p.position.y,
                p.position.z,
                p.orientation.phi,
                p.orientation.theta,
                bs,
            );
        }

        self.state.particles = particles;

        // Do a first sampling, so that the initial condition can also be obtained
        self.state.distribution.sample_from(&self.state.particles);

        self.state.distribution.dist *= self.settings.simulation.box_size.x
            * self.settings.simulation.box_size.y
            * self.settings.simulation.box_size.z;
    }

    /// Resumes from a given snapshot
    pub fn resume(&mut self, snapshot: Snapshot) {
        self.init(snapshot.particles);

        // Reset timestep
        self.state.timestep = snapshot.timestep;
        for (r, s) in self.state.rng.iter_mut().zip(snapshot.rng_seed) {
            r.reseed(s);
        }
    }

    /// Returns a fill Snapshot
    pub fn get_snapshot(&self) -> Snapshot {
        let seed: Vec<[extprim::u128::u128; 2]> =
            self.state.rng.iter().map(|r| r.extract_seed()).collect();

        Snapshot {
            particles: self.state.particles.clone(),
            // assuming little endianess
            rng_seed: seed.iter()
                .map(|s| [s[0].lo, s[0].hi, s[1].lo, s[1].hi])
                .collect(),
            timestep: self.state.timestep,
        }
    }

    // Getter
    /// Returns all particles
    pub fn get_particles(&self) -> Vec<Particle> {
        self.state.particles.clone()
    }

    /// Returns the first `n` particles
    pub fn get_particles_head(&self, n: usize) -> Vec<Particle> {
        self.state.particles[..n].to_vec()
    }

    /// Returns sampled distribution field
    pub fn get_distribution(&self) -> Distribution {
        self.state.distribution.clone()
    }

    /// Returns sampled flow field
    pub fn get_flow_field(&self) -> FlowField3D {
        self.spectral_solver.get_real_flow_field()
    }

    /// Returns magnetic field
    pub fn get_magnetic_field(&self) -> Array<f64, Ix4> {
        self.magnetic_solver.get_real_magnet_field()
    }

    /// Returns current timestep
    pub fn get_timestep(&self) -> usize {
        self.state.timestep
    }

    /// Do the actual simulation timestep
    pub fn do_timestep(&mut self) -> usize {
        // Sample probability distribution from ensemble.
        self.state.distribution.sample_from(&self.state.particles);
        // Renormalize distribution to keep number density constant.
        self.state.distribution.dist *= self.settings.simulation.box_size.x
            * self.settings.simulation.box_size.y
            * self.settings.simulation.box_size.z;

        // Calculate flow field from distribution.
        self.spectral_solver
            .update_flow_field(&self.state.distribution);
        let ff = self.spectral_solver.get_real_flow_field();

        let (md_b, md_grad_b) = self.magnetic_solver
            .mean_magnetic_field(&self.state.distribution);

        let between = Range::new(0f64, 1.);

        let chunksize = self.state.random_samples.len() / self.state.rng.len() + 1;

        let rot_diff = self.vcache.rot_diff;

        self.state
            .random_samples
            .par_chunks_mut(chunksize)
            .zip(self.state.rng.par_iter_mut())
            .for_each(|(c, mut rng)| {
                for r in c.iter_mut() {
                    *r = RandomVector {
                        x: StandardNormal::rand(&mut rng).0,
                        y: StandardNormal::rand(&mut rng).0,
                        z: StandardNormal::rand(&mut rng).0,
                        axis_angle: TWOPI * between.ind_sample(&mut rng),
                        rotate_angle: rayleigh_pdf(rot_diff, between.ind_sample(&mut rng)),
                    };
                }
            });

        // Update particle positions
        self.integrator.evolve_particles_inplace(
            &mut self.state.particles,
            &self.state.random_samples,
            ff.view(),
            md_b,
            md_grad_b,
        );

        // increment timestep counter to keep a continous identifier when resuming
        self.state.timestep += 1;
        self.state.timestep
    }
}

impl Drop for Simulation {
    fn drop(&mut self) {
        fft::fttw_finalize();
    }
}

fn rayleigh_pdf(sigma: f64, x: f64) -> f64 {
    sigma * f64::sqrt(-2. * f64::ln(1. - x))
}

impl Iterator for Simulation {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        Some(self.do_timestep())
    }
}
