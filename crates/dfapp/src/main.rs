//! DF Studio desktop application.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analysis;
mod app;
mod components;
mod cycle;
mod deps;
mod editor;
mod find;
mod format;
mod icons;
mod markdown;
mod pdf;
mod plots;
mod project;
mod studies_ui;
mod templates;
mod units_ui;

/// `dfstudio --report project.df report.pdf`: solve a project and write its
/// cycle-diagram report without opening a window.
fn report(args: &[String]) -> std::process::ExitCode {
    use std::process::ExitCode;
    let (Some(src), Some(out)) = (args.get(2), args.get(3)) else {
        eprintln!("usage: dfstudio --report project.df report.pdf");
        return ExitCode::from(2);
    };
    let f = match dfcore::dffile::DfFile::load(src) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{src}: {e}");
            return ExitCode::from(1);
        }
    };
    let mut s = app::Settings::default();
    project::apply_df(f, &mut s);
    let a = analysis::run(analysis::Request {
        gen: 1,
        text: s.text.clone(),
        opts: dfcore::CompileOptions { unit_system: Some(s.unit_system.clone()), check_fluids: true },
        solve: true,
        solve_opts: Default::default(),
        warm: None,
    });
    let converged = a.solution.as_ref().is_some_and(|x| x.converged);
    let bytes = cycle::report_pdf(&s.cycle, &cycle::CycleInputs { analysis: &a, display_units: &s.display_units, script: &s.text });
    if let Err(e) = std::fs::write(out, bytes) {
        eprintln!("{out}: {e}");
        return ExitCode::from(1);
    }
    println!("wrote {out}{}", if converged { "" } else { " (solution NOT converged)" });
    if converged {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// `dfstudio --write-examples DIR`: write the built-in templates as example
/// projects (equations, solution and cycle diagram), for the installers.
fn write_examples(args: &[String]) -> i32 {
    let Some(dir) = args.get(2) else {
        eprintln!("usage: dfstudio --write-examples DIR");
        return 2;
    };
    let dir = std::path::Path::new(dir);
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("{}: {e}", dir.display());
        return 1;
    }
    let mut code = 0;
    for t in &templates::ALL {
        let mut s = app::Settings::default();
        s.text = t.script.to_string();
        s.cycle = (t.diagram)();
        let a = analysis::run(analysis::Request {
            gen: 1,
            text: s.text.clone(),
            opts: dfcore::CompileOptions { unit_system: Some(s.unit_system.clone()), check_fluids: true },
            solve: true,
            solve_opts: Default::default(),
            warm: None,
        });
        if !a.solution.as_ref().is_some_and(|x| x.converged) {
            eprintln!("{}: not converged", t.name);
            code = 1;
        }
        let p = dir.join(format!("{}.df", t.name));
        match project::to_df(&s, Some(&a), &[], &Default::default()).save(&p) {
            Ok(()) => println!("wrote {}", p.display()),
            Err(e) => {
                eprintln!("{}: {e}", p.display());
                code = 1;
            }
        }
    }
    code
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--write-examples") {
        std::process::exit(write_examples(&args));
    }
    if args.get(1).map(String::as_str) == Some("--report") {
        std::process::exit(match report(&args) {
            c if c == std::process::ExitCode::SUCCESS => 0,
            _ => 1,
        });
    }
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("DF Studio")
            .with_app_id("dfstudio")
            .with_icon(eframe::egui::IconData { rgba: include_bytes!("../../../packaging/icons/icon_64.rgba").to_vec(), width: 64, height: 64 })
            .with_inner_size([1400.0, 860.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    let file = std::env::args().nth(1);
    eframe::run_native("DF Studio", opts, Box::new(move |cc| Ok(Box::new(app::DfApp::new(cc, file)))))
}
