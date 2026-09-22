"""Automation envelopes evaluated at absolute timeline frames.

A lane holds its first value before its first point and its last value after its
last point. A point's curve shapes the segment that follows it: linear moves in the
parameter's domain (log for frequencies and q, so sweeps move in equal ratios per
beat), hold keeps the value until the next point. Two points at one position jump
there. Values change exactly at their frames; nothing is smoothed.
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
        """The static value, or one value per frame of the timeline."""
        return static if env is None else env.at(np.arange(total))

    def channel_value(self, field, static, total):
        return self.value(self.channel.get(field), static, total)

    def send_value(self, to, static, total):
        return self.value(self.sends.get(to), static, total)


def amplitude(db):
    """dB to linear gain: a scalar, or a column that scales stereo frames."""
    if np.ndim(db) == 0:
        return 10 ** (db / 20)
    return (10 ** (db / 20))[:, None]
