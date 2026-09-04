use std::env;
use std::fs;
use std::path::PathBuf;

use mmorpg_editor_core::{
    BrushOperation, BrushSettings, HeightMap, NormalizedTabletSample, TabletPoint, TerrainDocument,
    TerrainEditor,
};

fn main() {
    let output = match parse_output_path() {
        Ok(output) => output,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let map = HeightMap::new(32, 32, 0.0, 1.0, 0.0)
        .expect("the editor demo dimensions and bounds must be valid");
    let mut editor = TerrainEditor::new(TerrainDocument::new(map));
    let brush = BrushSettings::new(5.0, 0.8, BrushOperation::Raise)
        .expect("the editor demo brush must be valid");
    let tablet = NormalizedTabletSample::new(0.75, 0.0, 0.0, 0.0, false, true);
    let outcome = editor
        .apply_stroke(&[TabletPoint::new(16.0, 16.0, tablet)], brush)
        .expect("the editor demo stroke must be valid");
    let source = editor.document().to_source();

    if let Some(path) = output {
        fs::write(&path, source).unwrap_or_else(|error| {
            eprintln!("could not write {}: {error}", path.display());
            std::process::exit(1);
        });
        println!(
            "editor spike: wrote {} (changed_samples={}, undo_available={})",
            path.display(),
            outcome.changed_samples,
            editor.can_undo()
        );
    } else {
        println!(
            "editor spike: heightmap={}x{} changed_samples={} source_bytes={}",
            editor.document().heightmap().width(),
            editor.document().heightmap().height(),
            outcome.changed_samples,
            source.len()
        );
        println!("use --output <path> to write the deterministic terrain source");
    }
}

fn parse_output_path() -> Result<Option<PathBuf>, String> {
    let mut arguments = env::args().skip(1);
    let mut output = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                println!("Usage: mmorpg-editor-core [--output <path>]");
                println!(
                    "Run a small terrain-editing spike and optionally save its source document."
                );
                std::process::exit(0);
            }
            "--output" => {
                let path = arguments
                    .next()
                    .ok_or_else(|| "--output requires a path".to_owned())?;
                output = Some(PathBuf::from(path));
            }
            unknown => return Err(format!("unknown argument '{unknown}'; use --help")),
        }
    }
    Ok(output)
}
