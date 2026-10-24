//! FAUST as a real-time device's source: `device.dsp` compiled by the FAUST
//! compiler's WebAssembly backend, its controls bound to the
//! manifest's fields by name, and wrapped in the device ABI's exports, so
//! the engine runs it as it runs a module built from Rust.
//!
//! FAUST's module keeps its state at address 0 and lists its controls,
//! each with the address of its value, in a JSON it writes there before
//! `init` overwrites it. The wrapper puts the ABI's fields and buffers past
//! both, writes each field's value to its control before `compute`, and
//! copies the output back over the input. A mono effect is compiled again
//! as two of itself side by side, which share their controls; one taking
//! one channel and giving two is given the two channels' mean.

use super::{num, FieldKind, FieldSpec, Installed, Manifest};
use serde_json::Value as Json;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T, String>;

/// The frames one call is given, as the ABI says.
const FRAMES: usize = aaw_engine::wasm::FRAMES;

/// Whether a manifest's source is FAUST's.
pub fn is_faust(source: &str) -> bool {
    source.ends_with(".dsp")
}

/// The FAUST compiler: `AAW_FAUST`, else `faust` on the path.
fn compiler() -> PathBuf {
    std::env::var_os("AAW_FAUST").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("faust"))
}

/// A control of the FAUST code: a slider or a number box, or a checkbox
/// or a button, which a switch moves.
#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    /// The field it is bound to: its label in lowercase, with every run of
    /// anything but letters and digits an underscore.
    pub name: String,
    pub label: String,
    pub switch: bool,
    /// The byte its value is at, an f32.
    pub zone: u32,
    pub min: f64,
    pub max: f64,
    pub init: f64,
    pub unit: String,
    pub log: bool,
}

/// What FAUST's compiler made: its module as text, the functions the ABI
/// calls, and what its JSON says.
#[derive(Clone, Debug)]
pub struct Module {
    pub text: String,
    /// The indexes of its `init` and `compute`.
    pub init: u32,
    pub compute: u32,
    /// The bytes of state at address 0.
    pub size: usize,
    /// The bytes of the JSON written at address 0 when the module starts.
    pub data: usize,
    pub inputs: usize,
    pub outputs: usize,
    pub controls: Vec<Control>,
}

/// A control's label as a field's name.
fn field_name(label: &str) -> String {
    let mut out = String::new();
    for c in label.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn walk(items: &[Json], out: &mut Vec<Control>) -> Result<()> {
    for item in items {
        let kind = item["type"].as_str().unwrap_or_default();
        if let Some(inner) = item["items"].as_array() {
            walk(inner, out)?;
            continue;
        }
        let label = item["label"].as_str().unwrap_or_default().to_string();
        let switch = match kind {
            "hslider" | "vslider" | "nentry" => false,
            "checkbox" | "button" => true,
            // A bargraph is the code's output, which no field moves.
            "hbargraph" | "vbargraph" => continue,
            other => return Err(format!("the control {label:?} is a {other}, which a device's fields do not move")),
        };
        let meta = item["meta"].as_array().cloned().unwrap_or_default();
        let said = |key: &str| meta.iter().find_map(|m| m[key].as_str()).unwrap_or_default().to_string();
        let number = |key: &str| item[key].as_f64().unwrap_or(0.0);
        let zone = item["index"].as_u64().ok_or_else(|| format!("the control {label:?} has no index in the compiler's JSON"))?;
        out.push(Control {
            name: field_name(&label),
            label,
            switch,
            zone: zone as u32,
            min: if switch { 0.0 } else { number("min") },
            max: if switch { 1.0 } else { number("max") },
            init: if switch { 0.0 } else { number("init") },
            unit: said("unit"),
            log: said("scale") == "log",
        });
    }
    Ok(())
}

/// Reads what FAUST's compiler wrote, `-lang wasm`: the JSON at address
/// 0, with the state's size, the channels and the controls, and the
/// module printed as text.
pub fn read_module(bytes: &[u8]) -> Result<Module> {
    use wasmparser::{DataKind, ExternalKind, Operator, Parser, Payload};
    let unread = |e: wasmparser::BinaryReaderError| format!("the compiler's module does not read: {e}");
    let (mut json, mut init, mut compute) = (None, None, None);
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(unread)? {
            Payload::ExportSection(exports) => {
                for e in exports {
                    let e = e.map_err(unread)?;
                    match (e.name, e.kind) {
                        ("init", ExternalKind::Func) => init = Some(e.index),
                        ("compute", ExternalKind::Func) => compute = Some(e.index),
                        _ => {}
                    }
                }
            }
            Payload::DataSection(data) => {
                for d in data {
                    let d = d.map_err(unread)?;
                    let DataKind::Active { offset_expr, .. } = d.kind else { continue };
                    let at_zero = matches!(offset_expr.get_operators_reader().read(), Ok(Operator::I32Const { value: 0 }));
                    if at_zero && json.is_none() {
                        json = Some(d.data.to_vec());
                    }
                }
            }
            _ => {}
        }
    }
    let data = json.ok_or("the compiler's module has no JSON at address 0; this reads FAUST's -lang wasm")?;
    let (Some(init), Some(compute)) = (init, compute) else {
        return Err("the compiler's module exports no init and compute; this reads FAUST's -lang wasm".into());
    };
    let json: Json = serde_json::Deserializer::from_slice(&data)
        .into_iter()
        .next()
        .ok_or("the compiler's JSON is empty")?
        .map_err(|e| format!("the compiler's JSON does not read: {e}"))?;
    let count = |key: &str| json[key].as_u64().map(|n| n as usize).ok_or_else(|| format!("the compiler's JSON has no {key}"));
    let mut controls = Vec::new();
    walk(json["ui"].as_array().map(Vec::as_slice).unwrap_or_default(), &mut controls)?;
    let mut names: Vec<&str> = controls.iter().map(|c| c.name.as_str()).collect();
    names.sort_unstable();
    if let Some(w) = names.windows(2).find(|w| w[0] == w[1]) {
        return Err(format!("two controls are named {}: give one another label", w[0]));
    }
    Ok(Module {
        text: wasmprinter::print_bytes(bytes).map_err(|e| format!("the compiler's module does not print: {e}"))?,
        init,
        compute,
        size: count("size")?,
        data: data.len(),
        inputs: count("inputs")?,
        outputs: count("outputs")?,
        controls,
    })
}

