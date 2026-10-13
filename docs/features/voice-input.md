# Speaking to the agent — proposed October 13, 2026

Status: proposed, not built. Backlog: Next, Speaking to the agent. Builds
into the field of [the agent panel's look](agent-panel-look.md). What the
Mac offers was read from Apple's documentation and forums on October 13,
2026 and nothing was run; see What was checked. Nothing here touches the
agent's engine: the words are text in the field, whichever engine the
panel runs (D114, [the agent's engine](agent-engine.md)).

## What

A microphone button in the panel's field. A click starts listening, the
words appear in the field as they are recognized, a second click or Return
stops, and the person reads, fixes and sends the text as any message. The
recognition runs on the Mac; nothing the person says leaves it. The person
asked: "add the microphone transcription button so I can click it, it will
convert my voice to text and then I can send the resulting text."

## Why

A request about a sound is easier said than typed while the song plays and
the hands are on the keys or the mouse, and both chat apps have the button.
Text that the person sends, not speech the agent hears, keeps the message a
message: the panel shows it, the record keeps it, and the agent reads what
the person meant to send.

## What was checked

| Read, October 13, 2026 | Says |
|---|---|
| Apple's Speech framework | `SFSpeechRecognizer` is on macOS since 10.15, with `SFSpeechAudioBufferRecognitionRequest` for live audio from `AVAudioEngine`'s input node and partial results as they come; `supportsOnDeviceRecognition` says whether the locale can be recognized on the Mac and `requiresOnDeviceRecognition` holds it there. Server recognition has a limit of about a minute a request and a daily quota; on-device has neither |
| The same, on what an app must declare | `NSSpeechRecognitionUsageDescription` and `NSMicrophoneUsageDescription` in `Info.plist`, or the system ends the app at the first use; `SFSpeechRecognizer.requestAuthorization` and the microphone's own question, each answered once and kept in System Settings › Privacy |
| Apple's forums on the hardened runtime | An app signed with `--options runtime`, as `build.sh` signs with a Developer ID, needs the `com.apple.security.device.audio-input` entitlement to open the microphone |
| WWDC 2025 | `SpeechAnalyzer` and `SpeechTranscriber` replace `SFSpeechRecognizer` on macOS 26: on-device, long-form, faster, and its model downloaded by the system. The app runs on macOS 14 (`LSMinimumSystemVersion`), and this Mac is on 14.8 |

## Design

**The button.** A microphone in the field, beside `+`. A click asks the
two permissions the first time, then listens: the button turns the person's
blue with a ring that follows the level, and the field shows the words as
they are recognized, the uncertain tail in the faint color until it firms
up. A click, Return or a pause of a few seconds stops; the text stays in
the field, the caret at its end, and Return sends it. Escape while
listening stops and takes the spoken words out. With text already in the
field, the words go on after it.

**On the Mac only.** `requiresOnDeviceRecognition` is set, for the
system's locale. Where the Mac cannot recognize that locale on the device,
the button says so in its help and does not listen; whether to offer
Apple's server then is open question 1. The concept's statement of what
leaves the machine stays true.

**The audio.** `AVAudioEngine` with a tap on its input node feeds the
request; the app's own engine plays the song through Core Audio apart from
it, so the song keeps playing while the person speaks. The microphone hears
the speakers too; a recognizer handles a song under a voice less well than
silence, and whether to lower the song's level while listening is open
question 2.

**The bundle.** `Info.plist` gains the two usage descriptions. `build.sh`
gains `AAW.entitlements` with `com.apple.security.device.audio-input`,
passed to `codesign` for both the ad hoc and the Developer ID signing.

**Later: macOS 26.** When the app runs on macOS 26, `SpeechTranscriber`
behind the same protocol, chosen with `#available`; it has no limit on
length and reads a sentence better. Not in this item.

**Scripted runs and tests.** The recognizer is behind a protocol the panel
owns, `Transcriber`, with the real one over `SFSpeechRecognizer` and a
stand-in that yields a file's words at intervals; `AAW_SPEECH=FILE` names
the file for a scripted run, as `AAW_AGENT` names a program. A scripted
click on the button with the stand-in fills the field and a picture shows
it; a test holds the field's text to the file's, the partial tail's color,
Escape's clearing and Return's sending. The real recognizer cannot be
driven by a script: the permission questions wait for a person.

## Done when

- The button asks the two permissions once, listens, and the words of a
  sentence spoken in the system's locale are in the field within a second
  of their being said, with the tail firming up.
- A click, Return and a pause each stop it; Escape clears what was spoken;
  Return sends what is there.
- Nothing leaves the Mac: the recognizer refuses to run without on-device
  support rather than falling back.
- The app signed with the hardened runtime opens the microphone.
- The person has spoken a request while the song played and sent it.

## Open questions

1. **A locale the Mac cannot recognize on the device.** Say so and do
   nothing, as proposed, or offer Apple's server with a plain word that the
   audio leaves the Mac.
2. **The song under the voice.** Lower the song's level by a set amount
   while listening, as a phone ducks music for a call, or leave it to the
   recognizer.
3. **Hold to talk.** A press held is the other gesture; a click that
   toggles is proposed since the hands are elsewhere.
4. **Punctuation.** The recognizer adds none; "comma" and "period" are
   words. Whether to add a period at a pause, or leave the text as heard
   for the person to fix.
5. **The spoken words sent at once.** Both chat apps have a second mode
   that sends when the person stops speaking. Not proposed; the person
   reads first.
