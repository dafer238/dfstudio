//! `dfs` — run DeltaF Studio scripts and projects from the command line.
//!
//! ```text
//! dfs run   model.dfs [--units eng|si|si-kpa|imperial]   solve and print variables
//! dfs check model.dfs                                 structural analysis only
//! dfs info  model.df                                  list the sections of a project file
//! dfs pack  model.dfs -o model.df [--attach f ...]    solve and save as a .df project
//!           [--project state.json]                    app state (diagram, plots, studies) as PROJ
//!           [--table name=file.csv ...]               numeric tables (header `name [unit]`)
//! dfs unpack model.df [dir]                           extract script, results (CSV), tables, files
//! dfs export model.dfs [--units ...]                  print the simplified script (one equation per line)
//! ```
//! `run` and `check` accept `.df` projects as well as text scripts.

use dfcore::dffile::{is_df, Artifact, DfFile, Results, Table};
use dfcore::{compile, solver, CompileOptions, Severity, SolveOptions, UnitSystem, VarStatus};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "usage: dfs <run|check|info|pack|unpack|export> <file> [--units eng|si|si-kpa|imperial] [-o out.df] [--attach files...] [--project state.json] [--table name=file.csv ...]"
        );
        return ExitCode::from(2);
    }
    let cmd = args[1].as_str();
    let path = &args[2];
    let raw = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let mut opts = CompileOptions::default();
    let mut project: Option<DfFile> = None;
    let src = if is_df(&raw) {
        match DfFile::from_bytes(&raw) {
            Ok(f) => {
                // the unit system chosen in the app travels in the project state
                opts.unit_system =
                    f.project.as_deref().and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok()).and_then(|v| serde_json::from_value(v.get("unit_system")?.clone()).ok());
                let s = f.script.clone();
                project = Some(f);
                s
            }
            Err(e) => {
                eprintln!("{path}: {e}");
                return ExitCode::from(1);
            }
        }
    } else if path.to_ascii_lowercase().ends_with(".ees") {
        match dfcore::ees::read(&raw) {
            Ok(e) => e.to_script(),
            Err(e) => {
                eprintln!("{path}: {e}");
                return ExitCode::from(1);
            }
        }
    } else {
        String::from_utf8_lossy(&raw).to_string()
    };
    match cmd {
        "info" => return info(path, &raw),
        "unpack" => {
            let Some(f) = project else {
                eprintln!("{path} is not a .df project");
                return ExitCode::from(2);
            };
            return unpack(f, args.get(3).filter(|a| !a.starts_with('-')).map(|s| s.as_str()).unwrap_or("."));
        }
        "run" | "check" | "pack" | "export" => {}
        _ => {
            eprintln!("unknown command {cmd}");
            return ExitCode::from(2);
        }
    }
    if let Some(i) = args.iter().position(|a| a == "--units") {
        opts.unit_system = match args.get(i + 1).map(|s| s.as_str()) {
            Some("si") => Some(UnitSystem::si()),
            Some("si-kpa" | "ees") => Some(UnitSystem::si_kpa()),
            Some("imperial") => Some(UnitSystem::imperial()),
            _ => Some(UnitSystem::engineering()),
        };
    }
    if cmd == "export" {
        let e = dfcore::ees_export::export(&src, &opts.unit_system.clone().unwrap_or_default());
        print!("{}", e.text);
        return ExitCode::SUCCESS;
    }
    let t0 = std::time::Instant::now();
    let model = compile(&src, &opts);
    let t_compile = t0.elapsed();
    for d in &model.diagnostics {
        let tag = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        eprintln!("{path}:{}: {tag}: {}", d.span.line, d.msg);
    }
    println!("{} equations, {} variables, unit system: {} (compiled in {:.2} ms)", model.eqs.len(), model.vars.len(), model.unit_system.name, t_compile.as_secs_f64() * 1e3);
    if model.has_errors() {
        return ExitCode::from(1);
    }
    let plan = solver::Plan::new(&model, &[]);
    let s = &plan.structure;
    println!(
        "structure: {} blocks (largest {}), determined {}, over-determined {}, under-determined {}, dof {}",
        s.blocks.len(),
        s.blocks.iter().map(|b| b.vars.len()).max().unwrap_or(0),
        s.count(VarStatus::Determined),
        s.count(VarStatus::OverDetermined),
        s.count(VarStatus::UnderDetermined),
        s.dof
    );
    if cmd == "check" {
        for (i, v) in model.vars.iter().enumerate() {
            if s.var_status[i] != VarStatus::Determined {
                println!("  {:<24} {:?}", v.display, s.var_status[i]);
            }
        }
        return ExitCode::SUCCESS;
    }
    let sopts = SolveOptions::default();
    let mut x = solver::initial_values(&model, &sopts);
    let sol = solver::solve_with_plan(&model, &plan, &mut x, &sopts);
    println!("{} ({:.2} ms)", sol.message, sol.elapsed_ms);
    let mut idx: Vec<usize> = (0..model.vars.len()).collect();
    idx.sort_by(|a, b| natord(&model.vars[*a].name, &model.vars[*b].name));
    for i in idx {
        let v = &model.vars[i];
        let (val, unit) = match model.display_unit(i as u32) {
            Some((mu, du)) => (mu.convert(sol.x[i], &du), du.symbol),
            None => (sol.x[i], String::new()),
        };
        let mark = if sol.known[i] { "" } else { "  (not solved)" };
        println!("  {:<24} = {:>16.8}  {}{}", v.display, val, unit, mark);
    }
    if cmd == "run" && !sol.converged {
        // a distinct exit status, so scripts and CI can detect a failed solve
        return ExitCode::from(3);
    }
    if cmd == "pack" {
        let out = args.iter().position(|a| a == "-o").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| {
            let p = std::path::Path::new(path);
            p.with_extension("df").to_string_lossy().to_string()
        });
        let mut f = project.unwrap_or_else(|| DfFile::new(src.clone()));
        f.results = Some(Results::from_solution(&model, &sol));
        if let Some(i) = args.iter().position(|a| a == "--attach") {
            for a in args[i + 1..].iter().take_while(|a| !a.starts_with('-')) {
                match std::fs::read(a) {
                    Ok(data) => {
                        let name = std::path::Path::new(a).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| a.clone());
                        f.files.retain(|x| x.name != name);
                        f.files.push(Artifact { name, data });
                    }
                    Err(e) => {
                        eprintln!("cannot read {a}: {e}");
                        return ExitCode::from(2);
                    }
                }
            }
        }
        if let Some(i) = args.iter().position(|a| a == "--project") {
            let Some(p) = args.get(i + 1) else {
                eprintln!("--project needs a JSON file");
                return ExitCode::from(2);
            };
            match std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map(|_| t).map_err(|e| e.to_string())) {
                Ok(t) => f.project = Some(t),
                Err(e) => {
                    eprintln!("{p}: {e}");
                    return ExitCode::from(2);
                }
            }
        }
        if let Some(i) = args.iter().position(|a| a == "--table") {
            for a in args[i + 1..].iter().take_while(|a| !a.starts_with('-')) {
                let Some((name, file)) = a.split_once('=') else {
                    eprintln!("--table expects name=file.csv, got {a}");
                    return ExitCode::from(2);
                };
                match std::fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|t| table_from_csv(name, &t)) {
                    Ok(t) => {
                        f.tables.retain(|x| x.name != t.name);
                        f.tables.push(t);
                    }
                    Err(e) => {
                        eprintln!("{file}: {e}");
                        return ExitCode::from(2);
                    }
                }
            }
        }
        if let Err(e) = f.save(&out) {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
        println!("wrote {out}");
    }
    if sol.converged {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Numeric CSV (comma separated, header `name [unit]` or `name`) as a project table.
fn table_from_csv(name: &str, text: &str) -> Result<Table, String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let head = lines.next().ok_or("empty CSV")?;
    let columns: Vec<(String, String)> = head
        .split(',')
        .map(|c| {
            let c = c.trim();
            match (c.find('['), c.ends_with(']')) {
                (Some(i), true) => (c[..i].trim().to_string(), c[i + 1..c.len() - 1].trim().to_string()),
                _ => (c.to_string(), String::new()),
            }
        })
        .collect();
    let mut data = Vec::new();
    let mut rows = 0;
    for (n, l) in lines.enumerate() {
        let vals: Vec<&str> = l.split(',').collect();
        if vals.len() != columns.len() {
            return Err(format!("row {} has {} values, header has {}", n + 2, vals.len(), columns.len()));
        }
        for v in vals {
            let v = v.trim();
            data.push(if v.is_empty() || v.eq_ignore_ascii_case("nan") { f64::NAN } else { v.parse().map_err(|_| format!("row {}: '{v}' is not a number", n + 2))? });
        }
        rows += 1;
    }
    Ok(Table { name: name.to_string(), columns, rows, data })
}

