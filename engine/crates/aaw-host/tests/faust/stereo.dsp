// A stereo test effect: a gain, a lowpass, a choice that scales the right
// channel and a bypass, with no smoothing so each is exact.
import("stdfaust.lib");
gain = hslider("gain_db", 0, -60, 12, 0.1) : ba.db2linear;
cutoff = hslider("Cutoff Hz[unit:Hz][scale:log]", 20000, 20, 20000, 1);
bypass = checkbox("bypass");
mode = nentry("mode", 0, 0, 2, 1);
channel = _ <: (fi.lowpass(2, cutoff) : *(gain)), _ : select2(bypass);
process = channel, (channel : *(1 + mode));
