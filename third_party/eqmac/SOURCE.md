# eqMac preset source

Upstream: https://github.com/bitgapp/eqMac
Commit: 04e5a3a9bd3a65f2b5105cf54a76d2c72a1d00d7
Preset blob SHA: 5639db822e7f375db0d772bc3d4e53751b235f4f
Equalizer blob SHA: e638d71e20b3e26fcc003934faa70103fa636a98
License blob SHA: 853bacdfe31d7fe0eb29ee7f23fe9a8be9249f71

The two Swift files are unmodified source snapshots used to generate Maris built-in preset data and to document the source DSP semantics. They retain their upstream copyright notices. Maris converts eqMac's 0.5-octave AVAudioUnitEQ parametric bandwidth into sample-rate-aware RBJ Q values and computes its own safety headroom. Those conversions are modifications made by Maris; the source global gain remains 0 dB.
