//! Real-time devices of the person's own: a WebAssembly module, compiled by
//! the engine off the audio thread and run on it, an instance to each place
//! the device is in a chain.
//!
//! The module meets the device ABI, version 1. It imports nothing but
//! the math functions of [`MATH`] from `env`, which the engine gives it
//! (FAUST's code calls them), and exports its `memory` and these
//! functions:
//!
//! - `aaw_abi() -> i32`, which returns 1;
//! - `aaw_init(sample_rate: f64, tempo: f64)`, called once before any audio;
//! - `aaw_fields() -> i32`, where the engine writes the fields' values
//!   before each call of `aaw_process`, an `f64` each in the manifest's
//!   order: a number as itself, a switch as 1 or 0, a choice as the index
//!   of its choice;
//! - `aaw_left() -> i32` and `aaw_right() -> i32`, where the engine writes
//!   the input, 128 `f32` frames each, and reads the output;
//! - `aaw_process(frames: i32, frame: i64, rolling: i32)`, which processes
//!   `frames`, 1 to 128, in place; `frame` is the timeline frame of the
//!   first, and `rolling` is 0 while the transport stands still and a tail
//!   rings out;
//! - optionally `aaw_latency() -> i32`, the frames by which the device
//!   delays its input, read after `aaw_init`.
//!
//! An instrument also exports `aaw_note(at: i32, key: i32, velocity: i32)`,
//! called before `aaw_process` for each note starting or ending within the
//! frames it is given, in order: `at` the frame among them, `key` the MIDI
//! note, `velocity` 1 to 127 for a start and 0 for an end. Its buffers hold
//! silence when `aaw_process` is called, and it writes its sound there.
//!
//! A call that traps, runs past its deadline, or gives samples that are not
//! numbers ends the instance: the device passes its input through from
//! then on, and the code keeps the reason for the host to say.

use aaw_dsp::device::{Live, Running};
use aaw_dsp::instrument::NoteEvent;
use aaw_dsp::envelope::{Envelope, Param};
use aaw_dsp::{Clock, Frame};
use aaw_model::rules::Domain;
use aaw_model::FieldValue;
use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use wasmtime::{Engine, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc};

/// The ABI version this runs.
pub const ABI: i32 = 1;
/// The most frames one call of `aaw_process` is given.
pub const FRAMES: usize = 128;
/// The most memory an instance may have.
pub const MEMORY_BYTES: usize = 256 << 20;
/// The most latency a device may declare: ten seconds at the song's rate.
const MAX_LATENCY_SECONDS: f64 = 10.0;
/// How often the deadline clock ticks, and how many ticks a call may take:
/// a call running 150 to 200 ms has stalled the audio long since.
const TICK: Duration = Duration::from_millis(50);
const DEADLINE_TICKS: u64 = 4;

type Result<T> = std::result::Result<T, String>;

/// The one wasmtime engine, whose epoch a thread advances for the
/// deadlines.
fn engine() -> Result<&'static Engine> {
    static ENGINE: OnceLock<std::result::Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = wasmtime::Config::new();
            config.epoch_interruption(true);
            let engine = Engine::new(&config).map_err(|e| format!("WebAssembly is not available: {e}"))?;
            let ticking = engine.clone();
            std::thread::Builder::new()
                .name("device deadlines".into())
                .spawn(move || loop {
                    std::thread::sleep(TICK);
                    ticking.increment_epoch();
                })
                .map_err(|e| format!("WebAssembly is not available: {e}"))?;
            Ok(engine)
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// The math functions a module may import from `env`, single precision,
/// by the names and types FAUST's WebAssembly backend calls them.
pub const MATH: [&str; 23] = [
    "_acosf", "_acoshf", "_asinf", "_asinhf", "_atanf", "_atan2f", "_atanhf", "_copysignf", "_cosf", "_coshf", "_expf", "_fmodf", "_isinff", "_isnanf", "_logf", "_log10f", "_powf",
    "_remainderf", "_roundf", "_sinf", "_sinhf", "_tanf", "_tanhf",
];

