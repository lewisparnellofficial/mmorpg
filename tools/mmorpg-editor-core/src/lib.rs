#![forbid(unsafe_code)]

use std::fmt;

/// The largest number of height samples accepted by this spike.
pub const MAX_SAMPLE_COUNT: usize = 16 * 1024 * 1024;

const SOURCE_HEADER: &str = "MMORPG_EDITOR_TERRAIN 1";
const RAISE_LOWER_STEP: f32 = 0.1;

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn unit(value: f32) -> f32 {
    finite_or_zero(value).clamp(0.0, 1.0)
}

fn signed_unit(value: f32) -> f32 {
    finite_or_zero(value).clamp(-1.0, 1.0)
}

/// Device-neutral tablet state with normalized numeric fields.
///
/// `pressure` and `rotation` are in `0.0..=1.0`; rotation is measured in
/// turns, where `0.0` and `1.0` both represent the zero-degree orientation.
/// `tilt_x` and `tilt_y` are in `-1.0..=1.0`. Native GUI integrations should
/// normalize their device-specific ranges before constructing this value, or
/// use the constructor, which clamps malformed input defensively.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormalizedTabletSample {
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub rotation: f32,
    pub eraser: bool,
    pub proximity: bool,
}

impl NormalizedTabletSample {
    pub fn new(
        pressure: f32,
        tilt_x: f32,
        tilt_y: f32,
        rotation: f32,
        eraser: bool,
        proximity: bool,
    ) -> Self {
        Self {
            pressure: unit(pressure),
            tilt_x: signed_unit(tilt_x),
            tilt_y: signed_unit(tilt_y),
            rotation: unit(rotation),
            eraser,
            proximity,
        }
    }

    pub const fn neutral() -> Self {
        Self {
            pressure: 0.0,
            tilt_x: 0.0,
            tilt_y: 0.0,
            rotation: 0.0,
            eraser: false,
            proximity: false,
        }
    }
}

/// A tablet sample located in heightmap sample coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabletPoint {
    pub x: f32,
    pub y: f32,
    pub sample: NormalizedTabletSample,
}

impl TabletPoint {
    pub fn new(x: f32, y: f32, sample: NormalizedTabletSample) -> Self {
        Self { x, y, sample }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EditorError {
    InvalidDimensions { width: usize, height: usize },
    SampleCountTooLarge { count: usize, maximum: usize },
    InvalidBounds { minimum: f32, maximum: f32 },
    SampleCountMismatch { expected: usize, actual: usize },
    InvalidBrushRadius(f32),
    InvalidBrushStrength(f32),
    InvalidSource(&'static str),
    InvalidSourceValue(String),
}

impl fmt::Display for EditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => {
                write!(
                    formatter,
                    "heightmap dimensions must be non-zero, got {width}x{height}"
                )
            }
            Self::SampleCountTooLarge { count, maximum } => {
                write!(
                    formatter,
                    "heightmap has {count} samples; maximum is {maximum}"
                )
            }
            Self::InvalidBounds { minimum, maximum } => {
                write!(formatter, "invalid height bounds {minimum}..{maximum}")
            }
            Self::SampleCountMismatch { expected, actual } => {
                write!(formatter, "expected {expected} samples, got {actual}")
            }
            Self::InvalidBrushRadius(radius) => {
                write!(
                    formatter,
                    "brush radius must be finite and positive, got {radius}"
                )
            }
            Self::InvalidBrushStrength(strength) => {
                write!(
                    formatter,
                    "brush strength must be finite and in 0..=1, got {strength}"
                )
            }
            Self::InvalidSource(reason) => write!(formatter, "invalid terrain source: {reason}"),
            Self::InvalidSourceValue(value) => {
                write!(formatter, "invalid terrain source value: {value}")
            }
        }
    }
}

impl std::error::Error for EditorError {}

/// A bounded row-major heightmap.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightMap {
    width: usize,
    height: usize,
    minimum: f32,
    maximum: f32,
    samples: Vec<f32>,
}