/// The manifest's line for a control it has no field for.
fn field_line(c: &Control) -> String {
    if c.switch {
        return format!("{}: {{type: switch, default: false}}", c.name);
    }
    let kind = match c.unit.to_lowercase().as_str() {
        "hz" => "hz",
        "db" => "db",
        "ms" => "ms",
        "%" => "percent",
        _ => "number",
    };
    let scale = match (kind, c.log) {
        ("hz", false) => ", scale: linear",
        ("hz", true) | (_, false) => "",
        (_, true) => ", scale: log",
    };
    format!("{}: {{type: {kind}, min: {}, max: {}, default: {}{scale}}}", c.name, num(c.min), num(c.max), num(c.init))
}

/// Binds each of the manifest's fields to the control of its name, in the
/// manifest's order: a switch to a checkbox or a button, a number or a
/// choice to a slider or a number box whose range holds the field's (a
/// choice's control is given the index of its choice). Every control has a
/// field; what does not match is refused in words, with the line to add.
pub fn bind(m: &Module, manifest: &Manifest, source: &str) -> Result<Vec<u32>> {
    let names = || m.controls.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ");
    if let Some(c) = m.controls.iter().find(|c| manifest.field(&c.name).is_none()) {
        let valid = aaw_model::valid_device_field(&c.name);
        return Err(match valid {
            true => format!("the control {:?} in {source} has no field in device.yaml; under fields add\n  {}", c.label, field_line(c)),
            false => format!("the control {:?} in {source} names no field: a label is a field's name, lowercase letters, digits and underscores starting with a letter", c.label),
        });
    }
    let mut zones = Vec::new();
    for f in &manifest.fields {
        let Some(c) = m.controls.iter().find(|c| c.name == f.name) else {
            return Err(format!(
                "the field {} has no control in {source}: a slider, a number box, a checkbox or a button labelled {}; its controls are {}",
                f.name,
                f.name,
                match m.controls.is_empty() {
                    true => "none".to_string(),
                    false => names(),
                }
            ));
        };
        check_kind(f, c, source)?;
        zones.push(c.zone);
    }
    Ok(zones)
}