/// What instantiates a module: the math of [`MATH`] under `env`.
fn linker() -> Result<&'static Linker<StoreLimits>> {
    static LINKER: OnceLock<std::result::Result<Linker<StoreLimits>, String>> = OnceLock::new();
    LINKER
        .get_or_init(|| {
            let mut l = Linker::new(engine()?);
            let failed = |e: wasmtime::Error| format!("WebAssembly is not available: {e}");
            macro_rules! one {
                ($($name:literal => $f:expr),* $(,)?) => {
                    $(l.func_wrap("env", $name, $f).map_err(failed)?;)*
                };
            }
            one! {
                "_acosf" => |x: f32| x.acos(),
                "_acoshf" => |x: f32| x.acosh(),
                "_asinf" => |x: f32| x.asin(),
                "_asinhf" => |x: f32| x.asinh(),
                "_atanf" => |x: f32| x.atan(),
                "_atan2f" => |y: f32, x: f32| y.atan2(x),
                "_atanhf" => |x: f32| x.atanh(),
                "_copysignf" => |x: f32, y: f32| x.copysign(y),
                "_cosf" => |x: f32| x.cos(),
                "_coshf" => |x: f32| x.cosh(),
                "_expf" => |x: f32| x.exp(),
                "_fmodf" => |x: f32, y: f32| x % y,
                "_isinff" => |x: f32| i32::from(x.is_infinite()),
                "_isnanf" => |x: f32| i32::from(x.is_nan()),
                "_logf" => |x: f32| x.ln(),
                "_log10f" => |x: f32| x.log10(),
                "_powf" => |x: f32, y: f32| x.powf(y),
                "_remainderf" => |x: f32, y: f32| x - (x / y).round_ties_even() * y,
                "_roundf" => |x: f32| x.round(),
                "_sinf" => |x: f32| x.sin(),
                "_sinhf" => |x: f32| x.sinh(),
                "_tanf" => |x: f32| x.tan(),
                "_tanhf" => |x: f32| x.tanh(),
            }
            Ok(l)
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn hash_of(parts: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

/// Compiles a module, or takes it from the modules compiled before by its
/// bytes: the same code in several places, or made again after an edit,
/// compiles once.
fn compile(bytes: &[u8]) -> Result<Module> {
    static MODULES: OnceLock<Mutex<HashMap<u64, Module>>> = OnceLock::new();
    let key = hash_of(bytes);
    let modules = MODULES.get_or_init(Default::default);
    if let Some(m) = modules.lock().expect("the modules").get(&key) {
        return Ok(m.clone());
    }
    let module = Module::new(engine()?, bytes).map_err(|e| format!("the module does not compile: {e:#}"))?;
    let mut map = modules.lock().expect("the modules");
    if map.len() >= 32 {
        map.clear();
    }
    map.insert(key, module.clone());
    Ok(module)
}

/// A field as the code is given it, from the manifest.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub default: FieldValue,
    pub min: f64,
    pub max: f64,
    /// Whether a lane on it moves on a log scale.
    pub log: bool,
    /// A choice's choices, whose index the code is given.
    pub choices: Vec<String>,
}

impl Field {
    /// The value the code is given for the song's value, or the default.
    fn number(&self, value: Option<&FieldValue>) -> f64 {
        let number = |v: &FieldValue| match v {
            FieldValue::Number(x) => Some(x.clamp(self.min, self.max)),
            FieldValue::Flag(b) => Some(f64::from(u8::from(*b))),
            FieldValue::Text(s) => self.choices.iter().position(|c| c == s).map(|i| i as f64),
        };
        value.and_then(number).or_else(|| number(&self.default)).unwrap_or(0.0)
    }
}

/// A module made ready to run: compiled, its exports checked, and tried
/// once at the song's rate and tempo.
pub struct Code {
    module: Module,
    fields: Vec<Field>,
    /// Whether the code is an instrument's, which is given notes.
    instrument: bool,
    rate: f64,
    tempo: f64,
    latency: usize,
    signature: u64,
    /// Why the code failed in a chain, when it has: kept for the host.
    failure: Arc<Mutex<Option<String>>>,
}

impl std::fmt::Debug for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Code").field("fields", &self.fields).field("latency", &self.latency).finish()
    }
}

/// An instance's exports, found once.
struct Wasm {
    store: Store<StoreLimits>,
    memory: Memory,
    process: TypedFunc<(i32, i64, i32), ()>,
    /// An instrument's `aaw_note`.
    note: Option<TypedFunc<(i32, i32, i32), ()>>,
    fields: usize,
    left: usize,
    right: usize,
}

fn deadline(store: &mut Store<StoreLimits>) {
    store.set_epoch_deadline(DEADLINE_TICKS);
}