impl HeightMap {
    pub fn new(
        width: usize,
        height: usize,
        minimum: f32,
        maximum: f32,
        initial: f32,
    ) -> Result<Self, EditorError> {
        let count = checked_sample_count(width, height)?;
        validate_bounds(minimum, maximum)?;
        let mut map = Self {
            width,
            height,
            minimum,
            maximum,
            samples: vec![minimum; count],
        };
        let initial = map.clamp(initial);
        map.samples.fill(initial);
        Ok(map)
    }

    pub fn from_samples(
        width: usize,
        height: usize,
        minimum: f32,
        maximum: f32,
        samples: Vec<f32>,
    ) -> Result<Self, EditorError> {
        let expected = checked_sample_count(width, height)?;
        validate_bounds(minimum, maximum)?;
        if samples.len() != expected {
            return Err(EditorError::SampleCountMismatch {
                expected,
                actual: samples.len(),
            });
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(EditorError::InvalidSource("samples must be finite"));
        }
        let samples = samples
            .into_iter()
            .map(|sample| sample.clamp(minimum, maximum))
            .collect();
        Ok(Self {
            width,
            height,
            minimum,
            maximum,
            samples,
        })
    }

    pub const fn width(&self) -> usize {
        self.width
    }

    pub const fn height(&self) -> usize {
        self.height
    }

    pub const fn minimum(&self) -> f32 {
        self.minimum
    }

    pub const fn maximum(&self) -> f32 {
        self.maximum
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn sample(&self, x: usize, y: usize) -> Option<f32> {
        self.index(x, y).map(|index| self.samples[index])
    }

    pub fn set_sample(&mut self, x: usize, y: usize, value: f32) -> bool {
        let Some(index) = self.index(x, y) else {
            return false;
        };
        self.samples[index] = self.clamp(value);
        true
    }

    fn index(&self, x: usize, y: usize) -> Option<usize> {
        (x < self.width && y < self.height).then_some(y * self.width + x)
    }

    fn clamp(&self, value: f32) -> f32 {
        finite_or_zero(value).clamp(self.minimum, self.maximum)
    }
}

fn checked_sample_count(width: usize, height: usize) -> Result<usize, EditorError> {
    if width == 0 || height == 0 {
        return Err(EditorError::InvalidDimensions { width, height });
    }
    let count = width
        .checked_mul(height)
        .ok_or(EditorError::SampleCountTooLarge {
            count: usize::MAX,
            maximum: MAX_SAMPLE_COUNT,
        })?;
    if count > MAX_SAMPLE_COUNT {
        return Err(EditorError::SampleCountTooLarge {
            count,
            maximum: MAX_SAMPLE_COUNT,
        });
    }
    Ok(count)
}

fn validate_bounds(minimum: f32, maximum: f32) -> Result<(), EditorError> {
    if !minimum.is_finite() || !maximum.is_finite() || minimum > maximum {
        return Err(EditorError::InvalidBounds { minimum, maximum });
    }
    Ok(())
}

/// A terrain source document. The document is deliberately independent from
/// the editor history, allowing it to be loaded, saved, or passed to another
/// tool without carrying transient undo state.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainDocument {
    heightmap: HeightMap,
}

impl TerrainDocument {
    pub fn new(heightmap: HeightMap) -> Self {
        Self { heightmap }
    }

    pub fn heightmap(&self) -> &HeightMap {
        &self.heightmap
    }

    pub fn heightmap_mut(&mut self) -> &mut HeightMap {
        &mut self.heightmap
    }

    pub fn to_source(&self) -> String {
        let mut source = String::new();
        source.push_str(SOURCE_HEADER);
        source.push('\n');
        source.push_str(&format!("width {}\n", self.heightmap.width));
        source.push_str(&format!("height {}\n", self.heightmap.height));
        source.push_str(&format!(
            "bounds {:?} {:?}\n",
            self.heightmap.minimum, self.heightmap.maximum
        ));
        source.push_str("samples\n");
        for sample in &self.heightmap.samples {
            source.push_str(&format!("{:?}\n", sample));
        }
        source.push_str("end\n");
        source
    }

