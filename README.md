# sound-opus

Opus for a desktop's sound, as [wlshare](https://github.com/andrewtheguy/wlshare)
and the [remotex](https://github.com/andrewtheguy/remotex) gateway both code it.

- `Stream`: what a stream is before any sound — the rate and the channels — and
  with them the block, the frames of samples in every packet, twenty
  milliseconds.
- `Encoder`: one Opus packet of every block, from 16-bit or float samples, at a
  rate its owner may move while it runs.
- `Stream::head`: the `OpusHead` a decoder is configured from, which nothing
  sends: everything in it follows from the stream.
- `walk::BitrateWalk`: the rate a link will bear, walked under the configured
  one — down while sending blocks, back up while the link keeps up, to a floor
  fixed here — and whether the link is behind, for a sender with silence to
  shed. Its one setting is whether it walks at all. The same walk for a stream
  coded by the gateway and for wlshare's, which the gateway tells the rate.

libopus is linked statically, from the prebuilt archive
[libopus-prebuilt](https://github.com/andrewtheguy/libopus-prebuilt) publishes,
so nothing of Opus is compiled to build and nothing is installed to run.

Use it by release tag:

```toml
sound-opus = { git = "https://github.com/andrewtheguy/sound-opus", tag = "v0.0.4" }
```