/// Instantiates the module and runs `aaw_init`, checking every export the
/// ABI names, `aaw_note` too for an instrument; the latency is the second
/// value.
fn instantiate(module: &Module, fields: usize, rate: f64, tempo: f64, instrument: bool) -> Result<(Wasm, usize)> {
    let engine = engine()?;
    if let Some(import) = module.imports().find(|i| i.module() != "env" || !MATH.contains(&i.name())) {
        return Err(format!(
            "the module imports {} from {}; a device imports nothing but the math functions env._sinf and the like (daw describe plugins.abi)",
            import.name(),
            import.module()
        ));
    }
    let limits = StoreLimitsBuilder::new().memory_size(MEMORY_BYTES).instances(1).build();
    let mut store = Store::new(engine, limits);
    store.limiter(|l| l);
    deadline(&mut store);
    let instance = linker()?.instantiate(&mut store, module).map_err(|e| format!("the module does not start: {e:#}"))?;
    let memory = instance.get_memory(&mut store, "memory").ok_or("the module exports no memory")?;
    macro_rules! func {
        ($name:literal, $params:ty, $results:ty, $shape:literal) => {
            instance
                .get_typed_func::<$params, $results>(&mut store, $name)
                .map_err(|_| format!("the module exports no {} {}", $name, $shape))?
        };
    }
    let abi = func!("aaw_abi", (), i32, "() -> i32");
    let init = func!("aaw_init", (f64, f64), (), "(f64, f64)");
    let fields_at = func!("aaw_fields", (), i32, "() -> i32");
    let left_at = func!("aaw_left", (), i32, "() -> i32");
    let right_at = func!("aaw_right", (), i32, "() -> i32");
    let process = func!("aaw_process", (i32, i64, i32), (), "(i32, i64, i32)");
    let note = match instrument {
        true => Some(func!("aaw_note", (i32, i32, i32), (), "(i32, i32, i32), which an instrument is given its notes by")),
        false => None,
    };
    let trap = |what: &str, e: wasmtime::Error| format!("{what} failed: {e:#}");
    let version = abi.call(&mut store, ()).map_err(|e| trap("aaw_abi", e))?;
    if version != ABI {
        return Err(format!("the module is of device ABI {version}; this engine runs ABI {ABI}"));
    }
    deadline(&mut store);
    init.call(&mut store, (rate, tempo)).map_err(|e| trap("aaw_init", e))?;
    let latency = match instance.get_func(&mut store, "aaw_latency") {
        None => 0,
        Some(f) => {
            let f = f.typed::<(), i32>(&store).map_err(|_| "the module's aaw_latency is not () -> i32".to_string())?;
            deadline(&mut store);
            let frames = f.call(&mut store, ()).map_err(|e| trap("aaw_latency", e))?;
            if frames < 0 || frames as f64 > MAX_LATENCY_SECONDS * rate {
                return Err(format!("aaw_latency gave {frames} frames; a device's latency is 0 to {MAX_LATENCY_SECONDS} seconds"));
            }
            frames as usize
        }
    };
    let mut place = |f: TypedFunc<(), i32>, what: &str, bytes: usize| -> Result<usize> {
        deadline(&mut store);
        let at = f.call(&mut store, ()).map_err(|e| trap(what, e))? as u32 as usize;
        let size = memory.data_size(&store);
        if at == 0 || at + bytes > size {
            return Err(format!("{what} gave {at}, where {bytes} bytes do not fit the memory's {size}"));
        }
        Ok(at)
    };
    let fields_ptr = if fields == 0 { 0 } else { place(fields_at, "aaw_fields", fields * 8)? };
    let left = place(left_at, "aaw_left", FRAMES * 4)?;
    let right = place(right_at, "aaw_right", FRAMES * 4)?;
    if left.abs_diff(right) < FRAMES * 4 {
        return Err("aaw_left and aaw_right overlap; each holds 128 frames".into());
    }
    Ok((
        Wasm {
            store,
            memory,
            process,
            note,
            fields: fields_ptr,
            left,
            right,
        },
        latency,
    ))
}

