"""Automation envelopes evaluated at absolute timeline frames.

A lane holds its first value before its first point and its last value after its
last point. A point's curve shapes the segment that follows it: linear moves in the
parameter's domain (log for frequencies and q, so sweeps move in equal ratios per
beat), hold keeps the value until the next point. Two points at one position jump
there. Values change exactly at their frames; nothing is smoothed. A lane whose
points share one value is that value, exactly, for the whole song.
"""

from __future__ import annotations
import numpy as np
from .model import Lane, Project, frame, target


class Envelope:
    def __init__(self, lane: Lane, domain: str, tempo: float, rate: int):
        self.frames = np.array([frame(p.at, tempo, rate) for p in lane.points])
        values = np.array([p.value for p in lane.points], dtype=np.float64)
        self.log = domain == "log"
        self.values = np.log(values) if self.log else values
        self.hold = np.array([p.curve == "hold" for p in lane.points])
        first = lane.points[0].value
        self.constant = first if all(p.value == first for p in lane.points) else None

    def at(self, frames) -> np.ndarray:
        """Values at the given timeline frames; frames may precede zero."""
        frames = np.asarray(frames)
        last = len(self.frames) - 1
        # side="right": after a jump the later of two coincident points applies.
        k = np.searchsorted(self.frames, frames, side="right") - 1
        before = k < 0
        k = np.clip(k, 0, last)
        following = np.minimum(k + 1, last)
        span = self.frames[following] - self.frames[k]
        t = (frames - self.frames[k]) / np.maximum(span, 1)
        start, end = self.values[k], self.values[following]
        v = np.where(self.hold[k] | (k == last), start, start + (end - start) * t)
        v = np.where(before, self.values[0], v)
        return np.exp(v) if self.log else v

    def span(self, start: int, count: int) -> np.ndarray:
        """Values at count consecutive timeline frames from start, equal to at().

        Segments are filled in place in the one output array, so memory stays at
        one float per frame however long the span is.
        """
        out = np.arange(start, start + count, dtype=np.float64)
        f, v = self.frames, self.values
        edges = np.clip(f - start, 0, count)  # where each point's frame falls in out
        out[: edges[0]] = v[0]
        out[edges[-1] :] = v[-1]
        # Only the segments overlapping the span: first the one containing start.
        first = max(0, np.searchsorted(f, start, side="right") - 1)
        stop = min(len(f) - 1, np.searchsorted(f, start + count))
        for k in range(first, stop):
            segment = out[edges[k] : edges[k + 1]]
            if self.hold[k]:
                segment[:] = v[k]
            else:
                segment -= f[k]
                segment /= max(f[k + 1] - f[k], 1)
                segment *= v[k + 1] - v[k]
                segment += v[k]
        return np.exp(out, out=out) if self.log else out


class Automation:
    """A track's, return's or the master's lanes, grouped by what they drive."""

    def __init__(self, owner, project: Project):
        s = project.session
        self.channel, self.sends, self.effects = {}, {}, {}
        self.params = [lane.param for lane in owner.automation]
        for lane in owner.automation:
            t = target(owner, lane.param)
            env = Envelope(lane, t.domain, s.tempo, s.sample_rate)
            if t.kind == "channel":
                self.channel[t.field] = env
            elif t.kind == "send":
                self.sends[t.send] = env
            else:
                self.effects.setdefault(t.effect, {})[t.name] = env

    @staticmethod
    def value(env, static, total):
        """The static value, a constant lane's value, or one value per frame."""
        if env is None:
            return static
        return env.span(0, total) if env.constant is None else env.constant

    def channel_value(self, field, static, total):
        return self.value(self.channel.get(field), static, total)

    def send_value(self, to, static, total):
        return self.value(self.sends.get(to), static, total)


def amplitude(db):
    """dB to linear gain, for a scalar or one value per frame."""
    return 10 ** (db / 20)


def per_frame(v):
    """A scalar or one value per frame, shaped to scale stereo frames."""
    return np.reshape(v, (-1, 1))
