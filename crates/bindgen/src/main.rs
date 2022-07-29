use anyhow::Result;
use clap::Parser;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use coral_bindgen::{ImportedFuncPatch, Patcher};

// —————————————————————————————————— CLI ——————————————————————————————————— //

#[derive(Parser)]
struct Args {
    /// Path to the wasm file to patch
    #[clap(value_parser)]
    wasm: PathBuf,

    /// Path to the patch file
    #[clap(value_parser)]
    patch: PathBuf,

    /// Output path
    #[clap(long, short, value_parser)]
    output: Option<String>,
}

fn main() {
    let args = Args::parse();
    match patch(args) {
        Ok(_) => (),
        Err(err) => {
            println!("Error: {:?}", err);
            process::exit(1)
        }
    };
}

// ————————————————————————————————— Patch —————————————————————————————————— //

struct Patch {
    tables: HashMap<String, Table>,
    functions: Vec<Function>,
}

fn patch(args: Args) -> Result<()> {
    let patch = parse_config(&args.patch)?;
    let file = std::fs::read(args.wasm)?;

    let mut patcher = Patcher::new(&file).expect("Failed to parse wasm file");

    let mut table_ids = HashMap::new();
    for (table_name, table) in patch.tables.into_iter() {
        let table_id = match table {
            Table::Imported {
                from,
                name,
                min_size,
                max_size,
            } => patcher.add_table_import(&from, &name, min_size, max_size),
            Table::Owned { min_size, max_size } => patcher.add_table(min_size, max_size),
        };
        table_ids.insert(table_name, table_id);
    }

    for function_patch in patch.functions {
        match function_patch {
            Function::Imported {
                from,
                name,
                patches,
            } => {
                let mut builder = ImportedFuncPatch::new();
                for patch in patches {
                    let table_id = if let Some(id) = table_ids.get(&patch.table) {
                        id
                    } else {
                        anyhow::bail!("Unknown table: '{}'", &patch.table);
                    };
                    builder.replace_with_handle(patch.arg_id, *table_id);
                }
                patcher.register_patch(&from, &name, builder.as_patch());
            }
        }
    }

    let mut module = patcher.patch().expect("Failed to transform module");
    let output_path = match args.output {
        Some(path) => path,
        None => String::from("out.wasm"),
    };
    module.emit_wasm_file(output_path)?;

    Ok(())
}

// ————————————————————————————— Configuration —————————————————————————————— //

enum Table {
    Imported {
        from: String,
        name: String,
        min_size: u32,
        max_size: Option<u32>,
    },
    Owned {
        min_size: u32,
        max_size: Option<u32>,
    },
}

enum Function {
    Imported {
        from: String,
        name: String,
        patches: Vec<ArgPatch>,
    },
}

struct ArgPatch {
    arg_id: u32,
    table: String,
}

fn parse_config(path: &Path) -> Result<Patch> {
    let file = fs::read_to_string(path)?;
    let file = file.parse::<toml::Value>()?;
    let file = match file {
        toml::Value::Table(table) => table,
        _ => anyhow::bail!("The patch file is not a valid TOML file"),
    };

    let mut tables = HashMap::new();
    let mut functions = Vec::new();

    for (key, value) in &file {
        match key.as_str() {
            "table" | "tables" => tables.extend(parse_tables(value)?),
            "function" | "functions" => functions.extend(parse_function(value)?),
            _ => anyhow::bail!("Invalid TOML table: {}", key),
        }
    }

    Ok(Patch { tables, functions })
}

fn parse_tables(values: &toml::Value) -> Result<HashMap<String, Table>> {
    let values = as_toml_table(values)?;
    let mut tables = HashMap::new();

    for (table_name, items) in values {
        let mut name = None;
        let mut from = None;
        let mut min_size = None;
        let mut max_size = None;

        let items = as_toml_table(items)?;
        for (key, value) in items {
            match key.as_str() {
                "from" => from = Some(as_toml_string(value)?.to_string()),
                "name" => name = Some(as_toml_string(value)?.to_string()),
                "min_size" => min_size = Some(as_toml_int(value)?),
                "max_size" => max_size = Some(as_toml_int(value)?),
                _ => anyhow::bail!("Unexpected table field: {}", key),
            }
        }

        let min_size = match min_size {
            Some(size) => size,
            None => anyhow::bail!("Missing 'min_size' in table '{}'", table_name),
        };
        let table = match (name, from) {
            (Some(name), Some(from)) => Table::Imported {
                from,
                name,
                min_size,
                max_size,
            },
            _ => Table::Owned { min_size, max_size },
        };
        tables.insert(table_name.to_string(), table);
    }

    Ok(tables)
}

fn parse_function(value: &toml::Value) -> Result<Vec<Function>> {
    let values = as_toml_table(value)?;
    let mut functions = Vec::new();

    for (name, items) in values {
        let name = name.to_string();
        let mut from = None;
        let mut patches = Vec::new();

        let items = as_toml_table(items)?;
        for (key, value) in items {
            match key.as_str() {
                "from" => from = Some(as_toml_string(value)?.to_string()),
                "patches" => patches.extend(parse_arg_patches(value)?),
                _ => anyhow::bail!("Unexpected function field: {}", key),
            }
        }

        if let Some(from) = from {
            functions.push(Function::Imported {
                from,
                name,
                patches,
            })
        } else {
            anyhow::bail!("Only imported functions are supported for now");
        }
    }

    Ok(functions)
}

fn parse_arg_patches(value: &toml::Value) -> Result<Vec<ArgPatch>> {
    let mut patches = Vec::new();
    let items = as_toml_array(value)?;
    for patch in items {
        patches.push(parse_arg_patch(patch)?);
    }

    Ok(patches)
}

fn parse_arg_patch(value: &toml::Value) -> Result<ArgPatch> {
    let mut arg_id = None;
    let mut table = None;

    let value = as_toml_table(value)?;
    for (key, value) in value {
        match key.as_str() {
            "arg" => arg_id = Some(as_toml_int(value)?),
            "table" => table = Some(as_toml_string(value)?.to_string()),
            _ => anyhow::bail!("Unexpected patch field: {}", key),
        }
    }

    match (arg_id, table) {
        (Some(arg_id), Some(table)) => Ok(ArgPatch { arg_id, table }),
        _ => anyhow::bail!("Malformed patch"),
    }
}

fn as_toml_table(value: &toml::Value) -> Result<&toml::value::Table> {
    match value {
        toml::Value::Table(table) => Ok(table),
        _ => anyhow::bail!("Malformed TOML, expected table"),
    }
}

fn as_toml_array(value: &toml::Value) -> Result<&toml::value::Array> {
    match value {
        toml::Value::Array(array) => Ok(array),
        _ => anyhow::bail!("Malformed TOML, expected array"),
    }
}

fn as_toml_string(value: &toml::Value) -> Result<&str> {
    match value {
        toml::Value::String(string) => Ok(string),
        _ => anyhow::bail!("Malformed TOML, expected string"),
    }
}

fn as_toml_int(value: &toml::Value) -> Result<u32> {
    match value {
        toml::Value::Integer(int) => Ok(u32::try_from(*int)?),
        _ => anyhow::bail!("Malformed TOML, expected integer"),
    }
}
