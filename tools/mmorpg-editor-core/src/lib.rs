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

/// The toolkit-neutral input device that produced a sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputSource {
    Pen,
    Eraser,
    Mouse,
}

/// A tablet sample located in heightmap sample coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabletPoint {
    pub x: f32,
    pub y: f32,
    pub sample: NormalizedTabletSample,
    pub source: InputSource,
    pub timestamp_ns: u64,
}

impl TabletPoint {
    pub fn new(x: f32, y: f32, sample: NormalizedTabletSample) -> Self {
        Self {
            x,
            y,
            sample,
            source: if sample.eraser {
                InputSource::Eraser
            } else {
                InputSource::Pen
            },
            timestamp_ns: 0,
        }
    }

    pub fn with_metadata(
        x: f32,
        y: f32,
        sample: NormalizedTabletSample,
        source: InputSource,
        timestamp_ns: u64,
    ) -> Self {
        Self {
            x,
            y,
            sample,
            source,
            timestamp_ns,
        }
    }
}

/// Native tablet phases that a GUI shell must translate into the editor
/// boundary. The enum intentionally matches the lifecycle rather than a
/// particular toolkit's event type, so a Qt, SDL, or future Linux adapter
/// can feed the same stroke state machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTabletPhase {
    ProximityEnter,
    Press,
    Move,
    Release,
    Cancel,
    ProximityLeave,
}

/// Raw axes as reported by a native tablet event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeTabletEvent {
    pub phase: NativeTabletPhase,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub tilt_x_degrees: f32,
    pub tilt_y_degrees: f32,
    pub rotation_degrees: f32,
    pub eraser: bool,
    pub source: InputSource,
    pub timestamp_ns: u64,
    pressure_min: f32,
    pressure_max: f32,
    tilt_limit_degrees: f32,
    rotation_period_degrees: f32,
}

impl NativeTabletEvent {
    /// Constructs an event using the ranges supplied by the native API.
    /// Pressure and rotation are normalized from their declared ranges;
    /// tilt is normalized symmetrically around zero.
    pub fn from_axes(
        phase: NativeTabletPhase,
        x: f32,
        y: f32,
        pressure: f32,
        pressure_min: f32,
        pressure_max: f32,
        tilt_x_degrees: f32,
        tilt_y_degrees: f32,
        tilt_limit_degrees: f32,
        rotation_degrees: f32,
        rotation_period_degrees: f32,
        eraser: bool,
    ) -> Result<Self, EditorError> {
        if !pressure_min.is_finite() || !pressure_max.is_finite() || pressure_min >= pressure_max {
            return Err(EditorError::InvalidTabletRange { field: "pressure" });
        }
        if !tilt_limit_degrees.is_finite() || tilt_limit_degrees <= 0.0 {
            return Err(EditorError::InvalidTabletRange { field: "tilt" });
        }
        if !rotation_period_degrees.is_finite() || rotation_period_degrees <= 0.0 {
            return Err(EditorError::InvalidTabletRange { field: "rotation" });
        }
        Ok(Self {
            phase,
            x,
            y,
            pressure,
            tilt_x_degrees,
            tilt_y_degrees,
            rotation_degrees,
            eraser,
            source: if eraser {
                InputSource::Eraser
            } else {
                InputSource::Pen
            },
            timestamp_ns: 0,
            pressure_min,
            pressure_max,
            tilt_limit_degrees,
            rotation_period_degrees,
        })
    }

    /// Qt's documented tablet axes use normalized pressure, +/-60-degree
    /// tilt, and a rotation measured in degrees around a 360-degree period.
    /// A Qt `QTabletEvent` callback can pass its values directly here.
    pub fn from_qt(
        phase: NativeTabletPhase,
        x: f32,
        y: f32,
        pressure: f32,
        tilt_x_degrees: f32,
        tilt_y_degrees: f32,
        rotation_degrees: f32,
        eraser: bool,
    ) -> Result<Self, EditorError> {
        Self::from_axes(
            phase,
            x,
            y,
            pressure,
            0.0,
            1.0,
            tilt_x_degrees,
            tilt_y_degrees,
            60.0,
            rotation_degrees,
            360.0,
            eraser,
        )
    }