fn check_kind(f: &FieldSpec, c: &Control, source: &str) -> Result<()> {
    let within = |lo: f64, hi: f64| lo >= c.min - 1e-9 * c.min.abs().max(1.0) && hi <= c.max + 1e-9 * c.max.abs().max(1.0);
    match (f.kind, c.switch) {
        (FieldKind::Switch, true) => Ok(()),
        (FieldKind::Switch, false) => Err(format!("{} is a switch, and its control in {source} is a slider: make it a checkbox", f.name)),
        (_, true) => Err(format!("{} is a {}, and its control in {source} is a checkbox or a button: make the field a switch", f.name, f.kind.name())),
        (FieldKind::Choice, false) if within(0.0, (f.choices.len() - 1) as f64) => Ok(()),
        (FieldKind::Choice, false) => Err(format!(
            "{} is a choice of {}, given to its control as 0 to {}; its control in {source} takes {} to {}",
            f.name,
            f.choices.len(),
            f.choices.len() - 1,
            num(c.min),
            num(c.max)
        )),
        (_, false) if within(f.min, f.max) => Ok(()),
        (_, false) => Err(format!(
            "{} is {} to {} in device.yaml, and its control in {source} takes {} to {}: make the field's range within the control's",
            f.name,
            num(f.min),
            num(f.max),
            num(c.min),
            num(c.max)
        )),
    }
}

