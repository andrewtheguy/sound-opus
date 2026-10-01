# Repository instructions

- Strict no backward-compatibility or legacy paths no matter what.
- One crate, `desktop-opus`: the one place a desktop's sound is coded as Opus
  for the wlshare daemon and the remotex gateway, which each pin it by a release
  tag of this repository. How a block becomes a packet, what the encoder is set
  to, which rates it takes and what a decoder is told change here and reach them
  as a pin bump. Nothing about a wire belongs here: how a packet is framed in a
  message, how a stream's shape is agreed, what a sample's bytes are and when
  the rate moves belong to each user.
- libopus is linked statically, from the prebuilt archive `opus-prebuilt`'s sys
  crate downloads: building compiles no C and needs no libopus installed, and
  nothing is loaded at run time.
- After changes run `cargo test` and `cargo clippy --all-targets -- -D warnings`.
  The encoder gets an independent decoder in its tests: FFmpeg's own, not its
  wrapper around libopus, run as `ffmpeg`, which the tests need on the path
  (`ffmpeg` on Debian and Ubuntu, `brew install ffmpeg` on macOS).
- Do not run `cargo fmt`. Errors are `thiserror`: every caller branches on them.
- A release is the version in `Cargo.toml` bumped and tagged `v<version>` on
  `main`; users pin the tag.