    pub fn from_source(source: &str) -> Result<Self, EditorError> {
        let lines: Vec<&str> = source.lines().collect();
        if lines.len() < 7 || lines[0] != SOURCE_HEADER {
            return Err(EditorError::InvalidSource("missing or unknown header"));
        }
        let width = parse_keyed_usize(lines[1], "width")?;
        let height = parse_keyed_usize(lines[2], "height")?;
        let bounds = parse_bounds(lines[3])?;
        let expected = checked_sample_count(width, height)?;
        if lines[4] != "samples" {
            return Err(EditorError::InvalidSource("expected samples section"));
        }
        let end_index = 5 + expected;
        if lines.len() != end_index + 1 || lines[end_index] != "end" {
            return Err(EditorError::InvalidSource(
                "sample count or end marker does not match dimensions",
            ));
        }
        let mut samples = Vec::with_capacity(expected);
        for line in &lines[5..end_index] {
            let value = line
                .parse::<f32>()
                .map_err(|_| EditorError::InvalidSourceValue((*line).to_owned()))?;
            if !value.is_finite() {
                return Err(EditorError::InvalidSource("samples must be finite"));
            }
            samples.push(value);
        }
        Ok(Self::new(HeightMap::from_samples(
            width, height, bounds.0, bounds.1, samples,
        )?))
    }
}

fn parse_keyed_usize(line: &str, key: &'static str) -> Result<usize, EditorError> {
    let mut fields = line.split_whitespace();
    if fields.next() != Some(key) {
        return Err(EditorError::InvalidSource("unexpected document field"));
    }
    let value = fields
        .next()
        .ok_or(EditorError::InvalidSource("missing document field value"))?;
    if fields.next().is_some() {
        return Err(EditorError::InvalidSource("extra document field data"));
    }
    value
        .parse()
        .map_err(|_| EditorError::InvalidSourceValue(value.to_owned()))
}

fn parse_bounds(line: &str) -> Result<(f32, f32), EditorError> {
    let mut fields = line.split_whitespace();
    if fields.next() != Some("bounds") {
        return Err(EditorError::InvalidSource("missing bounds"));
    }
    let minimum = fields
        .next()
        .ok_or(EditorError::InvalidSource("missing minimum bound"))?;
    let maximum = fields
        .next()
        .ok_or(EditorError::InvalidSource("missing maximum bound"))?;
    if fields.next().is_some() {
        return Err(EditorError::InvalidSource("extra bounds data"));
    }
    let minimum = minimum
        .parse::<f32>()
        .map_err(|_| EditorError::InvalidSourceValue(minimum.to_owned()))?;
    let maximum = maximum
        .parse::<f32>()
        .map_err(|_| EditorError::InvalidSourceValue(maximum.to_owned()))?;
    validate_bounds(minimum, maximum)?;
    Ok((minimum, maximum))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrushOperation {
    Raise,
    Lower,
    Smooth,
}

/// Parameters shared by every point in one brush stroke.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrushSettings {
    pub radius: f32,
    pub strength: f32,
    pub operation: BrushOperation,
}

