#![crate_type = "bin"]
#![recursion_limit = "1024"]

#[macro_use]
extern crate error_chain;

mod errors {
    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain! {}
}
mod init;
mod output;
mod simulation;
mod timedisplay;

use self::simulation::settings::{self, Settings};
use self::simulation::Simulation;
use crate::errors::*;
use crate::init::InitType;
use crate::output::path::OutputPath;
use crate::output::worker::Worker;
use clap::load_yaml;
use clap::App;
use colored::*;
use log::{debug, error, info};
use pbr::ProgressBar;
use std::path::Path;
use stochasticsampling::output::OutputEntry;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn version() -> String {
    let mut s = String::from("");
    if cfg!(feature = "single") {
        s.push_str("-s");
    }
    if cfg!(feature = "quasi2d") {
        s.push_str("-q2d");
    };
    format!("{}-{}{}", VERSION, env!("VERGEN_SHA_SHORT"), s)
}

fn main() {
    // initialize the env_logger implementation
    env_logger::init();

    // error handling of runner
    if let Err(ref e) = run() {
        error!("{}: {}", "error".red(), e);

        for e in e.iter().skip(1) {
            error!("caused by: {}", e);
        }

        // The backtrace is not always generated. Try to run this with
        // `RUST_BACKTRACE=1`.
        if let Some(backtrace) = e.backtrace() {
            error!("backtrace: {:?}", backtrace);
        }

        ::std::process::exit(1);
    }

    ::std::process::exit(0);
}

/// Main function
fn run() -> Result<()> {
    // Parse command line
    let yaml = load_yaml!("cli.yml");
    let cli_matches = App::from_yaml(yaml)
        .version(version().as_str())
        .get_matches();

    let settings_file_name = cli_matches.value_of("parameter_file").unwrap();

    let mut settings = if cli_matches.is_present("si_units") {
        settings::si::read_parameter_file(settings_file_name)
            .chain_err(|| "Error reading parameter file.")?
            .into_settings()
    } else {
        settings::read_parameter_file(settings_file_name)
            .chain_err(|| "Error reading parameter file.")?
    };

    let output_dir = Path::new(cli_matches.value_of("output_directory").unwrap());
    let path = OutputPath::new(output_dir, &settings.environment.prefix);
    path.create()
        .chain_err(|| "Cannot create output directory")?;

    // TOML does not work well at the moment
    let param_name = path.with_extension("toml");
    settings
        .save_to_file(param_name.to_str().unwrap())
        .chain_err(|| "Unable to save parameter file in simulation units.")?;

    settings.set_version(&version());
    // drop mutability for safety
    let settings = settings;

    let init_type = if cli_matches.is_present("initial_condition") {
        InitType::Stdin
    } else if settings.environment.init_file.is_none() {
        if cli_matches.is_present("resume") {
            bail!("Cannot resume. Pleas provide snapshot in `init_file` field in parameter file.");
        }
        InitType::Distribution
    } else if cli_matches.is_present("resume") {
        InitType::Resume
    } else {
        InitType::File
    };

    let mut simulation = init::init_simulation(&settings, init_type)
        .chain_err(|| "Error during initialization of simulation.")?;

    let show_progress = cli_matches.is_present("progress_bar");

    let worker = Worker::new(
        settings.environment.io_queue_size,
        &path,
        settings.environment.output_format,
    )
    .chain_err(|| "Unable to create output thread.")?;

    worker
        .write_metadata(settings.clone())
        .chain_err(|| "Unable to write metadata to output.")?;

    Ok(run_simulation(
        &settings,
        &mut simulation,
        worker,
        show_progress,
    )?)
}

