// NAME: the scaffold of a real-time device in FAUST, to edit.
//
// A real-time device runs on the audio thread as WebAssembly. This file is
// its source; daw plugin build SLUG compiles it to device.wasm with the
// FAUST compiler (brew install faust, or AAW_FAUST naming one) and wraps
// it in the device ABI. stdfaust.lib, FAUST's own library, is there.
//
// Each control is the field of device.yaml its label names: a slider or a
// number box a number, or a choice, which it is given as the index of its
// choice; a checkbox a switch. A field's range is within its control's,
// and every control has a field. The engine sets the controls every 128
// frames, so smooth what would click (si.smoo). process takes the sound,
// one channel or two, and gives one or two; a mono process is run on each
// channel.

import("stdfaust.lib");

gain = hslider("gain_db", 0, -24, 24, 0.1) : ba.db2linear : si.smoo;
tone_set = hslider("tone_hz", 20000, 200, 20000, 1);
tone = tone_set : si.smoo;
warmth = hslider("warmth_percent", 0, 0, 100, 1) / 100 : si.smoo;
mode = nentry("mode", 0, 0, 1, 1);
invert = checkbox("invert");

// At the top of its range the tone is open; below, a one-pole lowpass.
lowpass = _ <: _, fi.lowpass(1, tone) : select2(tone_set < 20000);
drive = 1 + 3 * warmth;
warm(x) = (1 - warmth) * x + warmth * ma.tanh(x * drive) / ma.tanh(drive);
sign = 1 - 2 * invert;
// mono is the second choice: each channel becomes the two's mean.
narrow(l, r) = l + (m - l) * mode, r + (m - r) * mode with { m = 0.5 * (l + r); };
channel = *(gain) : lowpass : warm : *(sign);

process = narrow : channel, channel;
