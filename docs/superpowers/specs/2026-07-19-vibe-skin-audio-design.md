# Vibe Skin Audio Design

## Goal

Add an optional audio layer to Vibe skins so a skin package can provide event sounds and ambient sounds, while users can disable all skin audio from the appearance dialog.

## Scope

- Extend standard Vibe skin packages with a top-level `audio` block.
- Import audio assets from `.aiskin`/`.zip` packages as safe `data:audio/...` URLs.
- Add a persisted user preference for enabling or disabling skin audio.
- Wire the starship cockpit skin to play sounds for agent selection, hologram interaction, and radar ambience.
- Keep audio optional. Existing skins without `audio` behave exactly as they do now.

## Skin Package Schema

The top-level `audio` object supports:

- `enabled`: optional boolean, defaults to `true`.
- `volume`: optional number from `0` to `1`, defaults to `0.5`.
- `events`: optional map from supported event names to audio asset paths or data URLs.
- `ambient`: optional array of background audio entries.

Supported event names for the first version:

- `agentSelect`: plays when the user selects an agent in the launch panel.
- `hologramInteract`: plays when the user presses or drags a skin hologram figure.
- `radarPulse`: reserved for explicit radar interactions or timed pulse playback.

Ambient entries contain:

- `id`: stable identifier used for cleanup and testing.
- `src`: audio asset path or data URL.
- `loop`: optional boolean, defaults to `true`.
- `intervalMs`: optional number. When present, the sound is replayed on that cadence instead of relying on native loop.
- `volume`: optional per-sound multiplier from `0` to `1`.

## Import Rules

- Only package-relative `.mp3`, `.ogg`, and `.wav` audio assets are accepted.
- Data URLs must start with `data:audio/mpeg`, `data:audio/mp3`, `data:audio/ogg`, or `data:audio/wav`.
- Remote URLs and absolute filesystem paths are rejected by omission.
- Audio assets count toward the existing package size limits.
- Invalid audio fields are ignored instead of failing the whole skin import.

## Runtime Behavior

- Skin audio starts muted unless the user preference is enabled and the active skin has audio.
- Ambient audio starts only after the first user interaction inside Vibe, matching browser/Tauri autoplay rules.
- Switching skins, disabling audio, leaving Vibe, or unmounting the screen stops all ambient sounds.
- Event sounds are short-lived `HTMLAudioElement` instances cloned from the source URL.
- Volume is `skin.audio.volume * sound.volume`, capped to `0..1`.

## Appearance Dialog

Add a single persisted control:

- Label: `皮肤音效` / `Skin sound effects`.
- It enables or disables all skin event and ambient audio.
- The setting is stored with the existing Vibe appearance preference.

## Starship Cockpit Defaults

The built-in starship skin declares audio metadata using package asset paths:

- `agentSelect`: weapon/console switch sound.
- `hologramInteract`: hologram tap/drag activation sound.
- Ambient `radar`: low-volume radar loop or pulse.

The implementation may use tiny bundled generated audio files if no external source assets exist in the repository.

## Testing

- Skin library tests cover audio manifest normalization, zip audio asset conversion, invalid audio rejection, and stored appearance audio preference.
- Vibe screen tests cover the appearance dialog control and event wiring using mocked audio playback.

## Out Of Scope

- No global volume mixer beyond the single skin-audio switch.
- No per-event UI configuration in the first version.
- No remote audio loading.
- No JavaScript hooks or arbitrary skin code.