fn info(path: &str, raw: &[u8]) -> ExitCode {
    let f = match DfFile::from_bytes(raw) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(1);
        }
    };
    println!("{path}: DeltaF Studio project, format {}.{}, {} bytes", raw[8], raw[10], raw.len());
    println!("  generator {}, created {}, modified {}", f.meta.generator, f.meta.created, f.meta.modified);
    println!("  {:<6} {:<28} {:>12} {:>12}", "tag", "name", "stored", "raw");
    for s in DfFile::inspect(raw).unwrap_or_default() {
        println!("  {:<6} {:<28} {:>12} {:>12}{}", s.tag, s.name, s.stored, s.raw, if s.compressed { "  zlib" } else { "" });
    }
    ExitCode::SUCCESS
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn unpack(f: DfFile, dir: &str) -> ExitCode {
    let root = std::path::Path::new(dir);
    let write = |rel: &str, data: &[u8]| -> Result<(), String> {
        // refuse names that would escape the target folder
        let relp = std::path::Path::new(rel);
        if relp.is_absolute() || relp.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(format!("unsafe file name {rel:?}"));
        }
        let p = root.join(relp);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, data).map_err(|e| format!("{}: {e}", p.display()))?;
        println!("  {}", p.display());
        Ok(())
    };
    let mut todo: Vec<(String, Vec<u8>)> = vec![("script.dfs".into(), f.script.clone().into_bytes())];
    if let Some(p) = &f.project {
        todo.push(("project.json".into(), p.clone().into_bytes()));
    }
    if let Some(r) = &f.results {
        let mut s = String::from("variable,value,unit,known\n");
        for i in 0..r.names.len() {
            s += &format!("{},{},{},{}\n", csv_field(&r.names[i]), r.values[i], csv_field(&r.units[i]), r.known[i]);
        }
        todo.push(("results.csv".into(), s.into_bytes()));
    }
    for t in &f.tables {
        let n = t.columns.len();
        let mut s = t.columns.iter().map(|(c, u)| csv_field(&if u.is_empty() { c.clone() } else { format!("{c} [{u}]") })).collect::<Vec<_>>().join(",") + "\n";
        for r in 0..t.rows {
            s += &(t.data[r * n..(r + 1) * n].iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",") + "\n");
        }
        todo.push((format!("tables/{}.csv", t.name), s.into_bytes()));
    }
    for a in &f.files {
        todo.push((format!("files/{}", a.name), a.data.clone()));
    }
    for (rel, data) in todo {
        if let Err(e) = write(&rel, &data) {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

/// Natural ordering so that T[2] < T[10].
fn natord(a: &str, b: &str) -> std::cmp::Ordering {
    let key = |s: &str| {
        let mut out: Vec<(String, i64)> = Vec::new();
        let mut txt = String::new();
        let mut num = String::new();
        for c in s.chars() {
            if c.is_ascii_digit() {
                num.push(c);
            } else {
                if !num.is_empty() {
                    out.push((std::mem::take(&mut txt), num.parse().unwrap_or(0)));
                    num.clear();
                }
                txt.push(c);
            }
        }
        out.push((txt, num.parse().unwrap_or(-1)));
        out
    };
    key(a).cmp(&key(b))
}