impl BrushSettings {
    pub fn new(radius: f32, strength: f32, operation: BrushOperation) -> Result<Self, EditorError> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(EditorError::InvalidBrushRadius(radius));
        }
        if !strength.is_finite() || !(0.0..=1.0).contains(&strength) {
            return Err(EditorError::InvalidBrushStrength(strength));
        }
        Ok(Self {
            radius,
            strength,
            operation,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrokeOutcome {
    pub changed_samples: usize,
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    before: Vec<f32>,
    after: Vec<f32>,
}

/// Stateful terrain editing session with stroke-granularity undo/redo.
#[derive(Clone, Debug)]
pub struct TerrainEditor {
    document: TerrainDocument,
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
}

impl TerrainEditor {
    pub fn new(document: TerrainDocument) -> Self {
        Self {
            document,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn document(&self) -> &TerrainDocument {
        &self.document
    }

    pub fn document_mut(&mut self) -> &mut TerrainDocument {
        &mut self.document
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn apply_stroke(
        &mut self,
        points: &[TabletPoint],
        brush: BrushSettings,
    ) -> Result<StrokeOutcome, EditorError> {
        // Validate before capturing history so rejected input cannot change
        // the editor or create an empty undo entry.
        BrushSettings::new(brush.radius, brush.strength, brush.operation)?;
        let before = self.document.heightmap.samples.clone();
        for point in points {
            self.apply_dab(*point, brush);
        }
        let changed_samples = before
            .iter()
            .zip(self.document.heightmap.samples.iter())
            .filter(|(before, after)| before != after)
            .count();
        if changed_samples > 0 {
            self.undo_stack.push(HistoryEntry {
                before,
                after: self.document.heightmap.samples.clone(),
            });
            self.redo_stack.clear();
        }
        Ok(StrokeOutcome { changed_samples })
    }

    pub fn undo(&mut self) -> bool {
        let Some(entry) = self.undo_stack.pop() else {
            return false;
        };
        self.document.heightmap.samples = entry.before.clone();
        self.redo_stack.push(entry);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(entry) = self.redo_stack.pop() else {
            return false;
        };
        self.document.heightmap.samples = entry.after.clone();
        self.undo_stack.push(entry);
        true
    }

    fn apply_dab(&mut self, point: TabletPoint, brush: BrushSettings) {
        if !point.sample.proximity || !point.x.is_finite() || !point.y.is_finite() {
            return;
        }
        let map = &mut self.document.heightmap;
        let radius = brush.radius;
        let min_x = (point.x - radius).floor().max(0.0) as usize;
        let min_y = (point.y - radius).floor().max(0.0) as usize;
        let max_x = (point.x + radius).ceil().min((map.width - 1) as f32) as usize;
        let max_y = (point.y + radius).ceil().min((map.height - 1) as f32) as usize;
        if min_x > max_x || min_y > max_y {
            return;
        }

        let average = if brush.operation == BrushOperation::Smooth {
            Some(neighborhood_average(
                map,
                point.x,
                point.y,
                radius,
                DabBounds {
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                },
            ))
        } else {
            None
        };
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as f32 - point.x;
                let dy = y as f32 - point.y;
                let distance = (dx * dx + dy * dy).sqrt();
                if distance > radius {
                    continue;
                }
                let normalized_distance = (distance / radius).clamp(0.0, 1.0);
                let linear_falloff = 1.0 - normalized_distance;
                let falloff = linear_falloff * linear_falloff * (3.0 - 2.0 * linear_falloff);
                let effect = brush.strength * point.sample.pressure * falloff;
                let Some(index) = map.index(x, y) else {
                    continue;
                };
                let old = map.samples[index];
                let next = match (brush.operation, average) {
                    (BrushOperation::Raise, _) => {
                        let direction = if point.sample.eraser { -1.0 } else { 1.0 };
                        old + direction * effect * (map.maximum - map.minimum) * RAISE_LOWER_STEP
                    }
                    (BrushOperation::Lower, _) => {
                        let direction = if point.sample.eraser { 1.0 } else { -1.0 };
                        old + direction * effect * (map.maximum - map.minimum) * RAISE_LOWER_STEP
                    }
                    (BrushOperation::Smooth, Some(target)) => old + (target - old) * effect,
                    (BrushOperation::Smooth, None) => old,
                };
                map.samples[index] = map.clamp(next);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct DabBounds {
    min_x: usize,
    max_x: usize,
    min_y: usize,
    max_y: usize,
}

fn neighborhood_average(
    map: &HeightMap,
    center_x: f32,
    center_y: f32,
    radius: f32,
    bounds: DabBounds,
) -> f32 {
    let mut weighted_sum = 0.0;
    let mut total_weight = 0.0;
    for y in bounds.min_y..=bounds.max_y {
        for x in bounds.min_x..=bounds.max_x {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > radius {
                continue;
            }
            let linear_falloff = 1.0 - (distance / radius).clamp(0.0, 1.0);
            let weight = linear_falloff * linear_falloff * (3.0 - 2.0 * linear_falloff);
            if let Some(index) = map.index(x, y) {
                weighted_sum += map.samples[index] * weight;
                total_weight += weight;
            }
        }
    }
    if total_weight > 0.0 {
        weighted_sum / total_weight
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(pressure: f32) -> NormalizedTabletSample {
        NormalizedTabletSample::new(pressure, 0.0, 0.0, 0.0, false, true)
    }

    fn point(x: f32, y: f32, pressure: f32) -> TabletPoint {
        TabletPoint::new(x, y, sample(pressure))
    }

    fn editor(initial: f32) -> TerrainEditor {
        let map = HeightMap::new(5, 5, 0.0, 1.0, initial).expect("valid test map");
        TerrainEditor::new(TerrainDocument::new(map))
    }

    #[test]
    fn normalizes_tablet_fields_and_heightmap_values() {
        let tablet = NormalizedTabletSample::new(2.0, -2.0, 0.25, -1.0, true, true);
        assert_eq!(tablet.pressure, 1.0);
        assert_eq!(tablet.tilt_x, -1.0);
        assert_eq!(tablet.tilt_y, 0.25);
        assert_eq!(tablet.rotation, 0.0);
        assert!(tablet.eraser);

        let map = HeightMap::new(2, 1, -1.0, 1.0, 5.0).expect("valid test map");
        assert_eq!(map.samples(), &[1.0, 1.0]);
        let mut map = map;
        assert!(map.set_sample(0, 0, -5.0));
        assert_eq!(map.sample(0, 0), Some(-1.0));
        assert!(!map.set_sample(2, 0, 0.0));
    }

    #[test]
    fn pressure_controls_raise_amount_and_proximity_controls_editing() {
        let brush = BrushSettings::new(1.5, 1.0, BrushOperation::Raise).unwrap();
        let mut no_pressure = editor(0.5);
        let mut full_pressure = editor(0.5);
        let mut out_of_proximity = editor(0.5);
        no_pressure
            .apply_stroke(&[point(2.0, 2.0, 0.0)], brush)
            .unwrap();
        full_pressure
            .apply_stroke(&[point(2.0, 2.0, 1.0)], brush)
            .unwrap();
        let inactive = TabletPoint::new(
            2.0,
            2.0,
            NormalizedTabletSample::new(1.0, 0.0, 0.0, 0.0, false, false),
        );
        out_of_proximity.apply_stroke(&[inactive], brush).unwrap();

        assert_eq!(no_pressure.document().heightmap().sample(2, 2), Some(0.5));
        assert!(full_pressure.document().heightmap().sample(2, 2).unwrap() > 0.5);
        assert_eq!(
            out_of_proximity.document().heightmap().sample(2, 2),
            Some(0.5)
        );
        assert!(!no_pressure.can_undo());
    }

    #[test]
    fn undo_and_redo_restore_complete_strokes() {
        let brush = BrushSettings::new(1.0, 1.0, BrushOperation::Raise).unwrap();
        let mut editor = editor(0.0);
        let initial = editor.document().heightmap().samples().to_vec();
        editor.apply_stroke(&[point(2.0, 2.0, 1.0)], brush).unwrap();
        let after_first = editor.document().heightmap().samples().to_vec();
        editor.apply_stroke(&[point(0.0, 0.0, 1.0)], brush).unwrap();
        let after_second = editor.document().heightmap().samples().to_vec();
        assert!(editor.undo());
        assert_eq!(editor.document().heightmap().samples(), after_first);
        assert!(editor.undo());
        assert_eq!(editor.document().heightmap().samples(), initial);
        assert!(editor.redo());
        assert_eq!(editor.document().heightmap().samples(), after_first);
        assert!(editor.redo());
        assert_eq!(editor.document().heightmap().samples(), after_second);
        assert!(!editor.redo());
    }

    #[test]
    fn source_round_trip_is_deterministic() {
        let map = HeightMap::from_samples(3, 2, -2.0, 4.0, vec![-2.0, 0.125, 4.0, 1.5, 2.25, -1.0])
            .unwrap();
        let document = TerrainDocument::new(map);
        let source = document.to_source();
        assert_eq!(source, document.to_source());
        let loaded = TerrainDocument::from_source(&source).unwrap();
        assert_eq!(loaded, document);
        assert_eq!(loaded.to_source(), source);
    }

    #[test]
    fn smooth_brush_moves_a_peak_toward_neighbors() {
        let map = HeightMap::from_samples(3, 1, 0.0, 1.0, vec![0.0, 1.0, 0.0]).unwrap();
        let mut editor = TerrainEditor::new(TerrainDocument::new(map));
        let brush = BrushSettings::new(1.1, 1.0, BrushOperation::Smooth).unwrap();
        editor.apply_stroke(&[point(1.0, 0.0, 1.0)], brush).unwrap();
        assert!(editor.document().heightmap().sample(1, 0).unwrap() < 1.0);
        assert!(editor.document().heightmap().sample(0, 0).unwrap() > 0.0);
    }
}
