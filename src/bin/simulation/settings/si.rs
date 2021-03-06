//! This module handles a TOML settings file.

#[cfg(feature = "single")]
use std::f32::consts::PI;
#[cfg(not(feature = "single"))]
use std::f64::consts::PI;

use serde_derive::{Deserialize, Serialize};
use std::fs::File;
use std::io::prelude::*;
use stochasticsampling::flowfield::stress::StressPrefactors;
use stochasticsampling::Float;
use toml;

error_chain! {
    foreign_links {
        TOMLError(toml::de::Error);
    }
}

const BOLTZMANN: Float = 1.380_648_52e-23;

/// Structure that holds settings, which are defined externally in a TOML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsSI {
    pub simulation: super::SimulationSettings,
    pub parameters: Parameters,
    pub environment: super::EnvironmentSettings,
}

/// Size of the simulation box in microns
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BoxSize {
    pub x: Float,
    pub y: Float,
    pub z: Float,
}

/// Holds rotational and translational diffusion constants
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiffusionConstants {
    pub translational: Float,
    pub rotational: Float,
}

/// Holds prefactors for active and magnetic stress
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Particle {
    pub radius: Float,
    #[serde(default)]
    pub shape: Float,
    pub self_propulsion_speed: Float,
    pub force_dipole: Float,
    pub magnetic_dipole_moment: Float,
    pub persistance_time: Float,
}

/// Holds phyiscal parameters
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameters {
    pub hydro_screening: Float,
    pub volume_exclusion: Float,
    pub viscocity: Float,
    pub temperature: Float,
    pub volume_fraction: Float,
    pub external_field: Float,
    pub particle: Particle,
}

/// Reads the content of a file `filename` into an string and return it.
fn read_from_file(filename: &str) -> Result<String> {
    let mut f = File::open(filename).chain_err(|| "Unable to open file.")?;
    let mut content = String::new();

    f.read_to_string(&mut content)
        .chain_err(|| "Unable to read file.")?;

    Ok(content)
}

/// Reads content of a file `param_file`, that should point to a valid TOML
/// file, and Parsers it.
/// Then returns the deserialized data in form of a Settings struct.
pub fn read_parameter_file(param_file: &str) -> Result<SettingsSI> {
    // read .toml file into string
    let toml_string = read_from_file(param_file).chain_err(|| "Unable to read parameter file.")?;

    let mut settings: SettingsSI =
        toml::from_str(&toml_string).chain_err(|| "Unable to parse parameter file.")?;

    settings.environment.version = "".to_string();

    check_settings(&settings)?;

    Ok(settings)
}

fn check_settings(s: &SettingsSI) -> Result<()> {
    // TODO Check settings for sanity. For example, particles_head <=
    // number_of_particles
    let bs = s.simulation.box_size;

    if bs.x <= 0. || bs.y <= 0. || bs.z <= 0. {
        bail!("Box size is invalid. Must be bigger than 0: {:?}", bs)
    }

    if s.simulation.output_at_timestep.particles_head.is_some()
        && s.simulation.number_of_particles
            < s.simulation.output_at_timestep.particles_head.unwrap()
    {
        bail!(
            "Cannot output more particles than available. `particles_head`
                   must be smaller or equal to `number_of_particles`"
        )
    }

    Ok(())
}

impl SettingsSI {
    pub fn into_settings(self) -> super::Settings {
        let number_density = volume_fraction_to_number_density(
            self.parameters.volume_fraction,
            self.parameters.particle.radius,
        );

        let xc = number_density.powf(-1. / 3.);
        let uc = self.parameters.particle.self_propulsion_speed;
        let tc = xc / uc;

        let stressf = number_density.powf(2. / 3.)
            / self.parameters.particle.self_propulsion_speed
            / self.parameters.viscocity;
        let stress = StressPrefactors {
            active: stressf * self.parameters.particle.force_dipole,
            magnetic: stressf
                * self.parameters.particle.magnetic_dipole_moment
                * self.parameters.external_field,
        };

        let rotfriction =
            8. * PI * self.parameters.viscocity * self.parameters.particle.radius.powi(3);
        let transfriction = 6. * PI * self.parameters.viscocity * self.parameters.particle.radius;

        let rotdiff_brown = BOLTZMANN * self.parameters.temperature / rotfriction;

        // DOI: 101  10.1209/0295-5075/101/20010
        let rotdiff_active = 1. / 2. / self.parameters.particle.persistance_time;

        let transdiff_brown = BOLTZMANN * self.parameters.temperature / transfriction;

        let diff = super::DiffusionConstants {
            translational: number_density.powf(1. / 3.) / uc * transdiff_brown,
            rotational: number_density.powf(-1. / 3.) / uc * (rotdiff_brown + rotdiff_active),
        };

        let alignment_parameter = self.parameters.particle.magnetic_dipole_moment
            * self.parameters.external_field
            / rotfriction
            / (rotdiff_brown + rotdiff_active);

        let mut res = super::Settings {
            simulation: self.simulation,
            parameters: super::Parameters {
                diffusion: diff,
                stress: stress,
                magnetic_reorientation: alignment_parameter * diff.rotational,
                magnetic_dipole: super::MagneticDipolePrefactors {
                    magnetic_dipole_dipole: number_density.powf(2. / 3.) / uc / rotfriction
                        * 4.0e-7
                        * PI
                        * self.parameters.particle.magnetic_dipole_moment.powi(2),
                },
                volume_exclusion: self.parameters.volume_exclusion,
                shape: self.parameters.particle.shape,
                hydro_screening: self.parameters.hydro_screening,
                magnetic_drag: number_density / uc / transfriction
                    * 4.0e-7
                    * PI
                    * self.parameters.particle.magnetic_dipole_moment.powi(2),
            },
            environment: self.environment.clone(),
        };

        res.simulation.box_size = super::BoxSize {
            x: self.simulation.box_size.x / xc,
            y: self.simulation.box_size.y / xc,
            z: self.simulation.box_size.z / xc,
        };

        res.simulation.timestep = self.simulation.timestep / tc;

        res
    }
}

fn volume_fraction_to_number_density(volfrac: Float, radius: Float) -> Float {
    let volp = 4. / 3. * PI * radius.powi(3);

    volfrac / volp
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn test_conversion() {
// let settings =
// read_parameter_file("./test/parameter_si.toml").unwrap(); println!("{:
// ?}", settings.into_settings());         unimplemented!();
//     }
// }