    pub fn from_mouse(
        phase: NativeTabletPhase,
        x: f32,
        y: f32,
        timestamp_ns: u64,
    ) -> Result<Self, EditorError> {
        let mut event = Self::from_axes(
            phase, x, y, 1.0, 0.0, 1.0, 0.0, 0.0, 60.0, 0.0, 360.0, false,
        )?;
        event.source = InputSource::Mouse;
        event.timestamp_ns = timestamp_ns;
        Ok(event)
    }

    pub fn with_timestamp(mut self, timestamp_ns: u64) -> Self {
        self.timestamp_ns = timestamp_ns;
        self
    }

    /// Converts this native event to the device-neutral project sample.
    pub fn normalized_sample(self, proximity: bool) -> NormalizedTabletSample {
        let pressure = if self.pressure.is_finite() {
            (self.pressure - self.pressure_min) / (self.pressure_max - self.pressure_min)
        } else {
            0.0
        };
        let tilt_x = if self.tilt_x_degrees.is_finite() {
            self.tilt_x_degrees / self.tilt_limit_degrees
        } else {
            0.0
        };
        let tilt_y = if self.tilt_y_degrees.is_finite() {
            self.tilt_y_degrees / self.tilt_limit_degrees
        } else {
            0.0
        };
        let rotation = if self.rotation_degrees.is_finite() {
            self.rotation_degrees
                .rem_euclid(self.rotation_period_degrees)
                / self.rotation_period_degrees
        } else {
            0.0
        };
        NormalizedTabletSample::new(pressure, tilt_x, tilt_y, rotation, self.eraser, proximity)
    }
}

/// Result emitted by [`TabletEventBridge`] after consuming one native event.
#[derive(Clone, Debug, PartialEq)]
pub enum TabletBridgeOutput {
    Ignored,
    ProximityEntered,
    ProximityLeft,
    StrokeStarted(TabletPoint),
    StrokePoint(TabletPoint),
    StrokeFinished(Vec<TabletPoint>),
    StrokeCancelled,
}

/// Errors produced while converting and buffering native tablet events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TabletBridgeError {
    InvalidEventCoordinate,
    StrokeAlreadyActive,
    NoActiveStroke,
    StrokeTooLong { maximum: usize },
    NonMonotonicTimestamp { previous: u64, current: u64 },
}

impl fmt::Display for TabletBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEventCoordinate => {
                formatter.write_str("tablet event coordinates must be finite")
            }
            Self::StrokeAlreadyActive => formatter.write_str("tablet stroke is already active"),
            Self::NoActiveStroke => formatter.write_str("tablet stroke is not active"),
            Self::StrokeTooLong { maximum } => {
                write!(formatter, "tablet stroke exceeds the {maximum}-point limit")
            }
            Self::NonMonotonicTimestamp { previous, current } => write!(
                formatter,
                "tablet timestamp moved backward from {previous} to {current}"
            ),
        }
    }
}

impl std::error::Error for TabletBridgeError {}

/// Maximum native points retained for one in-progress stroke in this spike.
pub const MAX_STROKE_POINTS: usize = 8192;

/// Converts native tablet lifecycle events into bounded, device-neutral
/// points. It owns no GUI handles and does not apply gameplay or terrain
/// mutations; the caller submits a completed point list to
/// [`TerrainEditor::apply_stroke`].
#[derive(Clone, Debug)]
pub struct TabletEventBridge {
    in_proximity: bool,
    points: Vec<TabletPoint>,
    maximum_points: usize,
    last_timestamp_ns: Option<u64>,
}

impl Default for TabletEventBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl TabletEventBridge {
    pub fn new() -> Self {
        Self {
            in_proximity: false,
            points: Vec::new(),
            maximum_points: MAX_STROKE_POINTS,
            last_timestamp_ns: None,
        }
    }

    pub fn with_maximum_points(maximum_points: usize) -> Result<Self, TabletBridgeError> {
        if maximum_points == 0 || maximum_points > MAX_STROKE_POINTS {
            return Err(TabletBridgeError::StrokeTooLong {
                maximum: MAX_STROKE_POINTS,
            });
        }
        Ok(Self {
            in_proximity: false,
            points: Vec::new(),
            maximum_points,
            last_timestamp_ns: None,
        })
    }

    pub fn is_in_proximity(&self) -> bool {
        self.in_proximity
    }

    pub fn is_stroke_active(&self) -> bool {
        !self.points.is_empty()
    }

