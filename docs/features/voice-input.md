# Speaking to the agent — implemented October 13, 2026

A microphone in the field of [the agent panel](agent-panel.md): a click
listens, the words appear in the field as they are recognized on the Mac,
and the person reads, fixes and sends them as any message. Asked for by
the person (D113); the choices made in building it are D118. Nothing here
touches the agent's engine: the words are text in the field, whichever
engine the panel runs (D114, [the agent's engine](agent-engine.md)). What
the real recognizer did on this Mac is under What was tried: it refused,
since the Mac had no language for on-device recognition.

## What

The microphone beside `+` at the field's right. A click asks the two
permissions the first time, then listens: the symbol fills in the person's
color with a ring that follows the microphone's level, and the words go
into the field after whatever is typed, the words still uncertain in the
faint color until the recognizer firms them up. A click, Return, typing or
a pause of a few seconds stops; the words stay in the field with the caret
at their end, and Return sends them. Escape while listening stops and takes
the spoken words out. The person asked: "add the microphone transcription
button so I can click it, it will convert my voice to text and then I can
send the resulting text."

## How it works

**The transcriber.** `AgentSpeech.swift` holds a protocol the panel owns,
`Transcriber`: whether it can listen and why not, `start` with three
callbacks, the whole text so far each time it changes and whether that is
the last word, the level, and the end, and `stop`. Two implement it.

| Transcriber | Chosen when | Does |
|---|---|---|
| `SpeechTranscriber` | Always, in the app | `SFSpeechRecognizer` for the system's locale over a tap on the input node of an `AVAudioEngine` of its own, so the song keeps playing through the app's engine; `requiresOnDeviceRecognition` set, partial results, punctuation added by the recognizer; the two permissions asked at the first click and kept by the system |
| `FileTranscriber` | `AAW_SPEECH=FILE` names a file | Yields the file's words one at a time, a tenth of a second apart, each with the whole so far, then the whole as final, the level alternating; for scripted runs and tests, as `AAW_AGENT` names a program |

**What the Mac must have.** The recognizer refuses to listen rather than
send audio off the Mac: when `supportsOnDeviceRecognition` is false for
the system's locale, the button's help and a click's refusal say so, and
that Dictation in System Settings › Keyboard downloads the language. With
Dictation off, no locale has it (What was tried). When the person has
refused the microphone or speech recognition in System Settings › Privacy,
the refusal says which. Two usage descriptions are in `Info.plist`, and
`AAW.entitlements`, passed to `codesign` for the app by `build.sh`, carries
`com.apple.security.device.audio-input` for the hardened runtime a
Developer ID signs with.

**The field while listening.** The panel keeps the draft as it was when
listening began, with a space after it when it ends in none, and sets the
draft to that followed by the recognizer's latest hypothesis each time one
arrives. The uncertain tail is the hypothesis past the words the previous
hypothesis also had, less the last word, which may still grow; `Transcribers.firmLength`
gives it, and the composer draws that range in the faint color as a
temporary attribute, as it draws the chip's white. A final result firms
everything.

**Stopping.** A click on the microphone and Return each stop, with the
words kept; Return while listening sends nothing, and the next Return
sends. A pause of three seconds after the last new word stops, or ten
seconds with nothing heard. Typing stops, keeping what was typed and
dropping what the recognizer still says. Escape stops and puts the draft
back as it was before listening. The real recognizer ends by itself when
it gives its final result; where it gives none after `stop`, the panel
ends it a second and a half later.

**Scripted runs.** `--listen` presses the microphone's button; with
`AAW_SPEECH=FILE` the field fills from the file, and `--wait` leaves time
for the words before the picture.

## Scripted runs and tests

Tests hold what firms up between two hypotheses, the stand-in reading a
file word by word and ending once, a stop mid-way, a missing file, the
stand-in chosen by the environment, the level of silence, full scale and
−25 dB, each refusal's words, the composer's faint tail with and without a
chip; and through the panel, over a transcriber the test speaks through:
the words after typed text with a space, the uncertain range after each
hypothesis and none after the final one, Return while listening stopping
and sending nothing and then sending, Escape leaving the typed text,
typing stopping it, a pause stopping it, the button stopping and starting
again, a refusal with its reason, and the file stand-in filling the field.

## What was tried

Scripted runs only, from the package's debug build with another AAW in
front, on a scratch project outside the checkout, with a file of eleven
words and one of sixty-six; nothing by hand and nothing spoken:

- `--listen` with the stand-in filled the field with the eleven words, Send
  in the person's color, in a picture; with the longer file the field grew
  to eight lines and scrolled.
- A picture a second into the longer file showed the microphone filled in
  the person's blue with its ring and the last word in the faint color.
- With the field clicked first: Escape a second in left the field empty;
  typing a word stopped listening and the word went on after the spoken
  ones; Return stopped, sent nothing, and a word typed after went on in the
  field.
- Two runs without the stand-in, by a mistake in the command, reached the
  real recognizer: the transport bar said that this Mac cannot recognize
  en_US speech on the device. A check with the Speech framework found
  `supportsOnDeviceRecognition` false for every English locale and
  `Dictation Enabled` off in the Mac's settings, which is where the
  language comes down from; so the refusal names Dictation. No permission
  was asked, since the recognizer refuses before asking, and nobody turned
  Dictation on or spoke.

## Limits

- The recognizer is on the device only; where the Mac has no language for
  it, the button refuses rather than offering Apple's server (open
  question 1 of the design, answered as proposed; the server stays in
  Later).
- The song is not lowered while the person speaks (open question 2).
- A click toggles; there is no hold to talk (open question 3).
- Punctuation is the recognizer's, `addsPunctuation`, where its model adds
  it; nothing is added at a pause (open question 4).
- The spoken words are never sent by themselves (open question 5).
- `SpeechTranscriber` of macOS 26 is not used; the app runs on macOS 14.
- The real recognizer cannot be driven by a script: the permission
  questions wait for a person.

## Open questions

1. **Three seconds.** Whether the pause that stops is the right length,
   or wants to be longer for someone thinking between phrases.
2. **Whether the ring reads as the level,** at 0.6 of the symbol's size at
   full scale, or wants to be a bar.
