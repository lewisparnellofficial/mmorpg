use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use mmorpg_editor_core::{
    BrushOperation, BrushSettings, CapturedStroke, HeightMap, InputSource, NativeTabletEvent,
    NativeTabletPhase, NormalizedTabletSample, TabletEventBridge, TabletPoint, TerrainDocument,
    TerrainEditor,
};

fn main() {
    if env::args().nth(1).as_deref() == Some("--bridge") {
        if let Err(message) = run_bridge() {
            eprintln!("editor bridge: {message}");
            std::process::exit(1);
        }
        return;
    }
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

fn run_bridge() -> Result<(), String> {
    let map = HeightMap::new(32, 32, 0.0, 1.0, 0.0)
        .map_err(|error| format!("cannot create bridge document: {error}"))?;
    let mut editor = TerrainEditor::new(TerrainDocument::new(map));
    let mut bridge = TabletEventBridge::new();
    let mut brush = BrushSettings::new(5.0, 0.8, BrushOperation::Raise)
        .map_err(|error| format!("cannot create bridge brush: {error}"))?;
    let mut last_capture: Option<CapturedStroke> = None;
    println!("ready");
    print_preview(&editor);
    io::stdout().flush().map_err(|error| error.to_string())?;

    for line in io::stdin().lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        let mut fields = line.split_whitespace();
        let command = fields.next().unwrap_or_default();
        let result = match command {
            "event" => {
                let event = parse_native_event(&mut fields)?;
                match bridge
                    .push(event)
                    .map_err(|error| format!("bridge rejected event: {error}"))?
                {
                    mmorpg_editor_core::TabletBridgeOutput::StrokeFinished(points) => {
                        let capture = CapturedStroke::from_points(points.clone())
                            .map_err(|error| format!("capture rejected stroke: {error}"))?;
                        let outcome = editor
                            .apply_stroke(&points, brush)
                            .map_err(|error| format!("terrain rejected stroke: {error}"))?;
                        last_capture = Some(capture);
                        print_preview(&editor);
                        format!(
                            "stroke-finished changed_samples={} undo={}",
                            outcome.changed_samples,
                            editor.can_undo()
                        )
                    }
                    output => format!("event {:?}", output),
                }
            }
            "brush" => {
                let operation = match fields.next() {
                    Some("raise") => BrushOperation::Raise,
                    Some("lower") => BrushOperation::Lower,
                    Some("smooth") => BrushOperation::Smooth,
                    Some(value) => return Err(format!("unknown brush operation '{value}'")),
                    None => return Err("brush requires raise, lower, or smooth".into()),
                };
                brush = BrushSettings::new(brush.radius, brush.strength, operation)
                    .map_err(|error| format!("invalid brush: {error}"))?;
                format!("brush {:?}", operation)
            }
            "undo" => {
                let result = format!("undo {}", editor.undo());
                print_preview(&editor);
                result
            }
            "redo" => {
                let result = format!("redo {}", editor.redo());
                print_preview(&editor);
                result
            }
            "save" => {
                let path = remaining_path(&mut fields, "save")?;
                atomic_write(Path::new(&path), editor.document().to_source().as_bytes())?;
                format!("saved {path}")
            }
            "open" => {
                let path = remaining_path(&mut fields, "open")?;
                let source = fs::read_to_string(&path)
                    .map_err(|error| format!("could not read {path}: {error}"))?;
                let document = TerrainDocument::from_source(&source)
                    .map_err(|error| format!("could not parse {path}: {error}"))?;
                editor = TerrainEditor::new(document);
                bridge = TabletEventBridge::new();
                last_capture = None;
                print_preview(&editor);
                format!("opened {path}")
            }
            "capture" => {
                let path = remaining_path(&mut fields, "capture")?;
                let capture = last_capture
                    .as_ref()
                    .ok_or_else(|| "no completed stroke is available".to_owned())?;
                atomic_write(Path::new(&path), capture.to_source().as_bytes())?;
                format!("captured {path}")
            }
            "replay" => {
                let path = remaining_path(&mut fields, "replay")?;
                let source = fs::read_to_string(&path)
                    .map_err(|error| format!("could not read {path}: {error}"))?;
                let capture = CapturedStroke::from_source(&source)
                    .map_err(|error| format!("could not parse {path}: {error}"))?;
                let first_timestamp = capture
                    .points()
                    .first()
                    .map_or(0, |point| point.timestamp_ns);
                bridge
                    .push(
                        NativeTabletEvent::from_qt(
                            NativeTabletPhase::ProximityEnter,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            false,
                        )
                        .map_err(|error| format!("replay proximity failed: {error}"))?
                        .with_timestamp(first_timestamp.saturating_sub(1)),
                    )
                    .map_err(|error| format!("replay proximity failed: {error}"))?;
                let mut changed_samples = 0;
                for (index, point) in capture.points().iter().enumerate() {
                    let phase = if index == 0 {
                        NativeTabletPhase::Press
                    } else if index + 1 == capture.points().len() {
                        NativeTabletPhase::Release
                    } else {
                        NativeTabletPhase::Move
                    };
                    if let mmorpg_editor_core::TabletBridgeOutput::StrokeFinished(points) = bridge
                        .push(native_event_from_point(*point, phase)?)
                        .map_err(|error| format!("replay event failed: {error}"))?
                    {
                        changed_samples = editor
                            .apply_stroke(&points, brush)
                            .map_err(|error| format!("replay terrain failed: {error}"))?
                            .changed_samples;
                    }
                }
                last_capture = Some(capture);
                print_preview(&editor);
                format!("replayed {path} changed_samples={changed_samples}")
            }
            "state" => format!(
                "state undo={} redo={} source_bytes={}",
                editor.can_undo(),
                editor.can_redo(),
                editor.document().to_source().len()
            ),
            "quit" => break,
            "" => continue,
            value => return Err(format!("unknown bridge command '{value}'")),
        };
        println!("{result}");
        io::stdout().flush().map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn print_preview(editor: &TerrainEditor) {
    let map = editor.document().heightmap();
    print!("preview {} {}", map.width(), map.height());
    for sample in map.samples() {
        print!(" {:.6}", sample);
    }
    println!();
}

fn native_event_from_point(
    point: TabletPoint,
    phase: NativeTabletPhase,
) -> Result<NativeTabletEvent, String> {
    let sample: NormalizedTabletSample = point.sample;
    let event = match point.source {
        InputSource::Mouse => {
            NativeTabletEvent::from_mouse(phase, point.x, point.y, point.timestamp_ns)
        }
        InputSource::Pen | InputSource::Eraser => NativeTabletEvent::from_qt(
            phase,
            point.x,
            point.y,
            sample.pressure,
            sample.tilt_x * 60.0,
            sample.tilt_y * 60.0,
            sample.rotation * 360.0,
            point.source == InputSource::Eraser,
        ),
    }
    .map_err(|error| error.to_string())?;
    Ok(event.with_timestamp(point.timestamp_ns))
}

fn parse_native_event<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
) -> Result<NativeTabletEvent, String> {
    let phase = match fields.next().ok_or("event requires a phase")? {
        "proximity-enter" => NativeTabletPhase::ProximityEnter,
        "press" => NativeTabletPhase::Press,
        "move" => NativeTabletPhase::Move,
        "release" => NativeTabletPhase::Release,
        "cancel" => NativeTabletPhase::Cancel,
        "proximity-leave" => NativeTabletPhase::ProximityLeave,
        value => return Err(format!("unknown event phase '{value}'")),
    };
    let x = parse_f32(fields.next(), "x")?;
    let y = parse_f32(fields.next(), "y")?;
    let pressure = parse_f32(fields.next(), "pressure")?;
    let source = match fields.next().ok_or("event requires a source")? {
        "pen" => InputSource::Pen,
        "eraser" => InputSource::Eraser,
        "mouse" => InputSource::Mouse,
        value => return Err(format!("unknown event source '{value}'")),
    };
    let tilt_x = parse_f32(fields.next(), "tilt_x")?;
    let tilt_y = parse_f32(fields.next(), "tilt_y")?;
    let rotation = parse_f32(fields.next(), "rotation")?;
    let timestamp_ns = fields
        .next()
        .ok_or("event requires a timestamp")?
        .parse::<u64>()
        .map_err(|_| "timestamp must be an unsigned integer".to_owned())?;
    let mut event = match source {
        InputSource::Mouse => NativeTabletEvent::from_mouse(phase, x, y, timestamp_ns),
        InputSource::Pen | InputSource::Eraser => NativeTabletEvent::from_qt(
            phase,
            x,
            y,
            pressure,
            tilt_x,
            tilt_y,
            rotation,
            source == InputSource::Eraser,
        ),
    }
    .map_err(|error| format!("invalid native event: {error}"))?;
    event.source = source;
    Ok(event.with_timestamp(timestamp_ns))
}

fn parse_f32(value: Option<&str>, field: &str) -> Result<f32, String> {
    value
        .ok_or_else(|| format!("event requires {field}"))?
        .parse::<f32>()
        .map_err(|_| format!("{field} must be a number"))
}

fn remaining_path<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
    command: &str,
) -> Result<String, String> {
    let path = fields.collect::<Vec<_>>().join(" ");
    if path.is_empty() {
        Err(format!("{command} requires a path"))
    } else {
        Ok(path)
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)
        .map_err(|error| format!("could not write temporary file: {error}"))?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("could not replace destination: {error}"));
    }
    Ok(())
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