    /// Feeds one native event into the bounded stroke lifecycle.
    pub fn push(
        &mut self,
        event: NativeTabletEvent,
    ) -> Result<TabletBridgeOutput, TabletBridgeError> {
        if !event.x.is_finite() || !event.y.is_finite() {
            return Err(TabletBridgeError::InvalidEventCoordinate);
        }
        if let Some(previous) = self.last_timestamp_ns
            && event.timestamp_ns < previous
        {
            return Err(TabletBridgeError::NonMonotonicTimestamp {
                previous,
                current: event.timestamp_ns,
            });
        }
        self.last_timestamp_ns = Some(event.timestamp_ns);
        match event.phase {
            NativeTabletPhase::ProximityEnter => {
                self.in_proximity = true;
                Ok(TabletBridgeOutput::ProximityEntered)
            }
            NativeTabletPhase::ProximityLeave => {
                self.in_proximity = false;
                if self.points.is_empty() {
                    Ok(TabletBridgeOutput::ProximityLeft)
                } else {
                    self.points.clear();
                    Ok(TabletBridgeOutput::StrokeCancelled)
                }
            }
            NativeTabletPhase::Press => {
                if !self.points.is_empty() {
                    return Err(TabletBridgeError::StrokeAlreadyActive);
                }
                self.in_proximity = true;
                let point = self.point(event);
                self.points.push(point);
                Ok(TabletBridgeOutput::StrokeStarted(point))
            }
            NativeTabletPhase::Move => {
                if self.points.is_empty() {
                    return Ok(TabletBridgeOutput::Ignored);
                }
                self.push_point(event)
            }
            NativeTabletPhase::Release => {
                if self.points.is_empty() {
                    return Err(TabletBridgeError::NoActiveStroke);
                }
                self.push_point(event)?;
                self.in_proximity = true;
                Ok(TabletBridgeOutput::StrokeFinished(std::mem::take(
                    &mut self.points,
                )))
            }
            NativeTabletPhase::Cancel => {
                if self.points.is_empty() {
                    return Ok(TabletBridgeOutput::Ignored);
                }
                self.points.clear();
                Ok(TabletBridgeOutput::StrokeCancelled)
            }
        }
    }

    fn point(&self, event: NativeTabletEvent) -> TabletPoint {
        TabletPoint::with_metadata(
            event.x,
            event.y,
            event.normalized_sample(true),
            event.source,
            event.timestamp_ns,
        )
    }