impl Code {
    /// Makes code ready from a module's bytes, for a device with these
    /// fields in a song at `rate` and `tempo`: compiles it, checks its
    /// exports and runs it once on silence, an instrument with a note
    /// played and let go.
    pub fn new(bytes: &[u8], fields: Vec<Field>, rate: f64, tempo: f64, instrument: bool) -> Result<Code> {
        let module = compile(bytes)?;
        let (mut wasm, latency) = instantiate(&module, fields.len(), rate, tempo, instrument)?;
        let mut silence = vec![[0.0; 2]; FRAMES];
        let values: Vec<f64> = fields.iter().map(|f| f.number(None)).collect();
        let notes = [NoteEvent { at: 0, key: 60, velocity: 100 }, NoteEvent { at: FRAMES / 2, key: 60, velocity: 0 }];
        run(&mut wasm, &values, &mut silence, if instrument { &notes } else { &[] }, 0, true)?;
        let signature = hash_of((hash_of(bytes), &values.len(), rate.to_bits(), tempo.to_bits(), latency, instrument));
        Ok(Code {
            module,
            fields,
            instrument,
            rate,
            tempo,
            latency,
            signature,
            failure: Arc::new(Mutex::new(None)),
        })
    }

    /// Why the code failed in a chain, if it has.
    pub fn failure(&self) -> Option<String> {
        self.failure.lock().ok().and_then(|f| f.clone())
    }

    pub fn latency(&self) -> usize {
        self.latency
    }

    /// Whether the code is an instrument's, given notes by `aaw_note`.
    pub fn instrument(&self) -> bool {
        self.instrument
    }

    /// An instance with the fields at `values`, in the manifest's order,
    /// none moving: for a check run.
    pub fn instance_at(&self, values: &BTreeMap<String, FieldValue>) -> Box<dyn Running> {
        let device = aaw_model::Device {
            id: None,
            name: String::new(),
            version: None,
            fields: values.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            bypass: false,
        };
        self.instance(&device, &BTreeMap::new())
    }

    fn fail(&self, why: String) {
        if let Ok(mut f) = self.failure.try_lock() {
            f.get_or_insert(why);
        }
    }
}

/// Writes the fields and a chunk of at most `FRAMES` frames, gives an
/// instrument the notes within it, runs the module on it and reads the
/// output back into `x`, in place and without allocating. A failure is
/// the error, and leaves `x` as it was.
fn run(wasm: &mut Wasm, values: &[f64], x: &mut [Frame], notes: &[NoteEvent], frame: i64, rolling: bool) -> Result<()> {
    debug_assert!(x.len() <= FRAMES);
    let data = wasm.memory.data_mut(&mut wasm.store);
    for (i, v) in values.iter().enumerate() {
        let at = wasm.fields + 8 * i;
        data[at..at + 8].copy_from_slice(&v.to_le_bytes());
    }
    for (i, f) in x.iter().enumerate() {
        data[wasm.left + 4 * i..wasm.left + 4 * i + 4].copy_from_slice(&(f[0] as f32).to_le_bytes());
        data[wasm.right + 4 * i..wasm.right + 4 * i + 4].copy_from_slice(&(f[1] as f32).to_le_bytes());
    }
    if let Some(note) = &wasm.note {
        for e in notes {
            deadline(&mut wasm.store);
            if let Err(e) = note.call(&mut wasm.store, (e.at as i32, i32::from(e.key), i32::from(e.velocity))) {
                return Err(assert_no_alloc::permit_alloc(|| match e.downcast_ref::<wasmtime::Trap>() {
                    Some(wasmtime::Trap::Interrupt) => "aaw_note ran past its deadline".to_string(),
                    _ => format!("aaw_note failed: {e:#}"),
                }));
            }
        }
    }
    deadline(&mut wasm.store);
    if let Err(e) = wasm.process.call(&mut wasm.store, (x.len() as i32, frame, i32::from(rolling))) {
        return Err(assert_no_alloc::permit_alloc(|| match e.downcast_ref::<wasmtime::Trap>() {
            Some(wasmtime::Trap::Interrupt) => "aaw_process ran past its deadline".to_string(),
            _ => format!("aaw_process failed: {e:#}"),
        }));
    }
    // The memory only grows, so the places checked at the start still fit.
    let data = wasm.memory.data(&wasm.store);
    let sample = |at: usize| f32::from_le_bytes(data[at..at + 4].try_into().expect("four bytes"));
    for i in 0..x.len() {
        let (l, r) = (sample(wasm.left + 4 * i), sample(wasm.right + 4 * i));
        if !l.is_finite() || !r.is_finite() {
            return Err("aaw_process gave samples that are not numbers".to_string());
        }
        x[i] = [f64::from(l), f64::from(r)];
    }
    Ok(())
}