/// FAUST's module as one meeting the device ABI: its two imported globals
/// defined, its memory grown to hold the ABI's buffers past its state and
/// its JSON, and the ABI's exports added. `zones` are the fields' controls,
/// in the manifest's order.
pub fn wrap(m: &Module, zones: &[u32]) -> Result<String> {
    if !(1..=2).contains(&m.inputs) {
        return Err(format!("the code takes {} channels; an effect's process takes 1 or 2, the sound it is given", m.inputs));
    }
    if !(1..=2).contains(&m.outputs) {
        return Err(format!("the code gives {} channels; an effect's process gives 1 or 2", m.outputs));
    }
    let align = |n: usize| n.div_ceil(16) * 16;
    let bytes = FRAMES * 4;
    let fields = align(m.size.max(m.data) + 16);
    let left = fields + align(8 * zones.len().max(1));
    let right = left + bytes;
    let out0 = right + bytes;
    let out1 = out0 + bytes;
    let mono = out1 + bytes;
    let ins = mono + bytes;
    let outs = ins + 8;
    let end = outs + 8;

    // The two globals FAUST imports, which nothing reads, are defined
    // after the other imports; the memory holds the ABI's buffers.
    let mut lines: Vec<String> = Vec::new();
    let mut last_import = None;
    let mut grown = false;
    for line in m.text.lines() {
        let t = line.trim();
        if t.starts_with("(import \"env\" \"memoryBase\"") || t.starts_with("(import \"env\" \"tableBase\"") {
            continue;
        }
        if t.starts_with("(import ") {
            last_import = Some(lines.len());
        }
        if let Some(rest) = t.strip_prefix("(memory ") {
            let mut pages = rest.trim_end_matches(')').split_whitespace().filter(|p| !p.starts_with("(;") && !p.starts_with('$')).map(str::parse::<usize>);
            let (Some(Ok(initial)), max) = (pages.next(), pages.next()) else {
                return Err(format!("the compiler's memory does not read: {t}"));
            };
            let need = initial.max(end.div_ceil(65536));
            let max = match max {
                Some(Ok(max)) => format!(" {}", max.max(need)),
                _ => String::new(),
            };
            lines.push(format!("  (memory {need}{max})"));
            grown = true;
            continue;
        }
        lines.push(line.to_string());
    }
    if !grown || !m.text.contains("(export \"memory\" (memory 0))") {
        return Err("the compiler's module exports no memory this reads".into());
    }
    let globals = ["  (global $memoryBase i32 i32.const 0)".to_string(), "  (global $tableBase i32 i32.const 0)".to_string()];
    let after = last_import.map_or(1, |i| i + 1);
    lines.splice(after..after, globals);
    let mut text = lines.join("\n");
    let close = text.rfind(')').ok_or("the compiler's module does not close")?;
    text.truncate(close);

    let mut abi = String::new();
    let _ = write!(
        abi,
        "
\t;; The device ABI, version 1, around the code above.
\t(func (export \"aaw_abi\") (result i32) (i32.const 1))
\t(func (export \"aaw_fields\") (result i32) (i32.const {fields}))
\t(func (export \"aaw_left\") (result i32) (i32.const {left}))
\t(func (export \"aaw_right\") (result i32) (i32.const {right}))
\t(func (export \"aaw_init\") (param $rate f64) (param $tempo f64)
\t\t(i32.store (i32.const {ins}) (i32.const {in0}))
\t\t(i32.store (i32.const {in1at}) (i32.const {right}))
\t\t(i32.store (i32.const {outs}) (i32.const {out0}))
\t\t(i32.store (i32.const {out1at}) (i32.const {out1}))
\t\t(call {init} (i32.const 0) (i32.trunc_sat_f64_s (f64.nearest (local.get $rate)))))
\t(func (export \"aaw_process\") (param $frames i32) (param $frame i64) (param $rolling i32)
\t\t(local $bytes i32) (local $at i32)
\t\t(local.set $bytes (i32.shl (local.get $frames) (i32.const 2)))
",
        in0 = if m.inputs == 1 { mono } else { left },
        in1at = ins + 4,
        out1at = outs + 4,
        init = m.init,
    );
    for (i, zone) in zones.iter().enumerate() {
        let _ = writeln!(abi, "\t\t(f32.store (i32.const {zone}) (f32.demote_f64 (f64.load (i32.const {}))))", fields + 8 * i);
    }
    if m.inputs == 1 {
        let _ = write!(
            abi,
            "\t\t(block $done (loop $each
\t\t\t(br_if $done (i32.ge_u (local.get $at) (local.get $bytes)))
\t\t\t(f32.store (i32.add (i32.const {mono}) (local.get $at)) (f32.mul (f32.const 0.5) (f32.add (f32.load (i32.add (i32.const {left}) (local.get $at))) (f32.load (i32.add (i32.const {right}) (local.get $at))))))
\t\t\t(local.set $at (i32.add (local.get $at) (i32.const 4)))
\t\t\t(br $each)))
"
        );
    }
    let _ = write!(
        abi,
        "\t\t(call {compute} (i32.const 0) (local.get $frames) (i32.const {ins}) (i32.const {outs}))
\t\t(memory.copy (i32.const {left}) (i32.const {out0}) (local.get $bytes))
\t\t(memory.copy (i32.const {right}) (i32.const {second}) (local.get $bytes)))
)
",
        second = if m.outputs == 2 { out1 } else { out0 },
        compute = m.compute,
    );
    text.push_str(&abi);
    Ok(text)
}

/// Runs the compiler on `source` with the device's folder to find what it
/// imports, writing FAUST's module to `out`.
fn compile(source: &Path, folder: &Path, out: &Path, name: &str) -> Result<Vec<u8>> {
    let faust = compiler();
    let run = Command::new(&faust)
        .args(["-lang", "wasm", "-ftz", "2", "-I"])
        .arg(folder)
        .arg("-o")
        .arg(out)
        .arg(source)
        .current_dir(folder)
        .output()
        .map_err(|e| {
            format!(
                "Could not start {}: {e}; the FAUST compiler builds a .dsp source: brew install faust, or faust.grame.fr, or AAW_FAUST naming one",
                faust.display()
            )
        })?;
    if !run.status.success() {
        let said = format!("{}{}", String::from_utf8_lossy(&run.stderr), String::from_utf8_lossy(&run.stdout));
        return Err(format!("{name} does not build:\n{}", said.trim()));
    }
    std::fs::read(out).map_err(|e| format!("the compiler wrote no {}: {e}", out.display()))
}

/// Builds a FAUST device's module to `to`: compiles its source, twice for
/// a mono effect, binds its controls to the manifest's fields, wraps it in
/// the ABI and assembles it.
pub fn build(device: &Installed, source: &str, to: &Path) -> Result<()> {
    let m = &device.manifest;
    if m.kind != "effect" {
        return Err(format!("{source} is FAUST, which builds an effect; a real-time {}'s source is Rust, device.rs", m.kind));
    }
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let src = device.dir.join(source);
    let mut module = read_module(&compile(&src, &device.dir, &work.path().join("device.wasm"), source)?)?;
    if module.inputs == 1 && module.outputs == 1 {
        // Each channel through a copy of the code, the copies' controls one.
        if source.contains('"') || source.contains('\\') {
            return Err(format!("{source}: a FAUST source's name has no quote or backslash"));
        }
        let stereo = work.path().join("stereo.dsp");
        std::fs::write(&stereo, format!("process = par(i, 2, component(\"{source}\"));\n")).map_err(|e| e.to_string())?;
        module = read_module(&compile(&stereo, &device.dir, &work.path().join("stereo.wasm"), source)?)?;
    }
    let zones = bind(&module, m, source)?;
    let text = wrap(&module, &zones)?;
    let bytes = wat::parse_str(&text).map_err(|e| format!("{source}: the module made from it does not assemble: {e}"))?;
    std::fs::write(to, bytes).map_err(|e| format!("{}: {e}", to.display()))
}
