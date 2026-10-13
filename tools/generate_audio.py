#!/usr/bin/env python3
"""Synthesises CyberHeist's sound effects and background music (story #111).

Every sound in godot/assets/audio/ is made by this script from plain maths,
with no samples or recordings, so there is nothing third-party to license or
credit. Run it again to rebuild them:

    python3 tools/generate_audio.py

It needs Python 3 and numpy, and writes 16-bit mono WAV files at 22.05 kHz,
which keeps the whole set to a few hundred kilobytes. The random parts are
seeded, so the same script always writes the same bytes.

Godot picks the files up on its next import. The music loops because its
.import file sets edit/loop_mode=2 (forward); regenerating the WAV keeps that
file as it is.
"""

from pathlib import Path
import wave

import numpy as np

SAMPLE_RATE = 22050
SEED = 111

REPOSITORY = Path(__file__).resolve().parent.parent
SFX_DIRECTORY = REPOSITORY / "godot" / "assets" / "audio" / "sfx"
MUSIC_DIRECTORY = REPOSITORY / "godot" / "assets" / "audio" / "music"

# The music loop: 4 bars of 4/4 at 120 bpm is exactly 8 seconds.
MUSIC_BPM = 120
MUSIC_BEATS = 16

# Note frequencies in hertz.
E2 = 82.41
F2 = 87.31
A1 = 55.0
A2 = 110.0
B_FLAT_2 = 116.54
E5 = 659.25
F5 = 698.46


# --- building blocks -------------------------------------------------------


def times(duration: float) -> np.ndarray:
    return np.arange(int(duration * SAMPLE_RATE)) / SAMPLE_RATE


def phase_of(frequency: np.ndarray) -> np.ndarray:
    """The running phase of an oscillator whose pitch may change over time."""
    return np.cumsum(2.0 * np.pi * frequency / SAMPLE_RATE)


def sine(phase: np.ndarray) -> np.ndarray:
    return np.sin(phase)


def square(phase: np.ndarray) -> np.ndarray:
    return np.sign(np.sin(phase))


def saw(phase: np.ndarray) -> np.ndarray:
    return 2.0 * ((phase / (2.0 * np.pi)) % 1.0) - 1.0


def exponential_sweep(start: float, end: float, duration: float) -> np.ndarray:
    """A pitch gliding from start to end, evenly in musical terms."""
    t = times(duration)
    return start * (end / start) ** (t / duration)


def decay(duration: float, time_constant: float) -> np.ndarray:
    return np.exp(-times(duration) / time_constant)