impl Live for Code {
    fn latency(&self) -> usize {
        self.latency
    }

    fn signature(&self) -> u64 {
        self.signature
    }

    fn instance(&self, device: &aaw_model::Device, lanes: &BTreeMap<String, Arc<Envelope>>) -> Box<dyn Running> {
        let params = self
            .fields
            .iter()
            .map(|f| {
                let value = f.number(device.fields.get(&f.name));
                let lane = lanes.get(&format!("fields.{}", f.name)).map(|e| Arc::new(e.in_domain(if f.log { Domain::Log } else { Domain::Linear })));
                Param::new(value, lane.as_ref())
            })
            .collect();
        let failed = self.failure().is_some();
        let wasm = if failed {
            None
        } else {
            match instantiate(&self.module, self.fields.len(), self.rate, self.tempo, self.instrument) {
                Ok((wasm, _)) => Some(wasm),
                Err(e) => {
                    self.fail(e);
                    None
                }
            }
        };
        Box::new(Playing {
            wasm,
            alive: true,
            params,
            bounds: self.fields.iter().map(|f| (f.min, f.max)).collect(),
            values: vec![0.0; self.fields.len()],
            line: vec![[0.0; 2]; self.latency],
            at: 0,
            notes: vec![NoteEvent { at: 0, key: 0, velocity: 0 }; if self.instrument { aaw_dsp::instrument::MAX_EVENTS } else { 0 }],
            failure: self.failure.clone(),
        })
    }
}

/// An instance in a chain: its module's state and its fields. With no
/// module, or once a call has failed, it passes its input through, delayed
/// by the code's latency as the chain expects.
struct Playing {
    wasm: Option<Wasm>,
    alive: bool,
    params: Vec<Param>,
    bounds: Vec<(f64, f64)>,
    values: Vec<f64>,
    /// The pass-through's delay line and where it is.
    line: Vec<Frame>,
    at: usize,
    /// An instrument's notes for a chunk, at the chunk's frames.
    notes: Vec<NoteEvent>,
    failure: Arc<Mutex<Option<String>>>,
}

impl Playing {
    fn through(&mut self, x: &mut [Frame]) {
        if self.line.is_empty() {
            return;
        }
        for f in x.iter_mut() {
            std::mem::swap(f, &mut self.line[self.at]);
            self.at = (self.at + 1) % self.line.len();
        }
    }
}

impl Running for Playing {
    fn process(&mut self, x: &mut [Frame], clock: Clock) {
        self.play(x, clock, &[]);
    }

    fn play(&mut self, x: &mut [Frame], clock: Clock, notes: &[NoteEvent]) {
        let rolling = clock.step != 0;
        let mut next = 0;
        for (k, chunk) in x.chunks_mut(FRAMES).enumerate() {
            // The notes within this chunk, at their frames among its own.
            let from = k * FRAMES;
            let mut upto = next;
            while upto < notes.len() && notes[upto].at < from + chunk.len() {
                upto += 1;
            }
            let mine = &notes[next..upto];
            next = upto;
            if !self.alive || self.wasm.is_none() {
                self.through(chunk);
                continue;
            }
            let frame = clock.frame + (k * FRAMES) as i64 * clock.step;
            for (i, p) in self.params.iter().enumerate() {
                let (lo, hi) = self.bounds[i];
                self.values[i] = if p.moves() { p.at(frame).clamp(lo, hi) } else { p.value() };
            }
            for (slot, e) in self.notes.iter_mut().zip(mine) {
                *slot = NoteEvent { at: e.at - from, ..*e };
            }
            let count = mine.len().min(self.notes.len());
            let wasm = self.wasm.as_mut().expect("an instance");
            if let Err(e) = run(wasm, &self.values, chunk, &self.notes[..count], frame, rolling) {
                self.alive = false;
                if let Ok(mut f) = self.failure.try_lock() {
                    f.get_or_insert(e);
                }
                self.through(chunk);
            }
        }
    }

    fn take_over(&mut self, old: &mut dyn Running) {
        let Some(old) = old.as_any().downcast_mut::<Playing>() else { return };
        if self.alive && old.alive && old.wasm.is_some() {
            std::mem::swap(&mut self.wasm, &mut old.wasm);
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