    fn push_point(
        &mut self,
        event: NativeTabletEvent,
    ) -> Result<TabletBridgeOutput, TabletBridgeError> {
        if self.points.len() >= self.maximum_points {
            self.points.clear();
            return Err(TabletBridgeError::StrokeTooLong {
                maximum: self.maximum_points,
            });
        }
        let point = self.point(event);
        self.points.push(point);
        Ok(TabletBridgeOutput::StrokePoint(point))
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
    InvalidTabletRange { field: &'static str },
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
            Self::InvalidTabletRange { field } => {
                write!(formatter, "tablet {field} range is invalid")
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

    fn qt_event(phase: NativeTabletPhase, x: f32, y: f32, pressure: f32) -> NativeTabletEvent {
        NativeTabletEvent::from_qt(phase, x, y, pressure, 30.0, -15.0, 90.0, false)
            .expect("valid Qt-shaped tablet event")
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

    #[test]
    fn qt_axes_normalize_into_the_device_neutral_sample() {
        let event = qt_event(NativeTabletPhase::Move, 1.0, 2.0, 0.75);
        let sample = event.normalized_sample(true);
        assert_eq!(sample.pressure, 0.75);
        assert_eq!(sample.tilt_x, 0.5);
        assert_eq!(sample.tilt_y, -0.25);
        assert_eq!(sample.rotation, 0.25);
        assert!(sample.proximity);

        let wrapped = qt_event(NativeTabletPhase::Move, 1.0, 2.0, 0.75);
        let wrapped = NativeTabletEvent {
            rotation_degrees: 450.0,
            ..wrapped
        };
        assert_eq!(wrapped.normalized_sample(true).rotation, 0.25);
    }

    #[test]
    fn device_source_and_timestamp_survive_bridge_conversion() {
        let mut bridge = TabletEventBridge::new();
        bridge
            .push(NativeTabletEvent::from_mouse(NativeTabletPhase::Press, 4.0, 5.0, 10).unwrap())
            .unwrap();
        let Ok(TabletBridgeOutput::StrokePoint(point)) = bridge
            .push(NativeTabletEvent::from_mouse(NativeTabletPhase::Move, 5.0, 6.0, 11).unwrap())
        else {
            panic!("mouse move must produce a stroke point");
        };
        assert_eq!(point.source, InputSource::Mouse);
        assert_eq!(point.timestamp_ns, 11);
        assert_eq!(point.sample.pressure, 1.0);
    }

    #[test]
    fn bridge_rejects_backward_native_timestamps() {
        let mut bridge = TabletEventBridge::new();
        bridge
            .push(NativeTabletEvent::from_mouse(NativeTabletPhase::Press, 1.0, 1.0, 20).unwrap())
            .unwrap();
        assert_eq!(
            bridge.push(
                NativeTabletEvent::from_mouse(NativeTabletPhase::Move, 1.0, 2.0, 19).unwrap(),
            ),
            Err(TabletBridgeError::NonMonotonicTimestamp {
                previous: 20,
                current: 19,
            })
        );
        assert!(bridge.is_stroke_active());
    }

    #[test]
    fn tablet_bridge_preserves_a_qt_stroke_lifecycle_and_points() {
        let mut bridge = TabletEventBridge::new();
        assert_eq!(
            bridge.push(qt_event(NativeTabletPhase::ProximityEnter, 0.0, 0.0, 0.0)),
            Ok(TabletBridgeOutput::ProximityEntered)
        );
        assert_eq!(
            bridge.push(qt_event(NativeTabletPhase::Press, 2.0, 3.0, 0.5)),
            Ok(TabletBridgeOutput::StrokeStarted(TabletPoint::new(
                2.0,
                3.0,
                NormalizedTabletSample::new(0.5, 0.5, -0.25, 0.25, false, true),
            )))
        );
        assert!(bridge.is_stroke_active());
        assert!(matches!(
            bridge.push(qt_event(NativeTabletPhase::Move, 2.5, 3.5, 1.0)),
            Ok(TabletBridgeOutput::StrokePoint(_))
        ));
        let Ok(TabletBridgeOutput::StrokeFinished(points)) =
            bridge.push(qt_event(NativeTabletPhase::Release, 3.0, 4.0, 0.25))
        else {
            panic!("release must finish the active stroke");
        };
        assert_eq!(points.len(), 3);
        assert_eq!(points[0].x, 2.0);
        assert_eq!(points[2].sample.pressure, 0.25);
        assert!(!bridge.is_stroke_active());
    }

    #[test]
    fn tablet_bridge_bounds_strokes_and_cancels_on_proximity_loss() {
        let mut bridge = TabletEventBridge::with_maximum_points(2).unwrap();
        bridge
            .push(qt_event(NativeTabletPhase::Press, 1.0, 1.0, 1.0))
            .unwrap();
        bridge
            .push(qt_event(NativeTabletPhase::Move, 1.0, 2.0, 1.0))
            .unwrap();
        assert_eq!(
            bridge.push(qt_event(NativeTabletPhase::Move, 1.0, 3.0, 1.0)),
            Err(TabletBridgeError::StrokeTooLong { maximum: 2 })
        );
        assert!(!bridge.is_stroke_active());

        bridge
            .push(qt_event(NativeTabletPhase::Press, 1.0, 1.0, 1.0))
            .unwrap();
        assert_eq!(
            bridge.push(qt_event(NativeTabletPhase::ProximityLeave, 1.0, 1.0, 0.0)),
            Ok(TabletBridgeOutput::StrokeCancelled)
        );
        assert!(!bridge.is_in_proximity());
        assert!(!bridge.is_stroke_active());
    }

    #[test]
    fn tablet_bridge_rejects_invalid_native_ranges_and_coordinates() {
        assert_eq!(
            NativeTabletEvent::from_axes(
                NativeTabletPhase::Move,
                0.0,
                0.0,
                0.5,
                1.0,
                1.0,
                0.0,
                0.0,
                60.0,
                0.0,
                360.0,
                false,
            ),
            Err(EditorError::InvalidTabletRange { field: "pressure" })
        );
        let mut bridge = TabletEventBridge::new();
        assert_eq!(
            bridge.push(qt_event(NativeTabletPhase::Move, f32::NAN, 0.0, 1.0)),
            Err(TabletBridgeError::InvalidEventCoordinate)
        );
    }
}