/// Spawns output thread and run simulation.
fn run_simulation(
    settings: &Settings,
    simulation: &mut Simulation,
    out: Worker,
    show_progress: bool,
) -> Result<()> {
    if settings.simulation.output_at_timestep.initial_condition {
        info!("Saving initial condition.");
        let mut initial = OutputEntry::default();
        initial.distribution = Some(simulation.get_distribution());
        initial.particles = if settings.simulation.output_at_timestep.particles.is_some() {
            settings
                .simulation
                .output_at_timestep
                .particles_head
                .and_then(|n| Some(simulation.get_particles_head(n)))
                .or_else(|| Some(simulation.get_particles()))
        } else {
            None
        };

        out.append(initial)
            .chain_err(|| "Unable to append initial condition.")?;
    }

    let mut pb = ProgressBar::new(settings.simulation.number_of_timesteps as u64);
    pb.format("┫██░┣");

    // only show bar, if flag was present
    pb.show_bar = show_progress;
    pb.show_counter = show_progress;
    pb.show_percent = show_progress;
    pb.show_speed = show_progress;
    pb.show_time_left = show_progress;
    pb.show_message = show_progress;

    // in case the simulation was resumed
    let timestep_start = simulation.get_timestep() + 1;
    let n = settings.simulation.number_of_timesteps + timestep_start;

    let start_time = time::now();

    // Run the simulation and send data to asynchronous to the IO-thread.
    for timestep in timestep_start..=n {
        pb.inc();
        simulation.do_timestep();

        // TODO: Refactor this ugly code

        // Build entry
        let entry = OutputEntry {
            distribution: settings
                .simulation
                .output_at_timestep
                .distribution
                .and_then(|x| {
                    if timestep % x == 0 {
                        info!("Timestep {}: Save distribution...", timestep);
                        Some(simulation.get_distribution())
                    } else {
                        None
                    }
                }),
            flowfield: settings
                .simulation
                .output_at_timestep
                .flowfield
                .and_then(|x| {
                    if timestep % x == 0 {
                        info!("Timestep {}: Save flow-field...", timestep);
                        Some(simulation.get_flow_field())
                    } else {
                        None
                    }
                }),
            magneticfield: settings
                .simulation
                .output_at_timestep
                .magneticfield
                .and_then(|x| {
                    if timestep % x == 0 {
                        info!("Timestep {}: Save magnetic-field...", timestep);
                        Some(simulation.get_magnetic_field())
                    } else {
                        None
                    }
                }),
            particles: settings
                .simulation
                .output_at_timestep
                .particles
                .and_then(|x| {
                    if timestep % x == 0 {
                        info!("Timestep {}: Save particles...", timestep);
                        settings
                            .simulation
                            .output_at_timestep
                            .particles_head
                            .and_then(|x| Some(simulation.get_particles_head(x)))
                            .or_else(|| Some(simulation.get_particles()))
                    } else {
                        None
                    }
                }),
            timestep: timestep,
        };

        if entry.distribution.is_some() || entry.flowfield.is_some() || entry.particles.is_some() {
            debug!("Some output is appended to queue.");
            match out.append(entry) {
                Ok(_) => (),
                Err(_) => return out.emergency_join(),
            };
        }

        match settings.simulation.output_at_timestep.snapshot {
            Some(x) if timestep % x == 0 => {
                info!("Timestep {}: Save snapshot...", timestep);
                let snapshot = simulation.get_snapshot();
                out.write_snapshot(snapshot)
            }
            _ => Ok(()),
        }?;
    }

    pb.finish_print(&format!("✓ {} ", "DONE".green().bold()));
    // TODO Why is this necessary?
    println!("");

    if settings.simulation.output_at_timestep.final_snapshot {
        let snapshot = simulation.get_snapshot();
        out.write_snapshot(snapshot)
            .chain_err(|| "Error writing last snapshot.")?;
    }

    print!("Writing buffer to disk… ");
    let opath = out.get_output_filepath().to_str().unwrap().to_string();

    out.quit()?;

    println!("DONE '{}'.", opath);

    let stop_time = time::now();
    let duration = stop_time - start_time;
    println!(
        "Elapsed time: {}",
        timedisplay::pretty_print_duration(duration)
    );

    Ok(())
}
