//! Real-time devices of the person's own: a WebAssembly module, compiled by
//! the engine off the audio thread and run on it, an instance to each place
//! the device is in a chain.
//!
//! The module meets the device ABI, version 1. It imports nothing, and
//! exports its `memory` and these functions:
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
//! A call that traps, runs past its deadline, or gives samples that are not
//! numbers ends the instance: the device passes its input through from
//! then on, and the code keeps the reason for the host to say.

use aaw_dsp::device::{Live, Running};
use aaw_dsp::envelope::{Envelope, Param};
use aaw_dsp::{Clock, Frame};
use aaw_model::rules::Domain;
use aaw_model::FieldValue;
use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use wasmtime::{Engine, Instance, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc};

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
    fields: usize,
    left: usize,
    right: usize,
}

fn deadline(store: &mut Store<StoreLimits>) {
    store.set_epoch_deadline(DEADLINE_TICKS);
}

/// Instantiates the module and runs `aaw_init`, checking every export the
/// ABI names; the latency is the second value.
fn instantiate(module: &Module, fields: usize, rate: f64, tempo: f64) -> Result<(Wasm, usize)> {
    let engine = engine()?;
    if let Some(import) = module.imports().next() {
        return Err(format!("the module imports {} from {}; a device imports nothing", import.name(), import.module()));
    }
    let limits = StoreLimitsBuilder::new().memory_size(MEMORY_BYTES).instances(1).build();
    let mut store = Store::new(engine, limits);
    store.limiter(|l| l);
    deadline(&mut store);
    let instance = Instance::new(&mut store, module, &[]).map_err(|e| format!("the module does not start: {e:#}"))?;
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
    /// exports and runs it once on silence.
    pub fn new(bytes: &[u8], fields: Vec<Field>, rate: f64, tempo: f64) -> Result<Code> {
        let module = compile(bytes)?;
        let (mut wasm, latency) = instantiate(&module, fields.len(), rate, tempo)?;
        let silence = vec![[0.0; 2]; FRAMES];
        let values: Vec<f64> = fields.iter().map(|f| f.number(None)).collect();
        call(&mut wasm, &values, &silence, 0, true).map(|_| ())?;
        let signature = hash_of((hash_of(bytes), &values.len(), rate.to_bits(), tempo.to_bits(), latency));
        Ok(Code {
            module,
            fields,
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

/// Writes the fields and a chunk of at most `FRAMES` frames, runs the
/// module on it and reads the output back into `x`. Leaves `x` as it was
/// on a failure, which is the error.
fn call(wasm: &mut Wasm, values: &[f64], x: &[Frame], frame: i64, rolling: bool) -> Result<Vec<Frame>> {
    let mut out = x.to_vec();
    run(wasm, values, &mut out, frame, rolling)?;
    Ok(out)
}

/// `call` in place, without allocating.
fn run(wasm: &mut Wasm, values: &[f64], x: &mut [Frame], frame: i64, rolling: bool) -> Result<()> {
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
            match instantiate(&self.module, self.fields.len(), self.rate, self.tempo) {
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
        let rolling = clock.step != 0;
        for (k, chunk) in x.chunks_mut(FRAMES).enumerate() {
            if !self.alive || self.wasm.is_none() {
                self.through(chunk);
                continue;
            }
            let frame = clock.frame + (k * FRAMES) as i64 * clock.step;
            for (i, p) in self.params.iter().enumerate() {
                let (lo, hi) = self.bounds[i];
                self.values[i] = if p.moves() { p.at(frame).clamp(lo, hi) } else { p.value() };
            }
            let wasm = self.wasm.as_mut().expect("an instance");
            if let Err(e) = run(wasm, &self.values, chunk, frame, rolling) {
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