def fade_edges(signal: np.ndarray, fade_seconds: float = 0.004) -> np.ndarray:
    """Ramps the first and last few milliseconds so a sound never clicks."""
    length = min(int(fade_seconds * SAMPLE_RATE), len(signal) // 2)
    ramp = np.linspace(0.0, 1.0, length)
    shaped = signal.copy()
    shaped[:length] *= ramp
    shaped[-length:] *= ramp[::-1]
    return shaped


def low_pass(signal: np.ndarray, cutoff: float) -> np.ndarray:
    """A one-pole low-pass filter: gentle, cheap, and good enough here."""
    coefficient = 1.0 - np.exp(-2.0 * np.pi * cutoff / SAMPLE_RATE)
    filtered = np.empty_like(signal)
    state = 0.0
    for index, sample in enumerate(signal):
        state += coefficient * (sample - state)
        filtered[index] = state
    return filtered


def high_pass(signal: np.ndarray, cutoff: float) -> np.ndarray:
    return signal - low_pass(signal, cutoff)


def normalise(signal: np.ndarray, peak: float) -> np.ndarray:
    return signal * (peak / np.max(np.abs(signal)))


def place(target: np.ndarray, sound: np.ndarray, start_seconds: float) -> None:
    """Mixes a sound into a longer buffer, cutting it off at the buffer's end."""
    start = int(start_seconds * SAMPLE_RATE)
    end = min(start + len(sound), len(target))
    target[start:end] += sound[: end - start]


def write_wav(path: Path, signal: np.ndarray) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    samples = np.clip(signal, -1.0, 1.0)
    pcm = (samples * 32767.0).astype("<i2")
    with wave.open(str(path), "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(SAMPLE_RATE)
        output.writeframes(pcm.tobytes())
    print(f"wrote {path.relative_to(REPOSITORY)} ({path.stat().st_size} bytes)")


# --- sound effects, one per card type --------------------------------------


def attack_sound(rng: np.random.Generator) -> np.ndarray:
    """A hard downward zap with a burst of static at the front: something hit."""
    duration = 0.28
    zap = square(phase_of(exponential_sweep(1400.0, 180.0, duration)))
    zap = low_pass(zap, 3500.0) * decay(duration, 0.09)
    static = rng.uniform(-1.0, 1.0, len(zap)) * decay(duration, 0.02)
    return fade_edges(normalise(zap + 0.6 * static, 0.9))


def defence_sound(rng: np.random.Generator) -> np.ndarray:
    """A rising, humming swell: a shield coming up."""
    duration = 0.42
    t = times(duration)
    pitch = exponential_sweep(180.0, 420.0, duration)
    hum = sine(phase_of(pitch)) + 0.5 * sine(phase_of(pitch * 1.5))
    tremolo = 0.75 + 0.25 * np.sin(2.0 * np.pi * 30.0 * t)
    swell = np.minimum(t / 0.05, 1.0) * np.exp(-np.maximum(t - 0.2, 0.0) / 0.08)
    return fade_edges(normalise(hum * tremolo * swell, 0.9))


def skill_sound(rng: np.random.Generator) -> np.ndarray:
    """Two quick rising blips: a command executing."""
    blip_length = 0.09
    blips = np.zeros(int(0.22 * SAMPLE_RATE))
    for start, pitch in [(0.0, 880.0), (0.1, 1320.0)]:
        tone = square(phase_of(np.full(int(blip_length * SAMPLE_RATE), pitch)))
        tone = low_pass(tone, 5000.0) * decay(blip_length, 0.04)
        place(blips, fade_edges(tone), start)
    return fade_edges(normalise(blips, 0.8))


def recon_sound(rng: np.random.Generator) -> np.ndarray:
    """A sonar ping with one echo: something being scanned."""
    duration = 0.6
    ping_length = 0.35
    ping = sine(phase_of(np.full(int(ping_length * SAMPLE_RATE), 1500.0)))
    ping *= decay(ping_length, 0.08)
    click = sine(phase_of(np.full(int(0.01 * SAMPLE_RATE), 3000.0)))

    sound = np.zeros(int(duration * SAMPLE_RATE))
    place(sound, ping, 0.0)
    place(sound, 0.5 * click, 0.0)
    place(sound, 0.35 * ping, 0.2)
    return fade_edges(normalise(sound, 0.85))


def social_engineering_sound(rng: np.random.Generator) -> np.ndarray:
    """A sly, voice-like murmur that bends downward, like a smooth excuse."""
    duration = 0.45
    t = times(duration)
    vibrato = 1.0 + 0.03 * np.sin(2.0 * np.pi * 6.0 * t)
    pitch = exponential_sweep(210.0, 140.0, duration) * vibrato
    carrier_phase = phase_of(pitch)
    modulation_depth = 2.0 + 1.5 * np.sin(2.0 * np.pi * 3.0 * t)
    voice = np.sin(carrier_phase + modulation_depth * np.sin(carrier_phase * 1.5))
    voice = low_pass(voice, 1800.0)
    shape = np.minimum(t / 0.04, 1.0) * np.exp(-np.maximum(t - 0.15, 0.0) / 0.1)
    return fade_edges(normalise(voice * shape, 0.85))


SOUND_EFFECTS = {
    "attack": attack_sound,
    "defence": defence_sound,
    "skill": skill_sound,
    "recon": recon_sound,
    "social_engineering": social_engineering_sound,
}


# --- background music ------------------------------------------------------


def loop_frequency(frequency: float, loop_seconds: float) -> float:
    """Nudges a pitch so it completes whole cycles over the loop, so held
    tones join up with no click where the loop wraps round."""
    return round(frequency * loop_seconds) / loop_seconds


def kick(duration: float = 0.18) -> np.ndarray:
    pitch = exponential_sweep(70.0, 38.0, duration)
    return sine(phase_of(pitch)) * decay(duration, 0.06)


def bass_note(frequency: float, duration: float) -> np.ndarray:
    tone = square(phase_of(np.full(int(duration * SAMPLE_RATE), frequency)))
    return fade_edges(tone * decay(duration, duration * 0.4))


def tick(rng: np.random.Generator, duration: float = 0.04) -> np.ndarray:
    return rng.uniform(-1.0, 1.0, int(duration * SAMPLE_RATE)) * decay(duration, 0.008)


def heist_loop(rng: np.random.Generator) -> np.ndarray:
    """Eight seconds of low, pulsing tension that loops without a seam.

    A detuned drone sits under a pulsing eighth-note bass that sinks through
    the four bars and ends on a semitone rub against the drone. A heartbeat
    kick, ticking hats and a faint high semitone cluster keep it uneasy.
    """
    beat = 60.0 / MUSIC_BPM
    loop_seconds = beat * MUSIC_BEATS
    t = times(loop_seconds)

    drone = saw(phase_of(np.full(len(t), loop_frequency(A1, loop_seconds))))
    drone += saw(phase_of(np.full(len(t), loop_frequency(A1 * 1.003, loop_seconds))))
    swell = 0.7 + 0.3 * np.sin(2.0 * np.pi * t / loop_seconds)
    drone *= swell

    eighth = beat / 2.0
    bar_roots = [A2, A2, F2, E2]
    bass = np.zeros(len(t))
    for bar, root in enumerate(bar_roots):
        for step in range(8):
            frequency = root
            # The last two eighths rub a semitone against the drone, the
            # most unsettled point in the loop, right before it starts again.
            if bar == 3 and step >= 6:
                frequency = B_FLAT_2
            accent = 1.0 if step % 2 == 0 else 0.7
            start = (bar * 8 + step) * eighth
            place(bass, accent * bass_note(frequency, eighth * 0.9), start)

    beats = np.zeros(len(t))
    for beat_index in range(MUSIC_BEATS):
        # A heartbeat: a strong hit on the beat and a softer one just after.
        if beat_index % 2 == 0:
            place(beats, kick(), beat_index * beat)
            place(beats, 0.6 * kick(), beat_index * beat + beat * 0.3)

    hats = np.zeros(len(t))
    for sixteenth in range(MUSIC_BEATS * 4):
        loudness = 0.9 if sixteenth % 4 == 2 else 0.4
        place(hats, loudness * tick(rng), sixteenth * beat / 4.0)

    cluster_swell = 0.5 - 0.5 * np.cos(2.0 * np.pi * t / loop_seconds)
    cluster = sine(phase_of(np.full(len(t), loop_frequency(E5, loop_seconds))))
    cluster += sine(phase_of(np.full(len(t), loop_frequency(F5, loop_seconds))))
    cluster *= cluster_swell

    # The filters below remember their last output, so filtering a single pass
    # would leave a step where the loop wraps. Filtering two passes back to
    # back and keeping the second means its start follows on from its own end.
    def filtered(signal: np.ndarray, filter_function, cutoff: float) -> np.ndarray:
        doubled = np.concatenate([signal, signal])
        return filter_function(doubled, cutoff)[len(signal):]

    mix = (
        0.30 * filtered(drone, low_pass, 260.0)
        + 0.32 * filtered(bass, low_pass, 600.0)
        + 0.55 * beats
        + 0.10 * filtered(hats, high_pass, 4000.0)
        + 0.05 * cluster
    )
    return normalise(mix, 0.75)


def main() -> None:
    rng = np.random.default_rng(SEED)
    for cue, make_sound in SOUND_EFFECTS.items():
        write_wav(SFX_DIRECTORY / f"{cue}.wav", make_sound(rng))
    write_wav(MUSIC_DIRECTORY / "heist_loop.wav", heist_loop(rng))


if __name__ == "__main__":
    main()
