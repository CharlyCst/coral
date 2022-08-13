// use anyhow::Result;
use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use coral_bindgen::link;
use walrus::{Module, ModuleConfig};

// —————————————————————————————————— CLI ——————————————————————————————————— //

#[derive(Parser)]
struct Args {
    /// Path of the base modules
    #[clap(value_parser)]
    base: PathBuf,

    /// Couples of module names and module paths
    #[clap(value_parser)]
    modules: Vec<String>,

    /// Output path
    #[clap(long, short, value_parser)]
    output: Option<String>,
}

fn main() {
    let args = Args::parse();
    if args.modules.len() % 2 != 0 {
        println!("Modules must be specified as pair 'module_name module_path'");
        process::exit(1);
    }

    let mut base = parse_base(args.base);
    for (name, path) in args
        .modules
        .iter()
        .step_by(2)
        .zip(args.modules.iter().skip(1).step_by(2))
    {
        link_module(&mut base, name, path);
    }

    let output_path = match args.output {
        Some(path) => path,
        None => String::from("out.wasm"),
    };
    base.emit_wasm_file(output_path).unwrap();
}

fn parse_base<P: AsRef<Path>>(path: P) -> Module {
    let wasm = fs::read(path).unwrap();
    let mut config = ModuleConfig::new();
    config.generate_name_section(false);
    config.parse(&wasm).unwrap()
}

fn link_module<P: AsRef<Path>>(base: &mut Module, name: &str, path: P) {
    let wasm = fs::read(path).unwrap();
    let config = ModuleConfig::new();
    let linkee = config.parse(&wasm).unwrap();
    link(base, &linkee, name);
}
